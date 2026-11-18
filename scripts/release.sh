#!/usr/bin/env bash
# Prepares a release: bumps the version, commits, and tags. You push.
#
#   scripts/release.sh 0.2.0
#   git push && git push origin v0.2.0     # the tag starts the build on GitHub
set -euo pipefail
cd "$(dirname "$0")/.."

NEW="${1:?usage: scripts/release.sh <version>, e.g. 0.2.0}"
[[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "the version must look like 1.2.3" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "commit or stash your changes first" >&2; exit 1; }
git rev-parse "v$NEW" >/dev/null 2>&1 && { echo "tag v$NEW already exists" >&2; exit 1; }

# Only the package version (the first `version = ` line of Cargo.toml).
perl -0pi -e "s/^version = \".*?\"/version = \"$NEW\"/m" Cargo.toml
cargo metadata --format-version 1 >/dev/null   # refreshes Cargo.lock

git commit -qam "chore: release v$NEW"
git tag -a "v$NEW" -m "ExploRust $NEW"

echo "Done. Now push the commit and the tag:"
echo "  git push && git push origin v$NEW"
