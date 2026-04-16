#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Shared CI helpers — single cross-platform entry point.

Usage:
  python3 scripts/ci.py <command> [args...]

Commands:
  resolve-version          Resolve version from tauri.conf.json, validate tag
  verify-secrets V1 V2 ... Check env vars are non-empty
  prepare-changelog VER OUT [RANGE]  Generate release notes markdown
  update-latest-json ...   Merge platform entry into Tauri updater manifest
  discord-notify ...       Send Discord webhook notification
  download-llama PLAT TGT FEAT  Download + validate prebuilt llama libs
  import-apple-cert        Import .p12 into temporary keychain (macOS)
  validate-notarization    Check Apple notarization credentials (macOS)
  free-disk-space          Remove unused toolchains on Linux runners
  install-protoc-windows   Install protoc via choco or direct download (Windows)
"""

import argparse
import datetime
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request
import urllib.error


# ── Helpers ───────────────────────────────────────────────────────────────────

def gh_output(key, value):
    """Append key=value to GITHUB_OUTPUT."""
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        with open(path, "a") as f:
            f.write(f"{key}={value}\n")


def gh_env(key, value):
    """Append key=value to GITHUB_ENV."""
    path = os.environ.get("GITHUB_ENV")
    if path:
        with open(path, "a") as f:
            f.write(f"{key}={value}\n")


def gh_path(directory):
    """Prepend directory to GITHUB_PATH."""
    path = os.environ.get("GITHUB_PATH")
    if path:
        with open(path, "a") as f:
            f.write(f"{directory}\n")


def error(msg):
    print(f"::error::{msg}")


def warning(msg):
    print(f"::warning::{msg}")


def conf_version():
    """Read version from src-tauri/tauri.conf.json."""
    with open("src-tauri/tauri.conf.json") as f:
        for line in f:
            m = re.search(r'"version"\s*:\s*"([^"]+)"', line)
            if m:
                return m.group(1)
    raise RuntimeError("Could not find version in src-tauri/tauri.conf.json")


def run(cmd, **kwargs):
    """Run a command, return CompletedProcess."""
    return subprocess.run(cmd, **kwargs)


# ── Commands ──────────────────────────────────────────────────────────────────

def cmd_resolve_version(_args):
    version = conf_version()
    event = os.environ.get("GITHUB_EVENT_NAME", "")
    ref = os.environ.get("GITHUB_REF", "")
    ref_name = os.environ.get("GITHUB_REF_NAME", "")
    dry_run = os.environ.get("DRY_RUN", "false")

    is_release = "false"
    tag = ""

    if dry_run == "true":
        tag = f"v{version}"
        print(f"[dry-run] Using version from tauri.conf.json: {version}")
    elif event == "push" and ref.startswith("refs/tags/v"):
        is_release = "true"
        tag = ref_name
        tag_ver = tag.lstrip("v")
        if tag_ver != version:
            error(f"Tag version ({tag_ver}) does not match tauri.conf.json version ({version}).")
            error("Bump the version in src-tauri/tauri.conf.json and src-tauri/Cargo.toml, then re-tag.")
            sys.exit(1)

    for k, v in [("is_release", is_release), ("version", version), ("tag", tag), ("dry_run", dry_run)]:
        gh_output(k, v)
    gh_env("VERSION", version)
    gh_env("TAG", tag)
    print(f"✓ Version: {version} (release={is_release}, dry_run={dry_run})")


def cmd_verify_secrets(args):
    ok = True
    for var in args.names:
        if not os.environ.get(var):
            error(f"Secret '{var}' is empty or not set.")
            ok = False
    if not ok:
        sys.exit(1)
    print(f"✓ All required secrets are present ({len(args.names)} checked).")


def cmd_prepare_changelog(args):
    version, output = args.version, args.output
    commit_range = args.range or "HEAD~50..HEAD"

    # Extract changelog section
    section = ""
    try:
        with open("CHANGELOG.md") as f:
            in_section = False
            for line in f:
                if re.match(rf"^## \[{re.escape(version)}\]", line):
                    in_section = True
                    continue
                if in_section and re.match(r"^## \[", line):
                    break
                if in_section:
                    section += line
    except FileNotFoundError:
        pass

    # Contributors
    contributors = ""
    try:
        result = run(["git", "log", "--format=%aN", commit_range],
                     capture_output=True, text=True)
        seen = set()
        for name in result.stdout.strip().splitlines():
            name = name.strip()
            if name and name not in seen:
                seen.add(name)
                contributors += f"- {name}\n"
    except Exception:
        pass

    with open(output, "w") as f:
        f.write("## Changelog\n\n")
        f.write(section.strip() + "\n" if section.strip() else
                f"_No changelog section found for version {version} in CHANGELOG.md._\n")
        f.write("\n## Contributors\n\n")
        f.write(contributors if contributors else
                f"_No commit contributors found in range {commit_range}._\n")

    lines = sum(1 for _ in open(output))
    print(f"✓ Release notes written to {output} ({lines} lines)")


def cmd_update_latest_json(args):
    with open(args.sig_file) as f:
        signature = f.read().strip()

    # Try to download existing manifest
    dl = run(["gh", "release", "download", args.tag,
              "--pattern", "latest.json", "--output", "latest.json", "--clobber"],
             capture_output=True, text=True)

    if dl.returncode == 0 and os.path.exists("latest.json"):
        with open("latest.json", encoding="utf-8-sig") as f:
            manifest = json.load(f)
    else:
        try:
            notes = run(["git", "tag", "-l", "--format=%(contents)", args.tag],
                        capture_output=True, text=True).stdout.strip()
        except Exception:
            notes = ""
        if not notes:
            notes = f"NeuroSkill\u2122 v{args.version}"
        manifest = {
            "version": args.version,
            "notes": notes,
            "pub_date": datetime.datetime.utcnow().strftime("%Y-%m-%dT%H:%M:%SZ"),
            "platforms": {},
        }

    manifest.setdefault("platforms", {})[args.platform] = {
        "url": args.url,
        "signature": signature,
    }

    with open("latest.json", "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")

    plats = ", ".join(sorted(manifest["platforms"]))
    print(f"Updated latest.json ({len(manifest['platforms'])} platform(s): {plats})")

    if args.upload:
        run(["gh", "release", "upload", args.tag, "latest.json", "--clobber"], check=True)
        print(f"✓ latest.json uploaded to release {args.tag}")


def cmd_discord_notify(args):
    webhook = os.environ.get("DISCORD_WEBHOOK_URL")
    if not webhook:
        print("⚠ DISCORD_WEBHOOK_URL not set, skipping.")
        return

    try:
        commit = run(["git", "log", "-1", "--format=%s"],
                     capture_output=True, text=True).stdout.strip()[:200]
    except Exception:
        commit = ""

    color = 3066993 if args.status == "success" else 15158332
    if args.status == "success":
        desc = f"Build published and ready to download.\n\n**[Download v{args.version}]({args.release_url or args.run_url})**"
    else:
        desc = f"The build failed. Check the run for details.\n\n**[View failed run]({args.run_url})**"

    payload = json.dumps({"embeds": [{
        "title": args.title,
        "description": desc,
        "url": args.run_url or "",
        "color": color,
        "fields": [
            {"name": "Tag",      "value": f"`{args.tag}`",     "inline": True},
            {"name": "Version",  "value": f"`{args.version}`", "inline": True},
            {"name": "Platform", "value": args.platform,       "inline": True},
            {"name": "Actor",    "value": os.environ.get("GITHUB_ACTOR", "ci"), "inline": True},
            {"name": "Commit",   "value": commit,              "inline": False},
        ],
        "footer": {"text": os.environ.get("GITHUB_REPOSITORY", "")},
    }]}).encode()

    req = urllib.request.Request(webhook, data=payload,
                                headers={"Content-Type": "application/json"})
    try:
        urllib.request.urlopen(req)
    except Exception as e:
        print(f"⚠ Discord notification failed (non-fatal): {e}")


def cmd_download_llama(args):
    plat, target, feature = args.platform, args.target, args.feature
    url = f"https://github.com/eugenehp/llama-cpp-rs/releases/latest/download/llama-prebuilt-{plat}-{target}-q1-{feature}.tar.gz"
    tmp = os.environ.get("RUNNER_TEMP", tempfile.gettempdir())
    archive = os.path.join(tmp, f"llama-prebuilt-{plat}.tar.gz")
    dest = os.path.join(tmp, f"llama-prebuilt-{plat}")
    os.makedirs(dest, exist_ok=True)

    print(f"Downloading prebuilt llama: {url}")
    try:
        urllib.request.urlretrieve(url, archive)
    except Exception as e:
        print(f"[warn] prebuilt llama artifact unavailable ({e}); fallback to source build")
        return

    run(["tar", "-xzf", archive, "-C", dest], check=True)

    # Find root (may be nested one level)
    root = dest
    for check in ("lib", "lib64", "bin"):
        if os.path.isdir(os.path.join(root, check)):
            break
    else:
        subdirs = [d for d in os.listdir(dest) if os.path.isdir(os.path.join(dest, d))]
        root = os.path.join(dest, subdirs[0]) if subdirs else ""

    if not root or not os.path.isdir(root):
        print("[warn] prebuilt llama archive layout invalid; fallback to source build")
        return

    # Check for library files
    exts = {
        "macos": (".a", ".dylib"),
        "linux": (".a", ".so"),
        "windows": (".lib", ".dll"),
    }.get(plat, (".a", ".so", ".lib", ".dll"))
    has_libs = any(
        f.endswith(exts) for dirpath, _, files in os.walk(root) for f in files
    )
    if not has_libs:
        print("[warn] prebuilt llama archive contains no libs; fallback to source build")
        return

    # Validate metadata
    meta_path = os.path.join(root, "metadata.json")
    if os.path.exists(meta_path):
        with open(meta_path) as f:
            meta = json.load(f)
        if meta.get("target") != target or feature not in meta.get("features", ""):
            print(f"[warn] prebuilt metadata mismatch (target={meta.get('target')} features={meta.get('features')}); fallback to source build")
            return

    gh_env("LLAMA_PREBUILT_DIR", root)
    gh_env("LLAMA_PREBUILT_SHARED", "0")
    print(f"[ok] LLAMA_PREBUILT_DIR={root}")


def cmd_import_apple_cert(_args):
    tmp = os.environ["RUNNER_TEMP"]
    keychain = os.path.join(tmp, "app-signing.keychain-db")
    password = run(["openssl", "rand", "-base64", "32"],
                   capture_output=True, text=True, check=True).stdout.strip()

    gh_env("KEYCHAIN_PATH", keychain)
    gh_env("KEYCHAIN_PASSWORD", password)

    run(["security", "create-keychain", "-p", password, keychain], check=True)
    run(["security", "set-keychain-settings", "-lut", "21600", keychain], check=True)
    run(["security", "unlock-keychain", "-p", password, keychain], check=True)

    cert_path = os.path.join(tmp, "cert.p12")
    import base64
    with open(cert_path, "wb") as f:
        f.write(base64.b64decode(os.environ["APPLE_CERTIFICATE"]))

    run(["security", "import", cert_path, "-k", keychain,
         "-P", os.environ["APPLE_CERTIFICATE_PASSWORD"],
         "-T", "/usr/bin/codesign", "-T", "/usr/bin/security"], check=True)
    os.remove(cert_path)

    run(["security", "set-key-partition-list", "-S", "apple-tool:,apple:",
         "-s", "-k", password, keychain], check=True)
    run(["security", "list-keychains", "-d", "user", "-s", keychain, "login.keychain"], check=True)

    print(f"✓ Apple Developer certificate imported into {keychain}")


def cmd_validate_notarization(_args):
    print("Checking notarization credentials …")
    result = run(["xcrun", "notarytool", "history",
                  "--apple-id", os.environ["APPLE_ID"],
                  "--password", os.environ["APPLE_PASSWORD"],
                  "--team-id", os.environ["APPLE_TEAM_ID"],
                  "--output-format", "json"],
                 capture_output=True, text=True)
    output = result.stdout + result.stderr
    if '"history"' in output:
        print("✓ Notarization credentials are valid.")
    elif any(w in output.lower() for w in ("unauthorized", "invalid", "401")):
        error("Apple notarization credentials are invalid.")
        error("Generate a new app-specific password at")
        error("  https://appleid.apple.com → Sign-In and Security → App-Specific Passwords")
        error("Then update the APPLE_PASSWORD secret in: GitHub → Settings → Environments → Release → Secrets")
        sys.exit(1)
    else:
        warning("Could not verify notarization credentials (Apple API may be intermittent).")
        warning(f"Output: {output[:500]}")
        print("Proceeding — actual notarization will fail later if credentials are invalid.")


def cmd_free_disk_space(_args):
    dirs = [
        "/usr/local/lib/android", "/usr/share/dotnet", "/opt/ghc",
        "/usr/local/.ghcup", "/usr/local/share/powershell",
        "/usr/local/share/chromium", "/usr/share/swift",
        "/opt/hostedtoolcache/CodeQL",
    ]
    for d in dirs:
        if os.path.exists(d):
            run(["sudo", "rm", "-rf", d])
    run(["sudo", "docker", "image", "prune", "-af"], capture_output=True)
    run(["df", "-h", "/"])


def cmd_install_protoc_windows(_args):
    # Check if already installed
    if shutil.which("protoc"):
        run(["protoc", "--version"])
        return

    # Try Chocolatey (3 attempts)
    installed = False
    for i in range(1, 4):
        run(["choco", "install", "protoc", "--no-progress", "-y"], capture_output=True)
        if shutil.which("protoc"):
            installed = True
            break
        import time
        time.sleep(5 * i)

    if not installed:
        print("[warn] Chocolatey unavailable; falling back to direct download")
        ver = "25.3"
        url = f"https://github.com/protocolbuffers/protobuf/releases/download/v{ver}/protoc-{ver}-win64.zip"
        tmp = os.environ.get("RUNNER_TEMP", tempfile.gettempdir())
        zip_path = os.path.join(tmp, f"protoc-{ver}-win64.zip")
        dest = os.path.join(tmp, f"protoc-{ver}")

        urllib.request.urlretrieve(url, zip_path)
        import zipfile
        with zipfile.ZipFile(zip_path) as zf:
            zf.extractall(dest)

        bin_dir = os.path.join(dest, "bin")
        if not os.path.exists(os.path.join(bin_dir, "protoc.exe")):
            raise RuntimeError("protoc fallback install failed: protoc.exe not found")
        gh_path(bin_dir)
        os.environ["PATH"] = bin_dir + os.pathsep + os.environ["PATH"]
        print("[ok] Installed protoc via direct download")

    if not shutil.which("protoc"):
        raise RuntimeError("protoc installation failed after all attempts")
    run(["protoc", "--version"])


# ── CLI ───────────────────────────────────────────────────────────────────────

def main():
    p = argparse.ArgumentParser(description="CI helpers", prog="ci.py")
    sub = p.add_subparsers(dest="command", required=True)

    sub.add_parser("resolve-version")

    vs = sub.add_parser("verify-secrets")
    vs.add_argument("names", nargs="+")

    cl = sub.add_parser("prepare-changelog")
    cl.add_argument("version")
    cl.add_argument("output")
    cl.add_argument("range", nargs="?")

    uj = sub.add_parser("update-latest-json")
    uj.add_argument("--platform", required=True)
    uj.add_argument("--url", required=True)
    uj.add_argument("--sig-file", required=True)
    uj.add_argument("--tag", required=True)
    uj.add_argument("--version", required=True)
    uj.add_argument("--upload", action="store_true")

    dn = sub.add_parser("discord-notify")
    dn.add_argument("--status", required=True)
    dn.add_argument("--title", required=True)
    dn.add_argument("--version", required=True)
    dn.add_argument("--tag", required=True)
    dn.add_argument("--platform", required=True)
    dn.add_argument("--release-url", default="")
    dn.add_argument("--run-url", default="")

    dl = sub.add_parser("download-llama")
    dl.add_argument("platform")
    dl.add_argument("target")
    dl.add_argument("feature")

    sub.add_parser("import-apple-cert")
    sub.add_parser("validate-notarization")
    sub.add_parser("free-disk-space")
    sub.add_parser("install-protoc-windows")

    args = p.parse_args()
    commands = {
        "resolve-version": cmd_resolve_version,
        "verify-secrets": cmd_verify_secrets,
        "prepare-changelog": cmd_prepare_changelog,
        "update-latest-json": cmd_update_latest_json,
        "discord-notify": cmd_discord_notify,
        "download-llama": cmd_download_llama,
        "import-apple-cert": cmd_import_apple_cert,
        "validate-notarization": cmd_validate_notarization,
        "free-disk-space": cmd_free_disk_space,
        "install-protoc-windows": cmd_install_protoc_windows,
    }
    commands[args.command](args)


if __name__ == "__main__":
    main()
