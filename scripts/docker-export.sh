#!/usr/bin/env bash
# Builds the Dockerfile's export stage and writes acu, acu-adapter-go,
# acu-adapter-python, acu-adapter-rust and acu-adapter-typescript/ to a
# folder on the host.
#
#   scripts/docker-export.sh                           out/docker, this machine's platform
#   scripts/docker-export.sh --platform darwin/arm64   another platform
#   scripts/docker-export.sh out/acu                   another folder
#   scripts/docker-export.sh -- --build-arg REGISTRY=mirror.gcr.io
#                                                      extra arguments for docker build
#
# The Dockerfile builds in containers of the build machine's own platform and
# cross-compiles for the platform that --platform names, so no step runs
# under emulation. The platforms: linux, darwin (macOS) and windows, each on
# amd64 or arm64.
set -euo pipefail
caller=$PWD
cd "$(dirname "$0")/.."

usage() {
  cat <<'EOF'
Usage: scripts/docker-export.sh [--platform <os/arch>] [<folder>] [-- <docker build arguments>...]

Builds the Dockerfile's export stage and writes acu, acu-adapter-go,
acu-adapter-python, acu-adapter-rust and acu-adapter-typescript/ to <folder>
(default: out/docker).

Options:
  --platform <os/arch>  The platform to build for: linux/amd64, linux/arm64,
                        darwin/amd64, darwin/arm64, windows/amd64 or
                        windows/arm64. The default follows this machine, which
                        Docker counts as linux, also on macOS and Windows.
                        The build cross-compiles, so every platform builds at
                        about the same speed.
  -h, --help            Print this help.
  --                    Pass every later argument to docker build, for example
                        --build-arg REGISTRY=mirror.gcr.io, or
                        --secret id=extra_ca,src=<ca.pem> behind a TLS-inspecting proxy.

On windows, acu, acu-adapter-go and acu-adapter-rust end in .exe, and the
Python adapter takes the name acu-adapter-python.pyz. The programs need Go,
Node, Python 3.12 or later and the pinned Rust nightly on the machine that
runs them. docs/install.md tells how to install them.
EOF
}

platform=""
dest=out/docker
dest_set=false
extra=()

while (($#)); do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    --platform)
      if (($# < 2)) || [[ -z "$2" ]]; then
        echo "docker-export: --platform needs a value; give one such as darwin/arm64." >&2
        exit 2
      fi
      platform=$2
      shift 2
      ;;
    --platform=*)
      platform=${1#--platform=}
      shift
      ;;
    --)
      shift
      extra=("$@")
      break
      ;;
    -*)
      echo "docker-export: the option $1 has no meaning here; run scripts/docker-export.sh --help, or pass it to docker build after --." >&2
      exit 2
      ;;
    *)
      if $dest_set; then
        echo "docker-export: the folder $dest already came first, so $1 has no place; give one folder." >&2
        exit 2
      fi
      # A folder that the caller names counts from the caller's folder; the
      # default counts from the repository root.
      if [[ "$1" == /* ]]; then dest=$1; else dest=$caller/$1; fi
      dest_set=true
      shift
      ;;
  esac
done

platform_args=()
if [[ -n "$platform" ]]; then
  case "$platform" in
    linux/amd64 | linux/arm64 | darwin/amd64 | darwin/arm64 | windows/amd64 | windows/arm64) ;;
    *)
      echo "docker-export: the Dockerfile cannot build for the platform $platform; choose linux, darwin or windows, each with amd64 or arm64, such as darwin/arm64." >&2
      exit 2
      ;;
  esac
  platform_args=(--platform "$platform")
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker-export: docker sits on no folder of the PATH; install Docker, then run this script again." >&2
  exit 1
fi

echo "docker-export: building the export stage${platform:+ for $platform} into $dest" >&2
# The ${a[@]+"${a[@]}"} form keeps an empty array safe under set -u in bash 3.2,
# the bash that macOS ships. --provenance=false keeps buildx from writing a
# provenance.json beside the programs; an argument after -- can still turn it
# back on.
docker build ${platform_args[@]+"${platform_args[@]}"} --target export --provenance=false --output "type=local,dest=$dest" ${extra[@]+"${extra[@]}"} .

echo "docker-export: $dest now holds:" >&2
ls -l "$dest"
