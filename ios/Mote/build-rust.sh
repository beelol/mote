#!/bin/sh
set -eu

if [ "${PLATFORM_NAME:-}" != "iphonesimulator" ]; then
    echo "Mote's Xcode shell currently supports Simulator builds only." >&2
    exit 1
fi
if [ "${ARCHS:-}" != "arm64" ]; then
    echo "Mote's Simulator shell requires the Apple Silicon arm64 architecture." >&2
    exit 1
fi

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
repo_root="$(CDPATH= cd -- "$SRCROOT/../.." && pwd)"
export CARGO_TARGET_DIR="$repo_root/target"
export IPHONEOS_DEPLOYMENT_TARGET
unset SDKROOT

cd "$repo_root"
if [ "$CONFIGURATION" = "Release" ]; then
    cargo build --locked -p mote-ios --target aarch64-apple-ios-sim --release
    profile=release
else
    cargo build --locked -p mote-ios --target aarch64-apple-ios-sim
    profile=debug
fi
mkdir -p "$BUILT_PRODUCTS_DIR/rust"
cp "$CARGO_TARGET_DIR/aarch64-apple-ios-sim/$profile/libmote_ios.a" "$BUILT_PRODUCTS_DIR/rust/"
