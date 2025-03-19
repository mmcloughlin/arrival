#!/usr/bin/env bash

set -euo pipefail

# Options.
function usage() {
    echo "Usage: ${0} -n <name> -t <timeout>"
    exit 2
}

name="adhoc"
timeout=60
while getopts "n:t:" opt; do
    case "${opt}" in
        n) name="${OPTARG}" ;;
        t) timeout="${OPTARG}" ;;
        *) usage ;;
    esac
done

[[ -n "${name}" ]]
[[ -n "${EVAL_DATA_DIR}" ]]

# Setup temporary directory.
tmp_dir=$(mktemp -d)

# Setup results directory.
timestamp=$(date -u '+%Y-%m-%dT%T')
output_dir="${EVAL_DATA_DIR}/${timestamp}-${name}"
mkdir -p "${output_dir}"

# Clean build
cargo clean

# Eval
RUST_LOG=info \
cargo run --bin veri --release -- \
    --codegen-crate-dir ../../../codegen/ \
    --work-dir "${tmp_dir}" \
    --name aarch64 \
    --log-dir "${output_dir}" \
    --results-to-log-dir \
    --timeout "${timeout}" \
    --num-threads 0 \
    --no-skip-todo \
    \
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
