#!/usr/bin/env bash
# Packs the programs of one platform, as the Dockerfile's export stage writes
# them, into one release archive: a .tar.gz for Linux and macOS, a .zip for
# Windows. .github/workflows/release.yml runs it for each platform.
#
#   scripts/release-pack.sh <folder> <version> <os/arch> <dest>
#
#   <folder>   the export stage's output (scripts/docker-export.sh writes one)
#   <version>  the release, such as v0.2.0
#   <os/arch>  the platform of the programs, such as darwin/arm64
#   <dest>     the folder that receives the archive
#
# The archive holds one folder, acu-<version>-<os>-<arch>/, with acu, its
# four adapters, LICENSE and README.md. The script checks that the export
# holds exactly those programs, sets the executable mode, and sorts the
# entries and fixes their times and owners, so that two runs on the same
# programs give the same bytes. SOURCE_DATE_EPOCH sets the time; without it,
# the script takes the time of the last commit. It prints the archive's path
# on standard output. It needs GNU tar and GNU touch, as Linux carries.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)

if (($# != 4)); then
  echo "release-pack: give four arguments: <folder> <version> <os/arch> <dest>; see the comment at the top of scripts/release-pack.sh." >&2
  exit 2
fi
src=$1 version=$2 platform=$3 dest=$4

case "$platform" in
  linux/amd64 | linux/arm64 | darwin/amd64 | darwin/arm64) os=${platform%/*} ext='' py=acu-adapter-python ;;
  windows/amd64 | windows/arm64) os=windows ext=.exe py=acu-adapter-python.pyz ;;
  *)
    echo "release-pack: the platform $platform has no release; choose linux, darwin or windows, each with amd64 or arm64." >&2
    exit 2
    ;;
esac
arch=${platform#*/}

if [[ ! "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]]; then
  echo "release-pack: the version $version holds characters that a file name cannot carry; give one such as v0.2.0." >&2
  exit 2
fi
if [[ ! -d "$src" ]]; then
  echo "release-pack: the folder $src does not exist; build the export stage first (scripts/docker-export.sh)." >&2
  exit 1
fi

want=$(printf '%s\n' "acu$ext" "acu-adapter-go$ext" "$py" "acu-adapter-rust$ext" acu-adapter-typescript | LC_ALL=C sort)
have=$(find "$src" -mindepth 1 -maxdepth 1 -exec basename {} \; | LC_ALL=C sort)
if [[ "$have" != "$want" ]]; then
  echo "release-pack: the folder $src holds other files than acu and its four adapters for $platform; build the export stage for that platform again." >&2
  printf 'release-pack: want:\n%s\nrelease-pack: have:\n%s\n' "$want" "$have" >&2
  exit 1
fi

name="acu-$version-$os-$arch"
epoch=${SOURCE_DATE_EPOCH:-$(git -C "$root" log -1 --format=%ct 2>/dev/null || date +%s)}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir "$work/$name"
cp -R "$src"/. "$work/$name/"
cp "$root/LICENSE" "$root/README.md" "$work/$name/"
# An artifact upload or a copy from another system can drop the executable
# mode, so set it again. The Python adapter on Windows runs through `python`
# and needs no mode, but a mode does no harm there.
chmod -R u+rwX,go+rX,go-w "$work/$name"
chmod 755 "$work/$name/acu$ext" "$work/$name/acu-adapter-go$ext" "$work/$name/acu-adapter-rust$ext" "$work/$name/$py" \
  "$work/$name/acu-adapter-typescript/dist/src/main.js"
# One time for every entry, so that the archive depends on the programs
# alone. zip reads the time from the files.
find "$work/$name" -exec touch -h -d "@$epoch" {} +

mkdir -p "$dest"
dest=$(cd "$dest" && pwd)
if [[ "$os" == windows ]]; then
  out="$dest/$name.zip"
  rm -f "$out"
  # -X leaves out the extra fields that hold owners and times beyond the
  # basic ones; -D leaves out entries for folders.
  (cd "$work" && find "$name" -type f | LC_ALL=C sort | TZ=UTC zip -q -X -D "$out" -@)
else
  out="$dest/$name.tar.gz"
  (cd "$work" && tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$epoch" \
    --format=gnu -cf - "$name" | gzip -n -9 >"$out")
fi
echo "release-pack: wrote $out" >&2
echo "$out"
