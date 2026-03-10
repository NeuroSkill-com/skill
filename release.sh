#!/usr/bin/env bash
#
# release.sh — Build, sign, notarize, package, and upload a NeuroSkill™ release.
#
# This script handles the full macOS release pipeline:
#   1. Build the Tauri app in release mode
#   2. Code-sign the .app bundle with a Developer ID certificate
#   3. Notarize with Apple (staple the ticket into the bundle)
#   4. Package the signed .dmg
#   5. Sign the updater artifact with the Tauri Ed25519 key
#   6. Generate the updater JSON manifest
#   7. Upload everything to S3
#
# ══════════════════════════════════════════════════════════════════════
# REQUIRED ENVIRONMENT VARIABLES
# ══════════════════════════════════════════════════════════════════════
#
#   APPLE_SIGNING_IDENTITY
#       Full name (or SHA-1 hash) of the "Developer ID Application"
#       certificate installed in your macOS Keychain.
#
#       How to get it:
#         1. Enroll in the Apple Developer Program ($99/year):
#            https://developer.apple.com/programs/
#         2. Go to https://developer.apple.com/account/resources/certificates
#         3. Create a "Developer ID Application" certificate
#         4. Download and double-click to install into Keychain Access
#         5. Run:  security find-identity -v -p codesigning
#            to see the full identity string
#
#       Example: "Developer ID Application: Jane Smith (A1B2C3D4E5)"
#
#   APPLE_ID
#       The Apple ID email address associated with your developer account.
#       This is the same email you use to sign in at developer.apple.com.
#
#       Example: "jane@example.com"
#
#   APPLE_PASSWORD
#       An app-specific password for Apple's notarization service.
#       This is NOT your Apple ID login password.
#
#       How to get it:
#         1. Go to https://appleid.apple.com/account/manage
#         2. Sign in → "Sign-In and Security" → "App-Specific Passwords"
#         3. Click "+" to generate a new password
#         4. Give it a label like "NeuroSkill™ notarization"
#         5. Copy the generated password (format: xxxx-xxxx-xxxx-xxxx)
#
#       Example: "abcd-efgh-ijkl-mnop"
#
#   APPLE_TEAM_ID
#       Your 10-character Apple Developer Team ID.
#
#       How to find it:
#         1. Go to https://developer.apple.com/account
#         2. Scroll down to "Membership details"
#         3. Copy the "Team ID" field (e.g. "A1B2C3D4E5")
#         — OR —
#         Run:  security find-identity -v -p codesigning
#         The Team ID is the string in parentheses at the end of
#         your Developer ID certificate name.
#
#       Example: "A1B2C3D4E5"
#
#   TAURI_SIGNING_PRIVATE_KEY
#       Base64-encoded Ed25519 private key for signing Tauri updater
#       artifacts. The app verifies updates against the corresponding
#       public key embedded in tauri.conf.json.
#
#       How to get it:
#         1. Run:  python3 src-tauri/keys/generate-keys.py
#            (requires pynacl: pip install pynacl)
#         2. The private key is written to src-tauri/keys/updater.key
#         3. The matching public key goes into tauri.conf.json →
#            plugins.updater.pubkey (the script does this for you)
#         — OR —
#         Source the generated env file:
#           source src-tauri/keys/.env.keys
#
#       Example: "RWQAAAAAxyz...base64..."
#
#   AWS_ACCESS_KEY_ID
#       AWS IAM access key ID for uploading release artifacts to S3.
#
#       How to get it:
#         1. Go to https://console.aws.amazon.com/iam/
#         2. Users → select your user → "Security credentials" tab
#         3. "Create access key" → choose "Command Line Interface"
#         4. Copy the Access Key ID
#         — OR —
#         Create a dedicated IAM user with only s3:PutObject and
#         s3:PutObjectAcl permissions on your release bucket.
#
#       Example: "AKIAIOSFODNN7EXAMPLE"
#
#   AWS_SECRET_ACCESS_KEY
#       AWS IAM secret access key (paired with AWS_ACCESS_KEY_ID).
#
#       How to get it:
#         Shown once when you create the access key (step above).
#         If lost, delete the old key and create a new one.
#
#       Example: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
#
# ══════════════════════════════════════════════════════════════════════
# OPTIONAL ENVIRONMENT VARIABLES
# ══════════════════════════════════════════════════════════════════════
#
#   TAURI_SIGNING_PRIVATE_KEY_PASSWORD      (default: "")
#       Password protecting the Ed25519 signing key.
#       Leave empty (or unset) if no password was set during key
#       generation — this is the default from generate-keys.py.
#
#   SKIP_NOTARIZE                           (default: "0")
#       Set to "1" to skip Apple notarization and stapling.
#       Useful for local testing; the app will show a Gatekeeper
#       warning on other machines without notarization.
#
#   SKIP_UPLOAD                             (default: "0")
#       Set to "1" to skip the S3 upload step.
#       The signed .dmg and updater artifacts are still produced
#       locally; you can upload them manually.
#
#   TAURI_TARGET
#       Override the Rust compilation target triple.
#       If unset, Tauri auto-detects the host architecture.
#
#       Common values:
#         aarch64-apple-darwin      — Apple Silicon only
#         x86_64-apple-darwin       — Intel only
#         universal-apple-darwin    — Fat binary (both archs)
#
#   S3_BUCKET                               (default: "releases.example.com")
#       Name of the S3 bucket to upload artifacts to.
#       Change this to your actual bucket name.
#
#       How to create one:
#         aws s3 mb s3://your-bucket-name --region us-east-1
#         — then configure it for static website hosting or put
#         CloudFront in front of it.
#
#   S3_REGION                               (default: "us-east-1")
#       AWS region where the S3 bucket lives.
#
#   S3_PREFIX                               (default: "skill")
#       Key prefix (folder) inside the bucket.
#       Artifacts are uploaded to: s3://<bucket>/<prefix>/<version>/
#
#   AWS_PROFILE
#       Named AWS CLI profile to use (from ~/.aws/credentials)
#       instead of the AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY
#       env vars. Useful if you manage multiple AWS accounts.
#
#       How to set up:
#         aws configure --profile skill-release
#         — then set AWS_PROFILE=skill-release
#
#   CLOUDFRONT_DISTRIBUTION_ID
#       If your S3 bucket is fronted by CloudFront, set this to the
#       distribution ID to automatically invalidate the cache after
#       uploading new artifacts.
#
#       How to find it:
#         1. Go to https://console.aws.amazon.com/cloudfront/
#         2. Find your distribution → copy the "Distribution ID"
#            (e.g. "E1ABCDEF2GHIJK")
#
#       Example: "E1ABCDEF2GHIJK"
#
# ══════════════════════════════════════════════════════════════════════
# FLAGS
# ══════════════════════════════════════════════════════════════════════
#
#   --dry-run | -n
#       Print every command that would be executed without actually
#       running anything destructive. Build is skipped; signing,
#       notarization, DMG packaging, and S3 upload are only printed.
#       Useful for verifying your env vars and paths are correct.
#
#   --env-file <path> | -e <path>
#       Path to the env file to load (default: env.txt next to release.sh).
#       Overrides the default location; variables in the file still take
#       priority over already-exported environment variables.
#
# ══════════════════════════════════════════════════════════════════════
# USAGE EXAMPLES
# ══════════════════════════════════════════════════════════════════════
#
#   # Full release (all env vars set):
#   source skill/src-tauri/keys/.env.keys
#   export APPLE_SIGNING_IDENTITY="Developer ID Application: Jane Smith (A1B2C3D4E5)"
#   export APPLE_ID="jane@example.com"
#   export APPLE_PASSWORD="abcd-efgh-ijkl-mnop"
#   export APPLE_TEAM_ID="A1B2C3D4E5"
#   export AWS_ACCESS_KEY_ID="AKIA..."
#   export AWS_SECRET_ACCESS_KEY="..."
#   export S3_BUCKET="releases.myapp.com"
#   bash /agent/release.sh
#
#   # Dry run — see what would happen:
#   bash /agent/release.sh --dry-run
#
#   # Use a custom env file location:
#   bash /agent/release.sh --env-file /secrets/prod.env
#   bash /agent/release.sh -e ~/my-release.env --dry-run
#
#   # Build + sign only, skip notarization and upload:
#   SKIP_NOTARIZE=1 SKIP_UPLOAD=1 bash /agent/release.sh
#
#   # Universal binary (Intel + Apple Silicon):
#   TAURI_TARGET=universal-apple-darwin bash /agent/release.sh
#
# ══════════════════════════════════════════════════════════════════════
# REQUIRED TOOLS (must be in PATH)
# ══════════════════════════════════════════════════════════════════════
#
#   codesign    — macOS code signing (ships with Xcode CLI tools)
#   xcrun       — Xcode toolchain runner (ships with Xcode CLI tools)
#   ditto       — macOS archive utility (ships with macOS)
#   hdiutil     — macOS disk image utility (ships with macOS)
#   npx         — Node.js package runner (install Node.js from nodejs.org)
#   npm         — Node.js package manager (bundled with Node.js)
#   aws         — AWS CLI v2 (brew install awscli, or pip install awscli)
#                 Only required when SKIP_UPLOAD != 1
#
# ──────────────────────────────────────────────────────────────────────
set -euo pipefail

# ── Constants ──────────────────────────────────────────────────────────────────

APP_NAME="skill"
BUNDLE_ID="com.root.skill"
REPO_ROOT="$(cd "$(dirname "$0")/skill" && pwd)"
TAURI_DIR="$REPO_ROOT/src-tauri"
ENTITLEMENTS="$TAURI_DIR/entitlements.plist"

S3_BUCKET="${S3_BUCKET:-releases.example.com}"
S3_REGION="${S3_REGION:-us-east-1}"
S3_PREFIX="${S3_PREFIX:-skill}"

# ── Parse flags ────────────────────────────────────────────────────────────────

DRY_RUN=0
ENV_FILE_ARG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run|-n)
            DRY_RUN=1
            shift
            ;;
        --env-file|-e)
            [ -z "${2:-}" ] && { echo "Error: $1 requires a path argument" >&2; exit 1; }
            ENV_FILE_ARG="$2"
            shift 2
            ;;
        --env-file=*|-e=*)
            ENV_FILE_ARG="${1#*=}"
            shift
            ;;
        *)
            echo "Unknown flag: $1" >&2; exit 1
            ;;
    esac
done

# ── Helpers ────────────────────────────────────────────────────────────────────

log()  { printf "\033[1;34m→ %s\033[0m\n" "$*"; }
err()  { printf "\033[1;31m✗ %s\033[0m\n" "$*" >&2; }
ok()   { printf "\033[1;32m✓ %s\033[0m\n" "$*"; }
dry()  { printf "\033[1;33m  [dry-run] %s\033[0m\n" "$*"; }

fail() { err "$1"; exit 1; }

# Execute a command, or just print it in dry-run mode.
run() {
    if [ "$DRY_RUN" = "1" ]; then
        dry "$*"
    else
        "$@"
    fi
}

check_var() {
    if [ -z "${!1:-}" ]; then
        if [ "$DRY_RUN" = "1" ]; then
            dry "WARNING: $1 is not set (would fail in real run)"
        else
            fail "Missing required env var: $1"
        fi
    fi
}

# ── Load env.txt (overrides env vars; env vars are the fallback) ───────────────

if [ -n "$ENV_FILE_ARG" ]; then
    ENV_FILE="$ENV_FILE_ARG"
else
    ENV_FILE="$(cd "$(dirname "$0")" && pwd)/env.txt"
fi
if [ -n "$ENV_FILE_ARG" ] && [ ! -f "$ENV_FILE" ]; then
    echo "Error: env file not found: $ENV_FILE" >&2; exit 1
fi
if [ -f "$ENV_FILE" ]; then
    log "Loading environment from $ENV_FILE"
    while IFS= read -r line || [ -n "$line" ]; do
        # Skip blank lines and comments
        [[ "$line" =~ ^[[:space:]]*$ ]]  && continue
        [[ "$line" =~ ^[[:space:]]*#  ]] && continue
        # Parse KEY=VALUE (optional surrounding quotes are stripped)
        if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
            key="${BASH_REMATCH[1]}"
            val="${BASH_REMATCH[2]}"
            # Strip matching surrounding double- or single-quotes
            if [[ "$val" =~ ^\"(.*)\"$ ]] || [[ "$val" =~ ^\'(.*)\'$ ]]; then
                val="${BASH_REMATCH[1]}"
            fi
            export "$key=$val"
        fi
    done < "$ENV_FILE"
else
    log "No env.txt found at $ENV_FILE — falling back to environment variables"
fi

# ── Preflight checks ──────────────────────────────────────────────────────────

check_var APPLE_SIGNING_IDENTITY
check_var APPLE_ID
check_var APPLE_PASSWORD
check_var APPLE_TEAM_ID
check_var TAURI_SIGNING_PRIVATE_KEY

export TAURI_SIGNING_PRIVATE_KEY="${TAURI_SIGNING_PRIVATE_KEY:-}"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="${TAURI_SIGNING_PRIVATE_KEY_PASSWORD:-}"

if [ "${SKIP_UPLOAD:-0}" != "1" ]; then
    check_var AWS_ACCESS_KEY_ID
    check_var AWS_SECRET_ACCESS_KEY
fi

# Verify tools are installed
for cmd in codesign xcrun npx ditto hdiutil; do
    command -v "$cmd" >/dev/null 2>&1 || fail "Required tool not found: $cmd"
done

if [ "${SKIP_UPLOAD:-0}" != "1" ]; then
    command -v aws >/dev/null 2>&1 || fail "Required tool not found: aws (install with: brew install awscli)"
fi

VERSION="$(grep '"version"' "$TAURI_DIR/tauri.conf.json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')"

log "Release build for $APP_NAME v$VERSION"
log "Signing identity: ${APPLE_SIGNING_IDENTITY:-<not set>}"
log "Team ID:          ${APPLE_TEAM_ID:-<not set>}"
log "Apple ID:         ${APPLE_ID:-<not set>}"
log "S3 destination:   s3://$S3_BUCKET/$S3_PREFIX/$VERSION/"
[ "$DRY_RUN" = "1" ] && log "MODE: DRY RUN — nothing destructive will execute"

# ── Step 1: Build ──────────────────────────────────────────────────────────────

log "Building Tauri app in release mode…"

cd "$REPO_ROOT"

npm install --prefer-offline 2>/dev/null || npm install

TAURI_TARGET="${TAURI_TARGET:-aarch64-apple-darwin}"

TAURI_ARGS=(build)
if [ -n "${TAURI_TARGET:-}" ]; then
    TAURI_ARGS+=(--target "$TAURI_TARGET")
    log "Target: $TAURI_TARGET"
fi

if [ "$DRY_RUN" = "1" ]; then
    dry "npx tauri ${TAURI_ARGS[*]}"
    log "Dry run: skipping actual build. Looking for existing artifacts…"
else
    npx tauri "${TAURI_ARGS[@]}"
fi

ok "Build step complete"

# ── Locate build artifacts ─────────────────────────────────────────────────────

if [ -n "${TAURI_TARGET:-}" ]; then
    BUNDLE_BASE="$TAURI_DIR/target/$TAURI_TARGET/release/bundle"
else
    BUNDLE_BASE="$TAURI_DIR/target/release/bundle"
fi

# Find the .app bundle
APP_BUNDLE="$(find "$BUNDLE_BASE/macos" -name "*.app" -maxdepth 1 2>/dev/null | head -1)"
if [ -z "$APP_BUNDLE" ] || [ ! -d "$APP_BUNDLE" ]; then
    if [ "$DRY_RUN" = "1" ]; then
        APP_BUNDLE="$BUNDLE_BASE/macos/$APP_NAME.app"
        dry "Would expect .app at: $APP_BUNDLE"
    else
        fail "Could not find .app bundle in $BUNDLE_BASE/macos/"
    fi
fi

log "App bundle: $APP_BUNDLE"

# ── Step 2: Code-sign ─────────────────────────────────────────────────────────

log "Code-signing the app bundle…"

run codesign \
    --deep \
    --force \
    --verify \
    --verbose \
    --timestamp \
    --options runtime \
    --entitlements "$ENTITLEMENTS" \
    --sign "${APPLE_SIGNING_IDENTITY:-}" \
    "$APP_BUNDLE"

ok "Code-signing complete"

log "Verifying code signature…"
run codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"
ok "Signature valid"

# ── Step 3: Notarize ──────────────────────────────────────────────────────────

if [ "${SKIP_NOTARIZE:-0}" = "1" ]; then
    log "Skipping notarization (SKIP_NOTARIZE=1)"
else
    log "Creating ZIP for notarization…"

    NOTARIZE_ZIP="$(mktemp -d)/$(basename "$APP_BUNDLE" .app).zip"

    if [ "$DRY_RUN" = "1" ]; then
        dry "ditto -c -k --keepParent $APP_BUNDLE $NOTARIZE_ZIP"
    else
        ditto -c -k --keepParent "$APP_BUNDLE" "$NOTARIZE_ZIP"
    fi

    log "Submitting to Apple notary service…"
    run xcrun notarytool submit "$NOTARIZE_ZIP" \
        --apple-id "${APPLE_ID:-}" \
        --password "${APPLE_PASSWORD:-}" \
        --team-id "${APPLE_TEAM_ID:-}" \
        --wait \
        --timeout 1800

    ok "Notarization accepted"

    log "Stapling notarization ticket…"
    run xcrun stapler staple "$APP_BUNDLE"
    ok "Ticket stapled"

    rm -f "$NOTARIZE_ZIP"
fi

# ── Step 4: Package DMG ───────────────────────────────────────────────────────

log "Building DMG…"

EXISTING_DMG="$(find "$BUNDLE_BASE/dmg" -name "*.dmg" -maxdepth 1 2>/dev/null | head -1)"

if [ -n "$EXISTING_DMG" ] && [ -f "$EXISTING_DMG" ]; then
    DMG_NAME="$(basename "$EXISTING_DMG")"
    DMG_OUT="$BUNDLE_BASE/dmg/$DMG_NAME"
else
    DMG_OUT="$BUNDLE_BASE/dmg/${APP_NAME}_${VERSION}.dmg"
    mkdir -p "$(dirname "$DMG_OUT")"
fi

if [ "$DRY_RUN" = "1" ]; then
    dry "hdiutil create -volname $APP_NAME -srcfolder <staging> -ov -format UDZO $DMG_OUT"
else
    DMG_TMP="$(mktemp -d)"
    DMG_STAGING="$DMG_TMP/staging"
    mkdir -p "$DMG_STAGING"
    cp -R "$APP_BUNDLE" "$DMG_STAGING/"
    ln -s /Applications "$DMG_STAGING/Applications"
    rm -f "$DMG_OUT"
    hdiutil create \
        -volname "$APP_NAME" \
        -srcfolder "$DMG_STAGING" \
        -ov \
        -format UDZO \
        "$DMG_OUT"
    rm -rf "$DMG_TMP"
fi

# ── Step 4b: Stamp version badge onto the DMG's Finder icon ──────────────────
#
# This composites the version string over the app icon and attaches the result
# as the DMG file's custom Finder icon via NSWorkspace. The icon is visible
# in Finder's icon/gallery views, making it trivial to compare builds visually
# without reading filenames.
#
# Requirements: ImageMagick + iconutil (Xcode CLI) + Python/AppKit (macOS).
# The stamp script is a no-op in dry-run mode.

if [ "$DRY_RUN" = "1" ]; then
    dry "bash $REPO_ROOT/scripts/stamp-dmg-icon.sh $DMG_OUT $VERSION $TAURI_DIR/icons/icon.png"
else
    log "Stamping version badge onto DMG icon…"
    if bash "$REPO_ROOT/scripts/stamp-dmg-icon.sh" "$DMG_OUT" "$VERSION" "$TAURI_DIR/icons/icon.png"; then
        ok "DMG icon stamped with v$VERSION"
    else
        # Non-fatal: a missing tool (e.g. ImageMagick) shouldn't abort the release.
        printf "\033[1;33m⚠ DMG icon stamp skipped (non-fatal — check script output above)\033[0m\n"
    fi
fi

# Sign the DMG
run codesign \
    --force \
    --timestamp \
    --sign "${APPLE_SIGNING_IDENTITY:-}" \
    "$DMG_OUT"

# Notarize the DMG
if [ "${SKIP_NOTARIZE:-0}" != "1" ]; then
    log "Notarizing DMG…"
    run xcrun notarytool submit "$DMG_OUT" \
        --apple-id "${APPLE_ID:-}" \
        --password "${APPLE_PASSWORD:-}" \
        --team-id "${APPLE_TEAM_ID:-}" \
        --wait \
        --timeout 1800
    run xcrun stapler staple "$DMG_OUT"
    ok "DMG notarized and stapled"
fi

ok "DMG: $DMG_OUT"

# ── Step 5: Collect updater artifacts ──────────────────────────────────────────

# Tauri produces a .tar.gz + .tar.gz.sig (macOS) or .nsis.zip + .nsis.zip.sig
# (Windows) when TAURI_SIGNING_PRIVATE_KEY is set during the build.

UPDATER_BUNDLE=""
UPDATER_SIG=""

while IFS= read -r f; do
    case "$f" in
        *.tar.gz.sig|*.nsis.zip.sig) UPDATER_SIG="$f" ;;
        *.tar.gz|*.nsis.zip)         UPDATER_BUNDLE="$f" ;;
    esac
done < <(find "$BUNDLE_BASE" \( -name "*.tar.gz" -o -name "*.tar.gz.sig" \
                                -o -name "*.nsis.zip" -o -name "*.nsis.zip.sig" \) 2>/dev/null)

log "Updater artifacts:"
[ -n "$UPDATER_BUNDLE" ] && echo "  bundle:    $UPDATER_BUNDLE"
[ -n "$UPDATER_SIG" ]    && echo "  signature: $UPDATER_SIG"
[ -n "$DMG_OUT" ]        && echo "  dmg:       $DMG_OUT"

# ── Step 6: Generate updater JSON manifest ─────────────────────────────────────

# The Tauri updater expects a JSON response from the endpoint with:
#   version, url, signature, notes (optional), pub_date (optional)
# We generate this so it can be uploaded alongside the artifacts.

MANIFEST_FILE="$BUNDLE_BASE/update-manifest.json"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
UPDATER_BUNDLE_NAME="$(basename "${UPDATER_BUNDLE:-update.tar.gz}")"
UPDATER_SIG_CONTENT=""

if [ -n "$UPDATER_SIG" ] && [ -f "$UPDATER_SIG" ]; then
    UPDATER_SIG_CONTENT="$(cat "$UPDATER_SIG")"
elif [ "$DRY_RUN" = "1" ]; then
    UPDATER_SIG_CONTENT="<signature-placeholder>"
fi

# S3 URL where the updater bundle will live after upload
UPDATER_URL="https://${S3_BUCKET}/${S3_PREFIX}/${VERSION}/${UPDATER_BUNDLE_NAME}"

cat > "$MANIFEST_FILE" <<MANIFEST_EOF
{
  "version": "$VERSION",
  "notes": "NeuroSkill™ v$VERSION",
  "pub_date": "$PUB_DATE",
  "url": "$UPDATER_URL",
  "signature": "$UPDATER_SIG_CONTENT"
}
MANIFEST_EOF

log "Updater manifest: $MANIFEST_FILE"
if [ "$DRY_RUN" = "1" ]; then
    dry "Manifest contents:"
    cat "$MANIFEST_FILE" | sed 's/^/    /'
fi

# ── Step 7: Upload to S3 ──────────────────────────────────────────────────────

if [ "${SKIP_UPLOAD:-0}" = "1" ]; then
    log "Skipping S3 upload (SKIP_UPLOAD=1)"
else
    log "Uploading to s3://$S3_BUCKET/$S3_PREFIX/$VERSION/ …"

    AWS_ARGS=()
    [ -n "${AWS_PROFILE:-}" ] && AWS_ARGS+=(--profile "$AWS_PROFILE")
    AWS_ARGS+=(--region "$S3_REGION")

    S3_DEST="s3://$S3_BUCKET/$S3_PREFIX/$VERSION"

    # Upload the DMG (main distribution artifact)
    if [ -f "$DMG_OUT" ]; then
        run aws s3 cp "${AWS_ARGS[@]}" \
            "$DMG_OUT" "$S3_DEST/$(basename "$DMG_OUT")" \
            --content-type "application/x-apple-diskimage"
        ok "Uploaded DMG"
    fi

    # Upload updater bundle (.tar.gz / .nsis.zip)
    if [ -n "$UPDATER_BUNDLE" ] && [ -f "$UPDATER_BUNDLE" ]; then
        run aws s3 cp "${AWS_ARGS[@]}" \
            "$UPDATER_BUNDLE" "$S3_DEST/$(basename "$UPDATER_BUNDLE")" \
            --content-type "application/gzip"
        ok "Uploaded updater bundle"
    fi

    # Upload updater signature (.sig)
    if [ -n "$UPDATER_SIG" ] && [ -f "$UPDATER_SIG" ]; then
        run aws s3 cp "${AWS_ARGS[@]}" \
            "$UPDATER_SIG" "$S3_DEST/$(basename "$UPDATER_SIG")" \
            --content-type "text/plain"
        ok "Uploaded updater signature"
    fi

    # Upload the update manifest JSON
    # This is the file the Tauri updater endpoint should serve.
    # Upload to both the versioned path and a "latest" path so the
    # endpoint can serve a static file without any server-side logic.
    if [ -f "$MANIFEST_FILE" ]; then
        run aws s3 cp "${AWS_ARGS[@]}" \
            "$MANIFEST_FILE" "$S3_DEST/update-manifest.json" \
            --content-type "application/json"

        # Also upload as the "latest" manifest — the Tauri endpoint URL
        # can point to this static file directly.
        run aws s3 cp "${AWS_ARGS[@]}" \
            "$MANIFEST_FILE" "s3://$S3_BUCKET/$S3_PREFIX/latest/update-manifest.json" \
            --content-type "application/json"
        ok "Uploaded update manifest (versioned + latest)"
    fi

    ok "S3 upload complete"

    # Optionally invalidate CloudFront cache if a distribution ID is set
    if [ -n "${CLOUDFRONT_DISTRIBUTION_ID:-}" ]; then
        log "Invalidating CloudFront cache…"
        run aws cloudfront create-invalidation "${AWS_ARGS[@]}" \
            --distribution-id "$CLOUDFRONT_DISTRIBUTION_ID" \
            --paths "/$S3_PREFIX/*"
        ok "CloudFront invalidation submitted"
    fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║                    Release Complete                         ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "  App:          $APP_NAME v$VERSION"
echo "  Bundle:       $APP_BUNDLE"
echo "  DMG:          $DMG_OUT"
echo "  Signed:       ${APPLE_SIGNING_IDENTITY:-<not set>}"
echo "  Notarized:    $([ "${SKIP_NOTARIZE:-0}" = "1" ] && echo "SKIPPED" || echo "YES")"
echo "  S3 upload:    $([ "${SKIP_UPLOAD:-0}" = "1" ] && echo "SKIPPED" || echo "s3://$S3_BUCKET/$S3_PREFIX/$VERSION/")"
echo "  Dry run:      $([ "$DRY_RUN" = "1" ] && echo "YES" || echo "no")"
echo ""
if [ "${SKIP_UPLOAD:-0}" != "1" ] && [ "$DRY_RUN" != "1" ]; then
    echo "  Update endpoint URL:"
    echo "    https://$S3_BUCKET/$S3_PREFIX/latest/update-manifest.json"
    echo ""
fi
echo "  Artifacts uploaded to S3:"
echo "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/$(basename "$DMG_OUT")"
[ -n "$UPDATER_BUNDLE" ] && \
echo "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/$(basename "$UPDATER_BUNDLE")"
[ -n "$UPDATER_SIG" ] && \
echo "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/$(basename "$UPDATER_SIG")"
echo "    s3://$S3_BUCKET/$S3_PREFIX/$VERSION/update-manifest.json"
echo "    s3://$S3_BUCKET/$S3_PREFIX/latest/update-manifest.json"
echo ""
