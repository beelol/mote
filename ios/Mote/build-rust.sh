#!/bin/sh
set -eu

if [ "${ARCHS:-}" != "arm64" ]; then
    echo "Mote's iPad shell requires the arm64 architecture." >&2
    exit 1
fi
case "${PLATFORM_NAME:-}" in
    iphonesimulator) rust_target=aarch64-apple-ios-sim ;;
    iphoneos) rust_target=aarch64-apple-ios ;;
    *)
        echo "Mote's iPad shell requires an iOS device or Simulator platform." >&2
        exit 1
        ;;
esac

export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
repo_root="$(CDPATH= cd -- "$SRCROOT/../.." && pwd)"
export CARGO_TARGET_DIR="$repo_root/target"
export IPHONEOS_DEPLOYMENT_TARGET
unset SDKROOT

cd "$repo_root"
if [ "$CONFIGURATION" = "Release" ]; then
    cargo build --locked -p mote-ios --target "$rust_target" --release
    profile=release
else
    cargo build --locked -p mote-ios --target "$rust_target"
    profile=debug
fi
mkdir -p "$BUILT_PRODUCTS_DIR/rust"
cp "$CARGO_TARGET_DIR/$rust_target/$profile/libmote_ios.a" "$BUILT_PRODUCTS_DIR/rust/"
