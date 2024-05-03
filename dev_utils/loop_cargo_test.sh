#!/usr/bin/env bash
set -eou pipefail

fd "\\.rs|\\.py" | CARGO_TARGET_DIR=target_bacon RUST_BACKTRACE=1 RUST_LOG=debug entr cargo test $@
