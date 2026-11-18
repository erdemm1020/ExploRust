#!/usr/bin/env bash
# Packs dist/ExploRust.app into dist/ExploRust-<version>.dmg (+ a .sha256 file).
# Run scripts/bundle.sh first.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="dist/ExploRust.app"
DMG="dist/ExploRust-$VERSION.dmg"
[ -d "$APP" ] || { echo "missing $APP: run scripts/bundle.sh first" >&2; exit 1; }

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
ditto "$APP" "$STAGE/ExploRust.app"
ln -s /Applications "$STAGE/Applications"

rm -f "$DMG"
# hdiutil sometimes fails with "Resource busy" on CI machines: try a few times.
for attempt in 1 2 3; do
    if hdiutil create -volname "ExploRust" -srcfolder "$STAGE" -ov -format UDZO "$DMG" >/dev/null; then
        break
    fi
    [ "$attempt" = 3 ] && { echo "hdiutil failed 3 times" >&2; exit 1; }
    sleep 5
done

# Same format as `shasum -a 256 file`: "<hash>  <name>". The app checks it before installing.
(cd dist && shasum -a 256 "ExploRust-$VERSION.dmg" > "ExploRust-$VERSION.dmg.sha256")

echo "==> $DMG"
cat "$DMG.sha256"
