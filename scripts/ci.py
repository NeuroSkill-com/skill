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
import time
import traceback
import urllib.request
import urllib.error

# Current subcommand name — set by main() for log prefixes.
_CMD = "ci"


# ── Helpers ───────────────────────────────────────────────────────────────────

def log(msg):
    """Print with [command] prefix for easy grep in CI logs."""
    print(f"[{_CMD}] {msg}", flush=True)


def gh_output(key, value):
    """Append key=value to GITHUB_OUTPUT."""
    path = os.environ.get("GITHUB_OUTPUT")
    if path:
        with open(path, "a") as f:
            f.write(f"{key}={value}\n")
    log(f"output {key}={value}")


def gh_env(key, value):
    """Append key=value to GITHUB_ENV."""
    path = os.environ.get("GITHUB_ENV")
    if path:
        with open(path, "a") as f:
            f.write(f"{key}={value}\n")
    log(f"env {key}={value}")


def gh_path(directory):
    """Prepend directory to GITHUB_PATH."""
    path = os.environ.get("GITHUB_PATH")
    if path:
        with open(path, "a") as f:
            f.write(f"{directory}\n")
    log(f"path +={directory}")


def error(msg):
    print(f"::error::[{_CMD}] {msg}", flush=True)


def warning(msg):
    print(f"::warning::[{_CMD}] {msg}", flush=True)


def conf_version():
    """Read version from src-tauri/tauri.conf.json."""
    with open("src-tauri/tauri.conf.json") as f:
        for line in f:
            m = re.search(r'"version"\s*:\s*"([^"]+)"', line)
            if m:
                return m.group(1)
    raise RuntimeError("Could not find version in src-tauri/tauri.conf.json")


def run(cmd, **kwargs):
    """Run a command with logging. Returns CompletedProcess."""
    label = " ".join(str(c) for c in cmd) if isinstance(cmd, (list, tuple)) else str(cmd)
    log(f"$ {label}")
    result = subprocess.run(cmd, **kwargs)
    if result.returncode != 0 and kwargs.get("check"):
        # check=True raises on its own, but log first
        pass
    elif result.returncode != 0 and not kwargs.get("capture_output"):
        log(f"  (exit {result.returncode})")
    return result


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
        log(f"choco attempt {i} failed, retrying in {5*i}s...")
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


def cmd_self_test(_args):
    """Validate that ci.py itself is healthy: syntax, imports, all commands parse."""
    errors = []

    # 1. Verify every command function exists and is callable
    command_map = {
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
        "self-test": cmd_self_test,
        "dry-run-release": cmd_dry_run_release,
    }
    for name, fn in command_map.items():
        if not callable(fn):
            errors.append(f"  {name}: not callable")
        else:
            log(f"✓ {name}")

    # 2. Verify all workflow files reference only known commands
    import glob
    known = set(command_map.keys())
    for yml in glob.glob(".github/workflows/*.yml"):
        with open(yml) as f:
            for i, line in enumerate(f, 1):
                if "scripts/ci.py" in line:
                    # Extract the subcommand (word after ci.py)
                    m = re.search(r'scripts/ci\.py\s+([a-z][-a-z]*)', line)
                    if m and m.group(1) not in known:
                        errors.append(f"  {yml}:{i}: unknown command '{m.group(1)}'")

    # 3. Verify conf_version works
    try:
        v = conf_version()
        log(f"✓ conf_version() = {v}")
    except Exception as e:
        errors.append(f"  conf_version(): {e}")

    if errors:
        error("self-test failed:")
        for e in errors:
            print(e, flush=True)
        sys.exit(1)

    log(f"✓ all {len(command_map)} commands OK")


def cmd_dry_run_release(args):
    """Local release dry-run — builds everything but skips signing/notarization/upload.

    Exercises the same pipeline as the CI release workflow:
      1. resolve-version
      2. npm build
      3. cargo build (skill + skill-daemon)
      4. assemble .app bundle
      5. prepare changelog
      6. report artifact locations
    """
    target = args.target
    skip_compile = args.skip_compile

    # 1. Version
    log("Step 1/6: resolve version")
    version = conf_version()
    log(f"version = {version}")

    # 2. Frontend
    log("Step 2/6: build frontend")
    if not skip_compile:
        run(["npm", "run", "build"], check=True)
    else:
        log("(skipped)")

    # 3. Cargo build
    log("Step 3/6: cargo build")
    if not skip_compile:
        run(["cargo", "build", "--release", "--target", target,
             "-p", "skill", "--features", "custom-protocol"],
            check=True, cwd="src-tauri")
        run(["cargo", "build", "--release", "--target", target,
             "-p", "skill-daemon", "--features", "llm"],
            check=True, cwd="src-tauri")
    else:
        log("(skipped)")

    # 4. Assemble .app
    log("Step 4/6: assemble .app bundle")
    binary = f"src-tauri/target/{target}/release/skill"
    if not os.path.exists(binary):
        if skip_compile:
            warning(f"Binary not found at {binary} — run without --skip-compile first")
            log("(skipped — no binary)")
        else:
            error(f"Binary not found at {binary} after build")
            sys.exit(1)
    else:
        run(["bash", "scripts/assemble-macos-app.sh", target], check=True)

    # 5. Changelog
    log("Step 5/6: prepare changelog")
    changelog = "dry-run-release-notes.md"
    try:
        tag = f"v{version}"
        result = run(["git", "describe", "--tags", "--abbrev=0", f"{tag}^"],
                     capture_output=True, text=True)
        prev = result.stdout.strip() if result.returncode == 0 else ""
        commit_range = f"{prev}..HEAD" if prev else "HEAD~20..HEAD"
    except Exception:
        commit_range = "HEAD~20..HEAD"

    # Reuse our own prepare-changelog
    class FakeArgs:
        pass
    fa = FakeArgs()
    fa.version = version
    fa.output = changelog
    fa.range = commit_range
    cmd_prepare_changelog(fa)

    # 6. Report
    log("Step 6/6: summary")
    bundle_dir = f"src-tauri/target/{target}/release/bundle/macos"
    app = os.path.join(bundle_dir, "NeuroSkill.app")

    print("\n" + "=" * 60, flush=True)
    print(f"  Dry-run release: v{version} ({target})", flush=True)
    print("=" * 60, flush=True)
    if os.path.isdir(app):
        # Get app size
        total = sum(
            os.path.getsize(os.path.join(dp, f))
            for dp, _, fns in os.walk(app)
            for f in fns
        )
        print(f"  .app bundle:  {app}  ({total / 1_048_576:.1f} MB)", flush=True)
    else:
        print(f"  .app bundle:  (not found)", flush=True)
    print(f"  changelog:    {changelog}", flush=True)
    print(f"\n  To run:   open '{app}'", flush=True)
    print(f"  To sign:  APPLE_SIGNING_IDENTITY=... bash scripts/assemble-macos-app.sh {target}", flush=True)
    print("=" * 60 + "\n", flush=True)


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
    sub.add_parser("self-test", help="Validate ci.py commands parse correctly")

    dr = sub.add_parser("dry-run-release", help="Local release dry-run (no push/sign/notarize)")
    dr.add_argument("--target", default="aarch64-apple-darwin",
                    help="Rust target triple (default: aarch64-apple-darwin)")
    dr.add_argument("--skip-compile", action="store_true",
                    help="Skip cargo build (use existing binaries)")

    args = p.parse_args()

    global _CMD
    _CMD = args.command

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
        "self-test": cmd_self_test,
        "dry-run-release": cmd_dry_run_release,
    }

    t0 = time.monotonic()
    log(f"starting")
    try:
        commands[args.command](args)
    except SystemExit:
        raise  # preserve explicit exit codes
    except subprocess.CalledProcessError as e:
        elapsed = time.monotonic() - t0
        error(f"Command failed (exit {e.returncode}): {' '.join(str(c) for c in e.cmd)}")
        if e.stdout:
            print(f"[{_CMD}] stdout:\n{e.stdout.strip()}", flush=True)
        if e.stderr:
            print(f"[{_CMD}] stderr:\n{e.stderr.strip()}", flush=True)
        error(f"Failed after {elapsed:.1f}s")
        sys.exit(e.returncode or 1)
    except Exception as e:
        elapsed = time.monotonic() - t0
        error(f"{type(e).__name__}: {e}")
        traceback.print_exc()
        error(f"Failed after {elapsed:.1f}s")
        sys.exit(1)

    elapsed = time.monotonic() - t0
    log(f"done ({elapsed:.1f}s)")


if __name__ == "__main__":
    main()
