#!/bin/sh
set -e
cd "$(dirname "$0")/.."
limit=$(cat .github/wasm-size-limit)
cargo build --release --target wasm32-unknown-unknown
file=$(find target/wasm32-unknown/release -maxdepth 1 -name '*.wasm' | head -1)
[ -n "$file" ]
size=$(wc -c < "$file")
[ $size -le "$limit" ] || { echo "size $size > limit $limit" >&2; exit 1; }
