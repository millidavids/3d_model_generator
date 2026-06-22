#!/usr/bin/env bash
# Build the reconstruction toolchain image for the NATIVE architecture.
#
#   arm64: run this on the Apple-Silicon Mac (now).
#   amd64: run this on the Windows/WSL2 box (later), then combine into a
#          multi-arch manifest. Do NOT cross-build amd64 on the Mac via QEMU —
#          C++ compiles run 5-8x slower under emulation.
#
# Usage: scripts/build-image.sh [target] [tag]
#   target: dev (default) | runtime
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${1:-dev}"
TAG="${2:-modelgen:${TARGET}}"
ARCH="$(uname -m)"

# amd64 also gets a containerized Blender; arm64 bakes host-native.
BLENDER_ARG=()
if [ "$ARCH" = "x86_64" ] && [ "$TARGET" = "runtime" ]; then
  BLENDER_ARG=(--build-arg INSTALL_BLENDER=1)
fi

echo "Building target=$TARGET tag=$TAG (native arch: $ARCH)..."
docker build --target "$TARGET" -t "$TAG" "${BLENDER_ARG[@]}" -f docker/Dockerfile .
echo "Done: $TAG"
