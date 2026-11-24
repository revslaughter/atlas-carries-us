#!/usr/bin/env bash
# Runs every check that CI runs: pinned versions, formatting, lints, unit
# tests, the golden files of both adapters, the end-to-end smoke test on each
# example, and this project's own map, whose docs must stay complete.
#
#   scripts/check.sh           everything
#   scripts/check.sh --quick   everything except the end-to-end run
#
# Run it inside the toolchain container (the devcontainer, or
# `docker run --rm -v "$PWD:/workspace" acu-toolchain scripts/check.sh`),
# where the tools match the pinned versions.
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
quick=false

# In CI the checkout belongs to another user than the container's. Trust the
# workspace and the corpus checkouts for this script's git calls only, so
# acu can read the commit for its source links.
export GIT_CONFIG_COUNT=10 \
  GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0="$root" \
  GIT_CONFIG_KEY_1=safe.directory GIT_CONFIG_VALUE_1="$root/corpus/go/chi" \
  GIT_CONFIG_KEY_2=safe.directory GIT_CONFIG_VALUE_2="$root/corpus/go/cobra" \
  GIT_CONFIG_KEY_3=safe.directory GIT_CONFIG_VALUE_3="$root/corpus/typescript/immer" \
  GIT_CONFIG_KEY_4=safe.directory GIT_CONFIG_VALUE_4="$root/corpus/typescript/hono" \
  GIT_CONFIG_KEY_5=safe.directory GIT_CONFIG_VALUE_5="$root/corpus/svelte/realworld" \
  GIT_CONFIG_KEY_6=safe.directory GIT_CONFIG_VALUE_6="$root/corpus/svelte/svelte-changelog" \
  GIT_CONFIG_KEY_7=safe.directory GIT_CONFIG_VALUE_7="$root/corpus/rust/ripgrep" \
  GIT_CONFIG_KEY_8=safe.directory GIT_CONFIG_VALUE_8="$root/corpus/rust/axum" \
  GIT_CONFIG_KEY_9=safe.directory GIT_CONFIG_VALUE_9="$root/corpus/javascript/commander"
[[ "${1:-}" == "--quick" ]] && quick=true

step() { printf '\n== %s\n' "$*"; }

step "Versions: pinned tools agree"
scripts/check-pins.sh
go version
node --version
cargo --version
nightly=$(sed -nE 's/^pub const NIGHTLY: &str = "([^"]+)";$/\1/p' adapters/rust/src/toolchain.rs)
rustup run "$nightly" rustdoc --version
rustup run "$nightly" rust-analyzer --version

step "Go adapter: format, vet, tests"
(
  cd adapters/go
  unformatted=$(gofmt -l .)
  if [[ -n "$unformatted" ]]; then echo "gofmt would change: $unformatted"; exit 1; fi
  go vet ./...
  go test ./...
)

step "Node workspace: install, format"
npm ci --no-audit --no-fund
npm run format:check

step "TypeScript adapter: type check, tests (golden file, schema, facts), build"
npm run check -w adapters/typescript
npm test -w adapters/typescript
npm run build -w adapters/typescript

step "Viewer: type check, tests, build"
npm run check -w viewer
npm test -w viewer
npm run build -w viewer

step "acu and the Rust adapter: format, lints, tests (golden file, schema, facts), release build"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
CI=${CI:-1} cargo test --workspace
cargo build --release --workspace

if $quick; then
  echo; echo "check.sh: quick checks passed"
  exit 0
fi

step "End to end: each example"
git submodule update --init --checkout --depth 1 corpus/go/chi corpus/go/cobra corpus/typescript/immer corpus/typescript/hono \
  corpus/svelte/realworld corpus/svelte/svelte-changelog corpus/rust/ripgrep corpus/rust/axum \
  corpus/javascript/commander
(cd adapters/go && go install ./cmd/acu-adapter-go)
# acu runs the TypeScript adapter from its build in this checkout, and the
# Rust adapter from beside itself in target/release.
mkdir -p out
for ex in lending chi cobra immer hono realworld svelte-changelog mixed lending-rust ripgrep axum \
  commander lending-js; do
  echo "-- $ex"
  ./target/release/acu extract -c "examples/$ex/atlas.yaml" -o "out/$ex.json"
  node tests/e2e/counts.mjs "out/$ex.json" "examples/$ex/counts.json"
  ./target/release/acu build -d "out/$ex.json" -o "out/site-$ex"
  node tests/e2e/smoke.mjs "out/site-$ex" "out/shot-$ex"
  node tests/e2e/smoke.mjs "out/site-$ex" "out/shot-$ex-light" --light
done

step "This project's own map: every unit, file and item documented"
./target/release/acu extract -c atlas.yaml -o out/self.json
node tests/e2e/docs.mjs out/self.json
./target/release/acu build -d out/self.json -o out/site-self
node tests/e2e/smoke.mjs out/site-self out/shot-self
node tests/e2e/smoke.mjs out/site-self out/shot-self-light --light

echo; echo "check.sh: all checks passed"
