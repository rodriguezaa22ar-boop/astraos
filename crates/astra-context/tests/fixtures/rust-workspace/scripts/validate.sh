#!/bin/sh
set -eu

cargo fmt --all --check
cargo test --workspace
