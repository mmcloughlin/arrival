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
    --num-threads 0 \
    --no-skip-todo \
    --filter include:tag:wasm_proposal_mvp \
    --filter exclude:tag:wasm_category_stack \
    --filter exclude:not:root:lower \
    --filter exclude:tag:vector \
    --filter exclude:tag:atomics \
    --filter exclude:tag:spectre \
    --filter exclude:tag:narrowfloat \
    --filter exclude:tag:clif_f32const \
    --filter exclude:tag:clif_f64const \
    --filter include:tag:clif_popcnt \
    --filter exclude:tag:i128 \
    ;
