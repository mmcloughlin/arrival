#!/usr/bin/env bash

set -euo pipefail

RUST_LOG=info ./script/veri.sh \
    -p release \
    "$@" \
    -- \
    --results-to-log-dir \
    --num-threads 0 \
    --filter include:tag:wasm_proposal_mvp \
    --filter exclude:tag:wasm_category_stack \
    --filter exclude:not:root:lower \
    --filter exclude:tag:vector \
    --filter exclude:tag:i128 \
    --filter exclude:tag:atomics \
    --filter exclude:tag:spectre \
    --filter exclude:tag:narrowfloat \
    ;
