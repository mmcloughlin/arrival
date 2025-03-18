#!/usr/bin/env bash

set -euo pipefail

# Clean build
cargo clean
cargo build --release

# Eval
RUST_LOG=info ./script/veri.sh \
    -p release \
    "$@" \
    -- \
    --results-to-log-dir \
    --timeout 30 \
    --num-threads 0 \
    --no-skip-todo \
    --filter include:tag:wasm_proposal_mvp \
    --filter exclude:tag:wasm_category_stack \
    --filter exclude:not:root:lower \
    --filter exclude:tag:vector \
    --filter exclude:tag:atomics \
    --filter exclude:tag:spectre \
    --filter exclude:tag:narrowfloat \
    --filter include:tag:clif_popcnt \
    --filter exclude:tag:amode_const \
    --filter exclude:tag:i128 \
    ;
