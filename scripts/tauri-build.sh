#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

npm exec tauri "$@"
bash src-tauri/scripts/patch-appimage-wayland.sh