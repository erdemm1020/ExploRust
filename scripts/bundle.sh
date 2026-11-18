#!/usr/bin/env bash
# Builds dist/ExploRust.app.
#
#   scripts/bundle.sh                       release build for this Mac
#   TARGETS="aarch64-apple-darwin x86_64-apple-darwin" scripts/bundle.sh   universal
#   PROFILE=debug scripts/bundle.sh         debug build (faster, for testing)
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${PROFILE:-release}"
TARGETS="${TARGETS:-$(rustc -vV | sed -n 's/^host: //p')}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
APP="dist/ExploRust.app"

if [ "$PROFILE" = "release" ]; then FLAG="--release"; else FLAG=""; fi

bins=()
for target in $TARGETS; do
    echo "==> building for $target ($PROFILE)"
    # shellcheck disable=SC2086
    cargo build $FLAG --target "$target"
    bins+=("target/$target/$PROFILE/ExploRust")
done

echo "==> assembling $APP (v$VERSION)"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/ExploRust" "${bins[@]}"
sed "s/@VERSION@/$VERSION/g" packaging/Info.plist > "$APP/Contents/Info.plist"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"

# Ad-hoc signature: required for the app to start on Apple Silicon.
# (A paid Developer ID would also remove the Gatekeeper warning on first launch.)
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

lipo -info "$APP/Contents/MacOS/ExploRust"
echo "==> done: $APP"
