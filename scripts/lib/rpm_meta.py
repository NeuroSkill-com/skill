#!/usr/bin/env python3
"""Read an RPM's header and payload listing without the `rpm` tool.

Why this exists: `scripts/verify-rpm-docker.sh` gives the real answer — it
installs the package on Fedora and lets `dnf` resolve the dependencies — but it
needs a working Docker, and the metadata half of those checks does not. This
parser runs anywhere Python does, so the version/arch/dependency assertions stay
available on a macOS dev box or when the Docker daemon is wedged.

It reads the header directly (the format is stable and documented) rather than
shelling out, and uses bsdtar for the payload listing, which macOS ships and
which understands the RPM container.

Usage:
    rpm_meta.py <file.rpm>                 # print metadata as JSON
    rpm_meta.py <file.rpm> --check x86_64  # assert the release invariants
"""

from __future__ import annotations

import json
import struct
import subprocess
import sys

# Header tags we care about (rpmtag.h).
TAG_NAME = 1000
TAG_VERSION = 1001
TAG_RELEASE = 1002
TAG_ARCH = 1022
TAG_LICENSE = 1014
TAG_REQUIRENAME = 1049

TYPE_STRING = 6
TYPE_STRING_ARRAY = 8
TYPE_I18NSTRING = 9

HEADER_MAGIC = b"\x8e\xad\xe8\x01"
LEAD_LEN = 96


def _read_header(buf: bytes, offset: int) -> tuple[dict[int, object], int]:
    """Parse one header section starting at `offset`; return (tags, end_offset)."""
    if buf[offset : offset + 4] != HEADER_MAGIC:
        raise ValueError(f"no header magic at offset {offset}")
    count, store_size = struct.unpack(">II", buf[offset + 8 : offset + 16])
    index_start = offset + 16
    store_start = index_start + count * 16

    tags: dict[int, object] = {}
    for i in range(count):
        tag, typ, data_off, cnt = struct.unpack(
            ">IIiI", buf[index_start + i * 16 : index_start + (i + 1) * 16]
        )
        base = store_start + data_off
        if typ in (TYPE_STRING, TYPE_I18NSTRING):
            end = buf.index(b"\x00", base)
            tags[tag] = buf[base:end].decode("utf-8", "replace")
        elif typ == TYPE_STRING_ARRAY:
            out, pos = [], base
            for _ in range(cnt):
                end = buf.index(b"\x00", pos)
                out.append(buf[pos:end].decode("utf-8", "replace"))
                pos = end + 1
            tags[tag] = out

    end_offset = store_start + store_size
    # Headers are padded to an 8-byte boundary.
    return tags, end_offset + (-end_offset % 8)


def read_metadata(path: str) -> dict:
    with open(path, "rb") as fh:
        buf = fh.read()

    # lead, then the signature header, then the real one.
    sig_start = LEAD_LEN
    _, after_sig = _read_header(buf, sig_start)
    tags, _ = _read_header(buf, after_sig)

    return {
        "name": tags.get(TAG_NAME),
        "version": tags.get(TAG_VERSION),
        "release": tags.get(TAG_RELEASE),
        "arch": tags.get(TAG_ARCH),
        "license": tags.get(TAG_LICENSE),
        "requires": tags.get(TAG_REQUIRENAME) or [],
    }


def read_payload(path: str) -> list[str]:
    """File list via bsdtar, which understands the rpm container."""
    try:
        out = subprocess.run(
            ["tar", "-tf", path], capture_output=True, text=True, check=True
        ).stdout
    except (subprocess.CalledProcessError, FileNotFoundError):
        return []
    return [line.lstrip("./") for line in out.splitlines() if line.strip()]


def rpm_vercmp_tilde_is_lower(version: str) -> bool | None:
    """Whether `version` marks a pre-release that sorts below its base version.

    Only the tilde rule is implemented, because that is the one this project
    keeps getting wrong: rpm sorts `~` below an absent segment, so 0.0.131~rc.30
    precedes 0.0.131, while any other separator (0.0.131.rc.30) sorts *after* it
    and makes an RC look newer than the release. Returns None when the version is
    not a pre-release at all.
    """
    if "rc" not in version:
        return None
    return "~" in version


def check(path: str, expect_arch: str) -> int:
    meta = read_metadata(path)
    files = read_payload(path)
    failures: list[str] = []
    passes: list[str] = []

    def assert_(cond: bool, ok_msg: str, bad_msg: str) -> None:
        (passes if cond else failures).append(ok_msg if cond else bad_msg)

    assert_(meta["name"] == "neuroskill", "name is neuroskill", f"name is {meta['name']!r}")
    assert_(
        meta["arch"] == expect_arch,
        f"arch is {expect_arch}",
        f"arch is {meta['arch']!r}, expected {expect_arch!r}",
    )
    assert_(
        bool(meta["license"]) and meta["license"] != "(none)",
        f"license present ({meta['license']})",
        "no License in metadata",
    )

    version = meta["version"] or ""
    assert_("-" not in version, "VERSION has no '-'", f"VERSION {version!r} contains '-', which rpm forbids")
    assert_("\\" not in version, "VERSION has no backslash", f"VERSION {version!r} contains a backslash")
    assert_("/" not in version, "VERSION has no '/'", f"VERSION {version!r} contains '/' (tilde-expansion bug)")

    tilde = rpm_vercmp_tilde_is_lower(version)
    if tilde is None:
        passes.append("not a pre-release; no tilde ordering to check")
    else:
        assert_(
            tilde,
            f"pre-release uses '~' so it sorts below its release ({version})",
            f"VERSION {version!r} is a pre-release without '~' — upgrade ordering inverts",
        )

    reqs = meta["requires"]
    assert_(
        any("libopenblas.so.0" in r for r in reqs),
        "auto-generated OpenBLAS soname requirement present",
        "no libopenblas.so.0 requirement — auto dep generation did not run",
    )
    assert_(
        "openblas" not in reqs,
        "no distro-specific bare 'openblas' requirement",
        "hard 'Requires: openblas' present — unsatisfiable where no such package exists",
    )

    for expected in (
        "usr/bin/neuroskill",
        "opt/neuroskill",
        "usr/share/applications/neuroskill.desktop",
    ):
        assert_(
            any(f == expected or f.startswith(expected + "/") for f in files),
            f"payload contains {expected}",
            f"payload missing {expected}",
        )

    print(f"  {meta['name']}-{version}-{meta['release']}.{meta['arch']}  ({len(files)} files)")
    for p in passes:
        print(f"  \033[32mPASS\033[0m  {p}")
    for f in failures:
        print(f"  \033[31mFAIL\033[0m  {f}", file=sys.stderr)
    print(f"\n  {len(passes)} passed, {len(failures)} failed")
    return 1 if failures else 0


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__)
        return 2
    path = argv[1]
    if "--check" in argv:
        return check(path, argv[argv.index("--check") + 1])
    meta = read_metadata(path)
    meta["files"] = read_payload(path)
    print(json.dumps(meta, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
