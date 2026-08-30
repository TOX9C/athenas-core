#!/bin/bash
# Build the macOS app with a STABLE code-signing identity so macOS TCC
# permission grants (notifications, mic, speech, Files & Folders) persist
# across rebuilds instead of re-prompting every launch.
#
# Ad-hoc signing (the default when no identity is configured) changes on every
# build, so macOS treats each build as a new app and re-asks for permission.
# Use a stable Developer ID identity when permission persistence across rebuilds
# matters; ad-hoc signatures change between builds and trigger new prompts.
#
# Usage:
#   scripts/macos-sign-locally.sh
#
# Optional env vars (passed through to `tauri build` for notarization):
#   APPLE_SIGNING_IDENTITY  exact identity name (auto-detected if unset)
#   APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Athena's Core"
BUNDLE_ID="com.athena.core"

# ── 1. Resolve the signing identity ────────────────────────────────────────
IDENTITY="${APPLE_SIGNING_IDENTITY:-}"
if [ -z "$IDENTITY" ]; then
  # Prefer Developer ID (durable); fall back to Apple Development (local use).
  IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
    | awk -F'"' '/Developer ID Application/ {print $2; exit}')"
  if [ -z "$IDENTITY" ]; then
    IDENTITY="$(security find-identity -v -p codesigning 2>/dev/null \
      | awk -F'"' '/Apple Development/ {print $2; exit}')"
  fi
fi

if [ -z "$IDENTITY" ]; then
  cat >&2 <<'EOF'
error: no code-signing identity found.

  Run:  security find-identity -v -p codesigning

If it prints "0 valid identities found", install a certificate first:
  - Developer ID Application  -> durable, requires paid Apple Developer Program
  - Apple Development         -> local-only, expires after a few days

Then re-run with:
  APPLE_SIGNING_IDENTITY="Developer ID Application: ... (TEAMID)" \
    scripts/macos-sign-locally.sh
EOF
  exit 2
fi

case "$IDENTITY" in
  *"Developer ID Application"*) ;;
  *)
    echo "warning: '$IDENTITY' is not a Developer ID identity." >&2
    echo "         Grants will persist only until the certificate expires." >&2
    ;;
esac
echo "signing identity: $IDENTITY"

# ── 2. Build (Tauri signs when APPLE_SIGNING_IDENTITY is set) ──────────────
export APPLE_SIGNING_IDENTITY="$IDENTITY"
# Pass through notarization credentials when present.
for var in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID APPLE_API_KEY APPLE_API_ISSUER; do
  if [ -n "${!var:-}" ]; then
    export "$var"
  fi
done

cd "$REPO_ROOT"
echo "building (this signs the app)…"
cargo tauri build

# ── 3. Verify ───────────────────────────────────────────────────────────────
APP="$REPO_ROOT/target/release/bundle/macos/$APP_NAME.app"
if [ ! -d "$APP" ]; then
  echo "error: expected app bundle not found at $APP" >&2
  exit 1
fi

codesign --verify --deep --strict --verbose=2 "$APP"
echo "--- signature ---"
codesign -dv "$APP" 2>&1 | grep -E 'Identifier=|Signature=|TeamIdentifier=' || true

echo "--- Gatekeeper assessment (may reject if not notarized) ---"
spctl --assess --type execute --verbose "$APP" || \
  echo "note: spctl rejected — notarize the app for a clean Gatekeeper result"

if [ -n "${APPLE_ID:-}" ]; then
  echo "--- notarization staple ---"
  xcrun stapler validate "$APP" || echo "note: no stapled notarization ticket found"
fi

# ── 4. Remind about stale grants ────────────────────────────────────────────
cat <<EOF

Done. To clear the stale (ad-hoc) grants and get exactly one clean prompt:

  tccutil reset All "$BUNDLE_ID"

Then install "$APP_NAME" into /Applications, launch it, and grant each
permission once — they will now persist across rebuilds.
EOF
