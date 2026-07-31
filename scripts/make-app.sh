#!/usr/bin/env bash
# Assembles starview.app around a built binary and ad-hoc codesigns it.
# Ad-hoc (no Developer ID): downloaded copies carry the quarantine xattr, so
# first launch needs right-click > Open (or xattr -d com.apple.quarantine).
#
# Usage: scripts/make-app.sh [path-to-binary] [out-dir]
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${1:-target/release/starview}"
OUT="${2:-target/release}"
VERSION=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)
APP="$OUT/starview.app"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/starview"
cp assets/starview.icns "$APP/Contents/Resources/starview.icns"
sed "s/__VERSION__/$VERSION/g" installer/Info.plist > "$APP/Contents/Info.plist"

codesign --force --sign - "$APP"
echo "built $APP (v$VERSION, ad-hoc signed)"
