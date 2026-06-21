#!/usr/bin/env bash
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
hook_src="${root_dir}/appimage/apprun-wayland-compat.sh"
bundle_root="${root_dir}/target"

if [ ! -f "$hook_src" ]; then
  echo "Wayland compat hook not found at ${hook_src}" >&2
  exit 1
fi

find_linuxdeploy() {
  local arch="$1"
  local candidates=(
    "${XDG_CACHE_HOME:-$HOME/.cache}/tauri/linuxdeploy-${arch}.AppImage"
    "${root_dir}/target/release/bundle/appimage/linuxdeploy-${arch}.AppImage"
    "${root_dir}/target/debug/bundle/appimage/linuxdeploy-${arch}.AppImage"
  )

  for candidate in "${candidates[@]}"; do
    if [ -f "$candidate" ]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

patch_appdir() {
  local appdir="$1"
  local hook_dst="${appdir}/apprun-hooks/wayland-compat.sh"
  local apprun="${appdir}/AppRun"

  if [ ! -d "$appdir" ] || [ ! -f "$apprun" ]; then
    return 0
  fi

  mkdir -p "${appdir}/apprun-hooks"
  cp "$hook_src" "$hook_dst"
  chmod +x "$hook_dst"

  if ! grep -q 'apprun-hooks/wayland-compat.sh' "$apprun"; then
    sed -i '/^exec "\$this_dir"\/AppRun.wrapped/i source "$this_dir"/apprun-hooks/wayland-compat.sh' "$apprun"
  fi

  printf 'Patched %s\n' "$appdir"
}

repack_appimage() {
  local appdir="$1"
  local arch="$2"
  local appimage="$3"
  local linuxdeploy

  if ! linuxdeploy="$(find_linuxdeploy "$arch")"; then
    echo "linuxdeploy not found for ${arch}; skipping AppImage repack for ${appdir}" >&2
    return 0
  fi

  export OUTPUT="$appimage"
  export ARCH="$arch"
  export APPIMAGE_EXTRACT_AND_RUN=1

  "$linuxdeploy" \
    --appimage-extract-and-run \
    --verbosity 1 \
    --appdir "$appdir" \
    --output appimage

  printf 'Repacked %s\n' "$appimage"
}

map_arch() {
  case "$1" in
    amd64) printf '%s\n' x86_64 ;;
    aarch64) printf '%s\n' aarch64 ;;
    armhf) printf '%s\n' armhf ;;
    i386) printf '%s\n' i686 ;;
    *) return 1 ;;
  esac
}

patched=0

while IFS= read -r -d '' appdir; do
  patch_appdir "$appdir"
  patched=1

  appimage_dir="$(dirname "$appdir")"
  product_name="$(basename "$appdir" .AppDir)"

  for appimage in "${appimage_dir}/${product_name}"_*.AppImage; do
    [ -f "$appimage" ] || continue

    arch_tag="${appimage##*_}"
    arch_tag="${arch_tag%.AppImage}"
    linuxdeploy_arch="$(map_arch "$arch_tag")" || continue

    repack_appimage "$appdir" "$linuxdeploy_arch" "$appimage"
  done
done < <(find "$bundle_root" -path '*/bundle/appimage/*.AppDir' -type d -print0 2>/dev/null)

if [ "$patched" -eq 0 ]; then
  echo "No AppImage AppDir found under ${bundle_root}; skipping Wayland patch" >&2
fi