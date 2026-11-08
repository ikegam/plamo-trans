#!/bin/sh
# Download a PLaMo 3 Translate GGUF from Hugging Face (resumable) and check its SHA-256.
# Usage: download-model.sh [SIZE [QUANT]]     default: 8b Q4_K_M
#   SIZE: 0.8b 2b 8b 31b     QUANT: Q4_K_M Q3_K_M Q2_K ... (see the model repository)
# Also available as `trans --download SIZE [QUANT]` (this script is embedded in the binary).
# The model is under the PLaMo Community License; read it before use:
#   https://huggingface.co/pfnet/plamo-3-translate-31b-GGUF
set -eu
SIZE=${1:-8b}
QUANT=${2:-Q4_K_M}
case $SIZE in 0.8b | 2b | 8b | 31b) ;; *) echo "unknown size $SIZE (0.8b 2b 8b 31b)" >&2; exit 2 ;; esac
REPO=pfnet/plamo-3-translate-$SIZE-GGUF
FILE=plamo-3-translate-$SIZE-$QUANT.gguf
URL=https://huggingface.co/$REPO/resolve/main/$FILE
DIR=${XDG_DATA_HOME:-$HOME/.local/share}/plamo-trans
mkdir -p "$DIR"
cd "$DIR"
# Hugging Face reports the SHA-256 of LFS files in X-Linked-ETag
SHA=$(curl -sfI "$URL" | tr -d '\r"' | awk 'tolower($1) == "x-linked-etag:" { print $2 }') || true
[ -n "$SHA" ] || { echo "cannot find $FILE in $REPO" >&2; exit 1; }
echo "PLaMo 3 Translate is under the PLaMo Community License: https://huggingface.co/$REPO" >&2
i=0
until curl -fL -C - --retry 5 -o "$FILE.part" "$URL"; do
    i=$((i + 1)); [ $i -lt 30 ] || exit 1
    sleep 5
done
echo "checking SHA-256..." >&2
if command -v sha256sum >/dev/null; then
    echo "$SHA  $FILE.part" | sha256sum -c --quiet
else
    echo "$SHA  $FILE.part" | shasum -a 256 -c --quiet
fi
mv "$FILE.part" "$FILE"
echo "$DIR/$FILE"
