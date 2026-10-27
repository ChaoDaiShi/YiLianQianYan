#!/bin/bash
# Developer build entrypoint. Recipients should install the GitHub-built DMG.
set -euo pipefail
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"
if [[ "$(uname -s)" != Darwin ]]; then
  echo "Build macOS installers on macOS or use the GitHub Actions release workflow." >&2
  exit 1
fi
if [[ "$(uname -m)" != arm64 ]]; then
  echo "This release entrypoint targets Apple Silicon; use an arm64 Mac runner." >&2
  exit 1
fi
for tool in node npm rustc cargo xcode-select; do
  command -v "$tool" >/dev/null
done
xcode-select -p >/dev/null
export CARGO_BUILD_JOBS=1
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_RELEASE_DEBUG=0
export CARGO_PROFILE_RELEASE_STRIP=debuginfo
export CARGO_PROFILE_RELEASE_LTO=false
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export NODE_OPTIONS=--max-old-space-size=2048
export MACOSX_DEPLOYMENT_TARGET=12.0
export APPLE_SIGNING_IDENTITY=-
echo "Prefer the GitHub-built DMG for installation; local builds require significant free RAM and disk."
echo "Building v$(node -p 'require("./package.json").version') for aarch64-apple-darwin"
npm ci
npm --prefix frontend ci
npm run tauri -- build --target aarch64-apple-darwin --bundles app,dmg -- --locked
echo "Installers: $PROJECT_ROOT/target/aarch64-apple-darwin/release/bundle"
