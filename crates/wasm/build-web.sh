#!/usr/bin/env bash
# Builds the Strata browser playground: wasm module + JS bindings into www/pkg.
#
# Prereqs:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version <wasm-bindgen version in Cargo.lock>
set -euo pipefail

cd "$(dirname "$0")/../.."

locked_version="$(grep -A1 'name = "wasm-bindgen"' Cargo.lock | grep version | head -1 | cut -d'"' -f2)"
cli_version="$(wasm-bindgen --version | cut -d' ' -f2)"
if [[ "$locked_version" != "$cli_version" ]]; then
  echo "error: wasm-bindgen-cli $cli_version does not match Cargo.lock ($locked_version)" >&2
  echo "  cargo install wasm-bindgen-cli --version $locked_version" >&2
  exit 1
fi

cargo build --target wasm32-unknown-unknown --release -p strata-wasm
wasm-bindgen --target web \
  --out-dir crates/wasm/www/pkg \
  target/wasm32-unknown-unknown/release/strata_wasm.wasm

echo
echo "Built. Serve the playground with:"
echo "  python3 -m http.server 8080 --directory crates/wasm/www"
echo "then open http://localhost:8080"
