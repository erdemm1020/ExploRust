#!/usr/bin/env bash
# Packs dist/ExploRust.app into dist/ExploRust-<version>.dmg (+ a .sha256 file),
# with the styled install window (background, big icons, arrow to Applications).
# Run scripts/bundle.sh first.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="dist/ExploRust.app"
DMG="dist/ExploRust-$VERSION.dmg"
[ -d "$APP" ] || { echo "missing $APP: run scripts/bundle.sh first" >&2; exit 1; }
rm -f "$DMG"

# dmgbuild lays the window out without opening the Finder, so it works on CI.
styled_dmg() {
    local venv="build/dmg-venv"
    [ -x "$venv/bin/dmgbuild" ] || {
        python3 -m venv "$venv" && "$venv/bin/pip" install --quiet "dmgbuild==2.0.0"
    } || return 1
    "$venv/bin/dmgbuild" -s packaging/dmg-settings.py -D app="$APP" "ExploRust" "$DMG"
}

# Plain fallback (no styling): app + a shortcut to Applications.
plain_dmg() {
    local stage
    stage="$(mktemp -d)"
    ditto "$APP" "$stage/ExploRust.app"
    ln -s /Applications "$stage/Applications"
    hdiutil create -volname "ExploRust" -srcfolder "$stage" -ov -format UDZO "$DMG" >/dev/null
    rm -rf "$stage"
}

# hdiutil sometimes fails with "Resource busy" on CI machines: try a few times.
made=""
for attempt in 1 2 3; do
    if styled_dmg; then made="styled"; break; fi
    echo "styled DMG failed (attempt $attempt)" >&2
    rm -f "$DMG"
    sleep 5
done
if [ -z "$made" ]; then
    echo "warning: falling back to a plain DMG" >&2
    plain_dmg
fi

# Same format as `shasum -a 256 file`: "<hash>  <name>". The app checks it before installing.
(cd dist && shasum -a 256 "ExploRust-$VERSION.dmg" > "ExploRust-$VERSION.dmg.sha256")

echo "==> $DMG (${made:-plain})"
cat "$DMG.sha256"
