#!/usr/bin/env bash
# Publishes server/goldens as the golden files of CI: a GitHub pre-release goldens-DATE whose asset
# is goldens.tar.zst, then writes its tag and SHA-256 in goldens/ARCHIVE (commit that file).
#   tools/goldens_publish.sh [TAG]
# Run it after changing the golden files on purpose, with the goldens run passing.
set -euo pipefail
cd "$(dirname "$0")/.."
TAG=${1:-goldens-$(date +%F)}
REPO=${GOLDENS_REPO:-Yutsa/altered-arena}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
COPYFILE_DISABLE=1 tar --exclude .DS_Store -C server -cf - goldens | zstd -T0 -19 -q -o "$tmp/goldens.tar.zst"
SHA256=$(shasum -a 256 "$tmp/goldens.tar.zst" | cut -d' ' -f1)
gh release create "$TAG" "$tmp/goldens.tar.zst" --repo "$REPO" --prerelease --latest=false \
  --title "Golden files $TAG" \
  --notes "Golden decision trees (server/goldens) for the CI goldens check. Not a version of the game: see goldens/ARCHIVE."
cat > goldens/ARCHIVE <<EOF
# The golden files CI downloads: asset goldens.tar.zst of the GitHub release TAG of this repository
# (tools/goldens_publish.sh makes a new one, tools/goldens_fetch.sh downloads it)
TAG=$TAG
SHA256=$SHA256
EOF
echo "goldens/ARCHIVE now names $TAG ($SHA256)"
