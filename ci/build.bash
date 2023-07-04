#!/usr/bin/env bash
# Script for building your rust projects.
set -e

source ci/common.bash

# $1 {path} = Path to cross/cargo executable
CROSS=$1
# $2 {string} = <Target Triple> e.g. x86_64-pc-windows-msvc
TARGET_TRIPLE=$2
# $3 {boolean} = Are we building for deployment?
RELEASE_BUILD=$3
# $4 {boolean} = Are we building for docker?
RELEASE_DOCKER=$4

required_arg $CROSS 'CROSS'
required_arg $TARGET_TRIPLE '<Target Triple>'

if [ -z "$RELEASE_BUILD" ]; then
    $CROSS build -p webb-relayer --target "$TARGET_TRIPLE" --features cli
else
    $CROSS build -p webb-relayer --target "$TARGET_TRIPLE" --features cli --release
fi

if [ -z "$RELEASE_DOCKER" ]; then
    # Do nothing.
    true
else
    # build for docker
    $CROSS build -p webb-relayer --target "$TARGET_TRIPLE" --features cli --release
    mkdir -p build && cp target/"$TARGET_TRIPLE"/release/webb-relayer build/webb-relayer
fi
