#!/usr/bin/env bash
# Downloads the golden files named by goldens/ARCHIVE into server/goldens (CI, or a fresh checkout):
#   tools/goldens_fetch.sh
# Needs gh (GH_TOKEN in CI) and zstd. Checks the archive's SHA-256 before extracting it.
set -euo pipefail
cd "$(dirname "$0")/.."
TAG=$(sed -n 's/^TAG=//p' goldens/ARCHIVE)
SHA256=$(sed -n 's/^SHA256=//p' goldens/ARCHIVE)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
gh release download "$TAG" --repo "${GOLDENS_REPO:-Yutsa/altered-arena}" --pattern goldens.tar.zst --dir "$tmp"
echo "$SHA256  $tmp/goldens.tar.zst" | shasum -a 256 -c -
rm -rf server/goldens
mkdir -p server
zstd -dc "$tmp/goldens.tar.zst" | tar -xf - -C server
echo "server/goldens: $(find server/goldens -name '*.json' | wc -l) files ($TAG)"
