#!/usr/bin/env bash
# Checks that every pinned tool version agrees across the files that pin it.
# The Dockerfile holds the versions that CI and the devcontainer use; the
# other files keep builds outside the container on the same versions.
#
#   Go          Dockerfile GO_IMAGE tag      = toolchain line in adapters/go/go.mod
#   Node        Dockerfile NODE_IMAGE tag    = .node-version
#   Rust        Dockerfile RUST_IMAGE tag    = channel in rust-toolchain.toml
#   Playwright  Dockerfile PLAYWRIGHT_VERSION = tests/e2e/package.json = package-lock.json
#   TypeScript  viewer/package.json = adapters/typescript/package.json = package-lock.json
#   Svelte      adapters/typescript/package.json = package-lock.json (svelte, svelte2tsx)
#   Nightly     Dockerfile RUST_NIGHTLY      = NIGHTLY in adapters/rust/src/toolchain.rs
#   rustdoc-types  adapters/rust/Cargo.toml (exact) = Cargo.lock
#   zig         Dockerfile ZIG_VERSION       = ziglang==<version> in CONTRIBUTING.md
#   cargo-zigbuild  Dockerfile CARGO_ZIGBUILD_VERSION = cargo-zigbuild@<version> in CONTRIBUTING.md
#
# It also checks that each base image carries a digest, that each zig
# archive carries a SHA-256 sum, and that TypeScript stays on major version
# 6, whose API the adapters and the Svelte tools use.
set -euo pipefail
cd "$(dirname "$0")/.."

fail=0
agree() { # name, then pairs of "where" "value"
  local name=$1; shift
  local first=$2 ok=1 out=""
  while (($#)); do
    out+="    $1: ${2:-<missing>}"$'\n'
    [[ "$2" == "$first" && -n "$2" ]] || ok=0
    shift 2
  done
  if ((ok)); then echo "pins: $name $first"; else echo "pins: $name versions disagree:"; printf '%s' "$out"; fail=1; fi
}

image_arg() { grep -E "^ARG $1=" Dockerfile | head -1 | sed -E 's/^ARG [A-Z_]+=//'; }

go_image=$(image_arg GO_IMAGE)
node_image=$(image_arg NODE_IMAGE)
rust_image=$(image_arg RUST_IMAGE)

for img in "$go_image" "$node_image" "$rust_image"; do
  if [[ "$img" != *@sha256:* ]]; then echo "pins: the base image $img has no digest"; fail=1; fi
done

tag_version() { sed -E 's/^.*:([0-9]+\.[0-9]+\.[0-9]+).*$/\1/' <<<"$1"; }

agree Go \
  "Dockerfile GO_IMAGE" "$(tag_version "$go_image")" \
  "adapters/go/go.mod toolchain" "$(sed -nE 's/^toolchain go([0-9.]+)$/\1/p' adapters/go/go.mod)"

agree Node \
  "Dockerfile NODE_IMAGE" "$(tag_version "$node_image")" \
  ".node-version" "$(tr -d ' \n' < .node-version)"

agree Rust \
  "Dockerfile RUST_IMAGE" "$(tag_version "$rust_image")" \
  "rust-toolchain.toml" "$(sed -nE 's/^channel *= *"([^"]+)".*$/\1/p' rust-toolchain.toml)"

agree Playwright \
  "Dockerfile PLAYWRIGHT_VERSION" "$(sed -nE 's/^ARG PLAYWRIGHT_VERSION=(.+)$/\1/p' Dockerfile)" \
  "tests/e2e/package.json" "$(sed -nE 's/^ *"playwright": *"([^"]+)".*$/\1/p' tests/e2e/package.json)" \
  "package-lock.json" "$(node -e 'const l=require("./package-lock.json");process.stdout.write(l.packages["node_modules/playwright"]?.version??"")' 2>/dev/null || true)"

npm_pin() { sed -nE "s/^ *\"$1\": *\"([^\"]+)\".*$/\1/p" "$2" | head -1; }

agree TypeScript \
  "viewer/package.json" "$(npm_pin typescript viewer/package.json)" \
  "adapters/typescript/package.json" "$(npm_pin typescript adapters/typescript/package.json)" \
  "package-lock.json" "$(node -e 'const l=require("./package-lock.json");process.stdout.write(l.packages["node_modules/typescript"]?.version??"")' 2>/dev/null || true)"

ts_version=$(npm_pin typescript adapters/typescript/package.json)
if [[ "$ts_version" != 6.* ]]; then
  echo "pins: TypeScript $ts_version leaves major version 6; the adapters need the TypeScript 6 API (see docs/adapters.md)."
  fail=1
fi

lock_pin() { node -e 'const l=require("./package-lock.json");process.stdout.write(l.packages["node_modules/"+process.argv[1]]?.version??"")' "$1" 2>/dev/null || true; }

for pkg in svelte svelte2tsx; do
  agree "$pkg" \
    "adapters/typescript/package.json" "$(npm_pin "$pkg" adapters/typescript/package.json)" \
    "package-lock.json" "$(lock_pin "$pkg")"
done

agree "Rust nightly" \
  "Dockerfile RUST_NIGHTLY" "$(sed -nE 's/^ARG RUST_NIGHTLY=(.+)$/\1/p' Dockerfile)" \
  "adapters/rust/src/toolchain.rs NIGHTLY" "$(sed -nE 's/^pub const NIGHTLY: &str = "([^"]+)";$/\1/p' adapters/rust/src/toolchain.rs)"

agree rustdoc-types \
  "adapters/rust/Cargo.toml" "$(sed -nE 's/^rustdoc-types = "=([^"]+)"$/\1/p' adapters/rust/Cargo.toml)" \
  "Cargo.lock" "$(awk '/^name = "rustdoc-types"$/{getline; gsub(/version = |"/, ""); print}' Cargo.lock)"

agree zig \
  "Dockerfile ZIG_VERSION" "$(sed -nE 's/^ARG ZIG_VERSION=(.+)$/\1/p' Dockerfile)" \
  "CONTRIBUTING.md" "$(sed -nE 's/^.*ziglang==([0-9][0-9.]*).*$/\1/p' CONTRIBUTING.md | head -1)"

for arch in AMD64 ARM64; do
  if ! grep -qE "^ARG ZIG_SHA256_${arch}=[0-9a-f]{64}$" Dockerfile; then
    echo "pins: the Dockerfile gives no SHA-256 sum in ZIG_SHA256_${arch}"
    fail=1
  fi
done

agree cargo-zigbuild \
  "Dockerfile CARGO_ZIGBUILD_VERSION" "$(sed -nE 's/^ARG CARGO_ZIGBUILD_VERSION=(.+)$/\1/p' Dockerfile)" \
  "CONTRIBUTING.md" "$(sed -nE 's/^.*cargo-zigbuild@([0-9][0-9.]*).*$/\1/p' CONTRIBUTING.md | head -1)"

if ((fail)); then
  echo "pins: change every file in a group together (see CONTRIBUTING.md, \"Versions\")."
  exit 1
fi
