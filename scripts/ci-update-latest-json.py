#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Update the Tauri updater manifest (latest.json) for a platform.

Usage:
  python3 scripts/ci-update-latest-json.py \\
    --platform darwin-aarch64 \\
    --url     "https://github.com/.../app.tar.gz" \\
    --sig-file path/to/sig.txt \\
    --tag     v1.2.3 \\
    --version 1.2.3

Downloads existing latest.json from the GitHub release (if any),
merges the platform entry, writes it back, and optionally uploads.
"""

import argparse
import datetime
import json
import os
import subprocess
import sys


def main():
    p = argparse.ArgumentParser(description="Update Tauri latest.json manifest")
    p.add_argument("--platform", required=True, help="e.g. darwin-aarch64, linux-x86_64, windows-x86_64")
    p.add_argument("--url", required=True, help="Download URL for the update archive")
    p.add_argument("--sig-file", required=True, help="Path to the .sig file")
    p.add_argument("--tag", required=True, help="Git tag (e.g. v1.2.3)")
    p.add_argument("--version", required=True, help="Semver version (e.g. 1.2.3)")
    p.add_argument("--upload", action="store_true", help="Upload latest.json to the GitHub release")
    args = p.parse_args()

    with open(args.sig_file) as fh:
        signature = fh.read().strip()

    # Try to download existing manifest from the release
    dl = subprocess.run(
        ["gh", "release", "download", args.tag,
         "--pattern", "latest.json", "--output", "latest.json", "--clobber"],
        capture_output=True, text=True,
    )

    if dl.returncode == 0:
        with open("latest.json", encoding="utf-8-sig") as fh:
            manifest = json.load(fh)
    else:
        # Try to get release notes from tag annotation
        try:
            notes = subprocess.check_output(
                ["git", "tag", "-l", "--format=%(contents)", args.tag],
                text=True,
            ).strip()
        except Exception:
            notes = ""
        if not notes:
            notes = f"NeuroSkill\u2122 v{args.version}"

        pub_date = datetime.datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ")
        manifest = {
            "version": args.version,
            "notes": notes,
            "pub_date": pub_date,
            "platforms": {},
        }

    manifest["platforms"][args.platform] = {
        "url": args.url,
        "signature": signature,
    }

    with open("latest.json", "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, indent=2, ensure_ascii=False)
        fh.write("\n")

    print(f"Updated latest.json ({len(manifest['platforms'])} platform(s)):")
    for plat in sorted(manifest["platforms"]):
        print(f"  {plat}")

    if args.upload:
        subprocess.run(
            ["gh", "release", "upload", args.tag, "latest.json", "--clobber"],
            check=True,
        )
        print(f"✓ latest.json uploaded to release {args.tag}")


if __name__ == "__main__":
    main()
