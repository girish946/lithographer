#!/usr/bin/env bash
# Build the litho CLI and copy it into src-tauri/binaries/ with the target-triple suffix
# required by Tauri externalBin sidecars.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LITHO_DIR="$(cd "$ROOT/../litho" && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"

mkdir -p "$BIN_DIR"

echo "Building litho CLI (release)..."
(cd "$LITHO_DIR" && cargo build --release --bin litho)

TRIPLE="$(rustc --print host-tuple)"
DEST="$BIN_DIR/litho-${TRIPLE}"

cp "$LITHO_DIR/target/release/litho" "$DEST"
chmod +x "$DEST"
echo "Sidecar ready: $DEST"