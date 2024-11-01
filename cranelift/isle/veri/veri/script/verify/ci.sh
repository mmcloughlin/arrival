#!/usr/bin/env bash

set -euo pipefail

./script/veri.sh "$@" -- \
    --filter include:first-rule-named \
    --filter exclude:tag:vector \
    --filter exclude:tag:broken \
    ;
