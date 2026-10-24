#!/bin/sh
# Builds the package the web app imports: pkg/omnigenesys_wasm.js (+ .wasm,
# + .d.ts types). Needs the wasm32 target and wasm-bindgen-cli 0.2.129:
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli --version 0.2.129 --locked
set -e
cd "$(dirname "$0")/../.."
cargo build --profile wasm-release --target wasm32-unknown-unknown -p omnigenesys_wasm
rm -rf crates/omnigenesys-wasm/pkg
wasm-bindgen --target web --out-dir crates/omnigenesys-wasm/pkg \
  target/wasm32-unknown-unknown/wasm-release/omnigenesys_wasm.wasm
ls -l crates/omnigenesys-wasm/pkg
