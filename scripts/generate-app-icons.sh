#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source="${repo_root}/src/assets/dark-logo.jpg"
out_dir="${repo_root}/src-tauri/icons"

if [[ ! -f "${source}" ]]; then
  echo "error: icon source not found: ${source}" >&2
  exit 1
fi

echo "Generating Tauri icons from ${source} ..."
(
  cd "${repo_root}"
  npm exec -- tauri icon "${source}" -o src-tauri/icons
)

echo "Icons written to ${out_dir}"