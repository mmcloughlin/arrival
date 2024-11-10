#!/usr/bin/env bash

set -euo pipefail

RUST_LOG=info ./script/veri.sh "$@" -- \
    --results-to-log-dir \
    --filter include:first-rule-named \
    --filter exclude:tag:vector \
    --filter exclude:tag:slow \
    --filter exclude:tag:i128 \
    --filter exclude:tag:atomics \
    ;
