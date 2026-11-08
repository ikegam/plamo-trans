#!/bin/sh
# Fill the version and checksums of a release into the Homebrew formula and the AUR PKGBUILD/.SRCINFO.
# Usage: packaging/update.sh VERSION SHA256SUMS
#   SHA256SUMS is the file attached to the GitHub release (download it with
#   gh release download vVERSION -p SHA256SUMS).
set -eu
V=$1
SUMS=$2
D=$(dirname "$0")
sum() {
    s=$(awk -v f="$1" '$2 == f { print $1 }' "$SUMS")
    [ -n "$s" ] || { echo "no checksum for $1 in $SUMS" >&2; exit 1; }
    echo "$s"
}
SRC=$(sum "plamo-trans-$V.tar.gz")

f=$D/homebrew/plamo-trans.rb
sed -i.bak "s/^  version \".*\"/  version \"$V\"/" "$f"
for t in macos-arm64 linux-x86_64 linux-arm64; do
    s=$(sum "plamo-trans-$V-$t.tar.gz")
    sed -i.bak "s/sha256 \"[0-9a-f]*\" # $t\$/sha256 \"$s\" # $t/" "$f"
done

for f in "$D/aur/PKGBUILD" "$D/aur/.SRCINFO"; do
    sed -i.bak \
        -e "s/^pkgver=.*/pkgver=$V/" -e "s/^pkgrel=.*/pkgrel=1/" \
        -e "s/^sha256sums=.*/sha256sums=('$SRC')/" \
        -e "s/^\tpkgver = .*/\tpkgver = $V/" -e "s/^\tpkgrel = .*/\tpkgrel = 1/" \
        -e "s/^\tsha256sums = .*/\tsha256sums = $SRC/" \
        -e "s|^\tsource = .*|\tsource = plamo-trans-$V.tar.gz::https://github.com/ikegam/plamo-trans/releases/download/v$V/plamo-trans-$V.tar.gz|" \
        "$f"
done
rm -f "$D"/homebrew/*.bak "$D"/aur/*.bak "$D"/aur/.SRCINFO.bak
echo "packaging updated to $V"
