#!/bin/bash
# Installs Hyperion Studio into /Applications from the built .app bundle.
#
# Usage: ./scripts/install-macos.sh
#
# The app is ad-hoc signed (no Apple Developer certificate yet), so macOS will
# warn on first launch. This script clears the quarantine attribute so the
# "unidentified developer" prompt does not appear for a local build.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/src-tauri/target/release/bundle/macos/Hyperion Studio.app"
DEST="/Applications/Hyperion Studio.app"

if [ ! -d "$APP" ]; then
  echo "App not found at: $APP"
  echo "Build it first:  bun run tauri build"
  exit 1
fi

if [ -d "$DEST" ]; then
  echo "Removing previous install: $DEST"
  rm -rf "$DEST"
fi

echo "Installing to $DEST"
cp -R "$APP" "$DEST"

# Ad-hoc signature: clear the quarantine flag for this locally built app.
xattr -dr com.apple.quarantine "$DEST" 2>/dev/null || true

# Refresh Finder's icon cache so the new icon shows immediately.
touch "$DEST"

echo "Done. Launch it from Applications, or:"
echo "  open \"$DEST\""
