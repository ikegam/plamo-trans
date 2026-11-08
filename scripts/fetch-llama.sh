#!/bin/sh
# Fetch llama.cpp at the commit in llama.cpp.commit into ./llama.cpp and apply patches/*.patch.
# Usage: scripts/fetch-llama.sh [COMMIT]
#
# Only that one commit and only the directories needed to build libllama/ggml are downloaded
# (shallow, sparse, partial clone), so this takes seconds, not a full clone.
# If ./llama.cpp already holds an unpacked llama.cpp tree without patches (e.g. a release tarball, as the AUR
# package does), the patches are applied to it instead of fetching. cargo build runs this automatically.
set -eu
cd "$(dirname "$0")/.."
REF=${1:-$(cat llama.cpp.commit)}
DEST=llama.cpp
STAMP=$DEST/.plamo-trans-commit

if [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$REF" ]; then
    exit 0
fi
if [ -f "$STAMP" ] || { [ -d "$DEST" ] && [ ! -f "$DEST/CMakeLists.txt" ]; }; then
    rm -rf "$DEST"   # another commit, or a broken tree
fi
if [ ! -d "$DEST" ]; then
    echo "fetching llama.cpp $REF" >&2
    git init -q "$DEST"
    git -C "$DEST" remote add origin https://github.com/ggml-org/llama.cpp
    git -C "$DEST" sparse-checkout set --no-cone \
        '/*' '!/*/' /cmake/ /include/ /src/ /licenses/ /vendor/CMakeLists.txt \
        /ggml/CMakeLists.txt /ggml/cmake/ /ggml/include/ '/ggml/src/*' '!/ggml/src/*/' \
        /ggml/src/ggml-cpu/ /ggml/src/ggml-cuda/ /ggml/src/ggml-vulkan/ /ggml/src/ggml-metal/ /ggml/src/ggml-blas/
    git -C "$DEST" fetch -q --depth 1 --filter=blob:none origin "$REF"
    git -C "$DEST" checkout -q FETCH_HEAD
fi
# vendor/ only feeds mtmd and common, which are not built
echo "# plamo-trans: mtmd/common dependencies are not used" > "$DEST/vendor/CMakeLists.txt"
for p in patches/*.patch; do
    [ -e "$p" ] || continue
    patch -s -p1 -d "$DEST" < "$p"
done
echo "$REF" > "$STAMP"
