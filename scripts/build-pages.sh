#!/bin/sh
set -eu

cargo run --release --locked -- build-web-bundle
cargo run --release --locked -- assemble-pages
cargo build --release --locked --target wasm32-unknown-unknown --lib
wasm-bindgen \
  --target web \
  --out-dir dist/pkg \
  --out-name fo2_dps \
  target/wasm32-unknown-unknown/release/fo2_dps.wasm

test -f dist/index.html
test -f dist/data/app-data.v1.json
test -f dist/pkg/fo2_dps.js
test -f dist/pkg/fo2_dps_bg.wasm

printf 'Static Pages build ready in dist/\n'
