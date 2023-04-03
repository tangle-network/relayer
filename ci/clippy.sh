#!/bin/sh
set -e

cargo clippy --workspace --all-targets --all-features -- -D warnings -D deprecated -D clippy::perf \
          -D clippy::complexity -D clippy::style -D clippy::correctness \
          -D clippy::suspicious

cargo clippy --workspace --all-features -- -D clippy::unwrap_used