#!/bin/sh
set -e

# Checks which apply to all code including tests
cargo clippy --workspace --all-targets --all-features -- -D warnings -D deprecated -D clippy::perf \
          -D clippy::complexity -D clippy::style -D clippy::correctness \
          -D clippy::suspicious -D clippy::dbg_macro -D clippy::if_then_some_else_none \
          -D clippy::items-after-statements -D clippy::implicit_clone \
          -D clippy::cast_lossless -D clippy::manual_string_new \
          -D clippy::redundant_closure_for_method_calls \
          -D clippy::unused_self -D clippy::get_first

# Checks which apply to main code (not tests)
cargo clippy --workspace --all-features -- -D clippy::unwrap_used \
      -D clippy::indexing_slicing
