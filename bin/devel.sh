#!/bin/sh

set -e

BIN_DIR=$(dirname $0)

cd "${BIN_DIR}/.."
cargo build --workspace
cargo run -- ${@}
