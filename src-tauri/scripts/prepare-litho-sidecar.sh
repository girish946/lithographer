#!/usr/bin/env bash
# Build the litho CLI and copy it into src-tauri/binaries/ with the target-triple suffix
# required by Tauri externalBin sidecars.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
LITHO_DIR="$(cd "$ROOT/../litho" && pwd)"
BIN_DIR="$ROOT/src-tauri/binaries"

mkdir -p "$BIN_DIR"

echo "Building litho CLI (release, real-io)..."
(cd "$LITHO_DIR" && cargo build --release --no-default-features --features real-io --bin litho)

TRIPLE="$(rustc --print host-tuple)"
DEST="$BIN_DIR/litho-${TRIPLE}"

cp "$LITHO_DIR/target/release/litho" "$DEST"
chmod +x "$DEST"

# Post-write verify needs LinuxBufferedDeviceReader (not O_DIRECT new_reader).
# Use process substitution — `grep -q` closes the pipe early and SIGPIPE + pipefail
# would otherwise make `strings | grep -q` look like a failed check.
if ! grep -Fq "new_verify_reader" < <(strings "$DEST"); then
    echo "error: litho sidecar is missing buffered verification I/O (stale litho build?)" >&2
    exit 1
fi

echo "Sidecar ready: $DEST"