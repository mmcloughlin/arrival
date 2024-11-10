#!/usr/bin/env bash

set -euo pipefail

RUST_LOG=info ./script/veri.sh "$@" -- \
    --results-to-log-dir \
    --num-threads 0 \
    --filter include:tag:wasm_proposal_mvp \
    --filter exclude:not:root:lower \
    --filter exclude:not:specified \
    --filter exclude:tag:vector \
    --filter exclude:tag:i128 \
    --filter exclude:tag:atomics \
    --filter exclude:tag:narrowfloat \
    ;
