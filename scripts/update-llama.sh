#!/bin/sh
# Re-vendor llama.cpp into vendor/llama.cpp at a given commit and apply patches/*.patch.
# Usage: scripts/update-llama.sh [COMMIT]     (default: the commit in vendor/llama.cpp/COMMIT)
#
# Only the parts needed to build libllama/ggml (CPU, CUDA, Vulkan, Metal, BLAS backends) are copied.
# The fetch is shallow (one commit), so this takes seconds, not minutes.
set -eu
cd "$(dirname "$0")/.."
DEST=vendor/llama.cpp
REF=${1:-$(cat "$DEST/COMMIT" 2>/dev/null || true)}
[ -n "$REF" ] || { echo "usage: $0 COMMIT" >&2; exit 2; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
git init -q "$TMP/src"
git -C "$TMP/src" fetch -q --depth 1 https://github.com/ggml-org/llama.cpp "$REF"
git -C "$TMP/src" checkout -q FETCH_HEAD
REF=$(git -C "$TMP/src" rev-parse HEAD)

KEEP="CMakeLists.txt LICENSE licenses cmake include src
ggml/CMakeLists.txt ggml/cmake ggml/include
ggml/src/ggml-cpu ggml/src/ggml-cuda ggml/src/ggml-vulkan ggml/src/ggml-metal ggml/src/ggml-blas"

rm -rf "$DEST"
mkdir -p "$DEST/ggml/src" "$DEST/vendor"
for p in $KEEP; do
    mkdir -p "$DEST/$(dirname "$p")"
    cp -a "$TMP/src/$p" "$DEST/$p"
done
# top-level files of ggml/src (sources, CMakeLists), without the unused backend directories
find "$TMP/src/ggml/src" -maxdepth 1 -type f -exec cp -a {} "$DEST/ggml/src/" \;
# vendor/ only feeds mtmd and common, which are not built; keep an empty stub so add_subdirectory(vendor) works
echo "# plamo-trans: third-party libraries for mtmd/common are not vendored (not built)" > "$DEST/vendor/CMakeLists.txt"

for p in patches/*.patch; do
    [ -e "$p" ] || continue
    patch -s -p1 -d "$DEST" < "$p"
done
echo "$REF" > "$DEST/COMMIT"
du -sh "$DEST"
echo "vendored llama.cpp $REF"
