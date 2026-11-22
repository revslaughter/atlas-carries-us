#!/usr/bin/env bash
# Runs every check that CI runs: formatting, lints, unit tests, the golden
# files, and the end-to-end smoke test on each example.
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
[[ "${1:-}" == "--quick" ]] && quick=true

step() { printf '\n== %s\n' "$*"; }

step "Versions"
go version
node --version
cargo --version

step "Go adapter: format, vet, tests"
(
  cd adapters/go
  unformatted=$(gofmt -l .)
  if [[ -n "$unformatted" ]]; then echo "gofmt would change: $unformatted"; exit 1; fi
  go vet ./...
  go test ./...
)

step "Viewer: install, type check, tests, build"
npm ci --no-audit --no-fund
npm run check -w viewer
npm test -w viewer
npm run build -w viewer

step "acu: format, lints, tests, release build"
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release

if $quick; then
  echo; echo "check.sh: quick checks passed"
  exit 0
fi

step "End to end: each example"
# The workspace may belong to another user than the container's; tell git so.
git -c safe.directory="$root" submodule update --init --checkout --depth 1 corpus/go/chi corpus/go/cobra
(cd adapters/go && go install ./cmd/acu-adapter-go)
mkdir -p out
for ex in lending chi cobra; do
  echo "-- $ex"
  ./target/release/acu extract -c "examples/$ex/atlas.yaml" -o "out/$ex.json"
  node tests/e2e/counts.mjs "out/$ex.json" "examples/$ex/counts.json"
  ./target/release/acu build -d "out/$ex.json" -o "out/site-$ex"
  node tests/e2e/smoke.mjs "out/site-$ex" "out/shot-$ex"
  node tests/e2e/smoke.mjs "out/site-$ex" "out/shot-$ex-light" --light
done

echo; echo "check.sh: all checks passed"
