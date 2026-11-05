#!/bin/sh
# Clone and build llama.cpp (shared libraries) into ./llama.cpp for plamo-trans.
# Usage: scripts/build-llama.sh [cmake options...]
#   CUDA:   scripts/build-llama.sh -DGGML_CUDA=ON
#   Vulkan: scripts/build-llama.sh -DGGML_VULKAN=ON
#   (no option = CPU only)
# LLAMA_REF selects the llama.cpp commit (default: the one plamo-trans was tested with).
set -eu
cd "$(dirname "$0")/.."
REF=${LLAMA_REF:-3259ab6e5dd86fad1931962d784702cb53c7877b}
if [ ! -d llama.cpp ]; then
    git clone https://github.com/ggml-org/llama.cpp llama.cpp
fi
git -C llama.cpp fetch -q origin
git -C llama.cpp checkout -q "$REF"
cmake -S llama.cpp -B llama.cpp/build -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON \
    -DLLAMA_BUILD_TESTS=OFF -DLLAMA_BUILD_EXAMPLES=OFF -DLLAMA_BUILD_SERVER=OFF "$@"
cmake --build llama.cpp/build -j"$(nproc 2>/dev/null || echo 4)" --target llama
