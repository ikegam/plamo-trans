#!/bin/sh
# Download a PLaMo 3 Translate GGUF from Hugging Face (resumable) and check its SHA-256.
# Usage: scripts/download-model.sh [SIZE [QUANT]]     default: 31b Q4_K_M
#   SIZE: 0.8b 2b 8b 31b     QUANT: Q4_K_M Q3_K_M Q2_K ... (see the model repository)
# The model is under the PLaMo Community License; read it before use:
#   https://huggingface.co/pfnet/plamo-3-translate-31b-GGUF
set -eu
SIZE=${1:-31b}
QUANT=${2:-Q4_K_M}
REPO=pfnet/plamo-3-translate-$SIZE-GGUF
FILE=plamo-3-translate-$SIZE-$QUANT.gguf
DIR=${XDG_DATA_HOME:-$HOME/.local/share}/plamo-trans
mkdir -p "$DIR"
cd "$DIR"
SHA=$(curl -sfL "https://huggingface.co/api/models/$REPO/tree/main" |
    python3 -c "import json,sys; print(next(f['lfs']['oid'] for f in json.load(sys.stdin) if f['path']=='$FILE'))") || {
    echo "cannot find $FILE in $REPO" >&2; exit 1; }
URL=https://huggingface.co/$REPO/resolve/main/$FILE
i=0
until curl -fL -C - --retry 5 -o "$FILE.part" "$URL"; do
    i=$((i + 1)); [ $i -lt 30 ] || exit 1
    sleep 5
done
echo "checking SHA-256..."
echo "$SHA  $FILE.part" | sha256sum -c --quiet
mv "$FILE.part" "$FILE"
echo "$DIR/$FILE"
