#!/usr/bin/env bash
# Builds the Dockerfile's export stage and writes acu, acu-adapter-go,
# acu-adapter-rust and acu-adapter-typescript/ to a folder on the host.
#
#   scripts/docker-export.sh                          out/docker, this machine's architecture
#   scripts/docker-export.sh --platform linux/arm64   another Linux architecture
#   scripts/docker-export.sh out/acu                  another folder
#   scripts/docker-export.sh -- --build-arg REGISTRY=mirror.gcr.io
#                                                     extra arguments for docker build
#
# The Dockerfile builds in Linux containers, so it makes Linux programs only.
# A platform other than the machine's own runs the build under emulation
# (QEMU, through binfmt), which takes far longer than a native build.
set -euo pipefail
caller=$PWD
cd "$(dirname "$0")/.."

usage() {
  cat <<'EOF'
Usage: scripts/docker-export.sh [--platform <os/arch>] [<folder>] [-- <docker build arguments>...]

Builds the Dockerfile's export stage and writes acu, acu-adapter-go,
acu-adapter-rust and acu-adapter-typescript/ to <folder> (default: out/docker).

Options:
  --platform <os/arch>  The platform to build for, for example linux/amd64 or
                        linux/arm64. The default follows this machine. Only
                        linux works: the build runs in Linux containers. Another
                        architecture runs under emulation and takes much longer;
                        set it up once with
                        docker run --privileged --rm tonistiigi/binfmt --install <arch>
  -h, --help            Print this help.
  --                    Pass every later argument to docker build, for example
                        --build-arg REGISTRY=mirror.gcr.io, or
                        --secret id=extra_ca,src=<ca.pem> behind a TLS-inspecting proxy.

The programs need Go, Node and the pinned Rust nightly at run time.
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
        echo "docker-export: --platform needs a value; give one such as linux/arm64." >&2
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
  os=${platform%%/*}
  if [[ "$os" != linux || "$platform" != */* ]]; then
    echo "docker-export: the Dockerfile builds in Linux containers, so it cannot make programs for $platform; choose a Linux platform such as linux/amd64 or linux/arm64." >&2
    exit 2
  fi
  platform_args=(--platform "$platform")
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "docker-export: docker sits on no folder of the PATH; install Docker, then run this script again." >&2
  exit 1
fi

echo "docker-export: building the export stage${platform:+ for $platform} into $dest" >&2
# The ${a[@]+"${a[@]}"} form keeps an empty array safe under set -u in bash 3.2,
# the bash that macOS ships.
docker build ${platform_args[@]+"${platform_args[@]}"} --target export --output "type=local,dest=$dest" ${extra[@]+"${extra[@]}"} .

echo "docker-export: $dest now holds:" >&2
ls -l "$dest"
