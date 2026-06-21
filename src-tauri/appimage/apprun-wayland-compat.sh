#! /usr/bin/env bash

# AppImage launcher workaround for Wayland compositors (e.g. Hyprland on Arch).
# Bundled WebKitGTK can fail to create an EGL display unless the host
# libwayland-client is preloaded and desktop integration is disabled.

export DESKTOPINTEGRATION=1

if [ -z "${LD_PRELOAD:-}" ]; then
  for lib in \
    /usr/lib/libwayland-client.so \
    /usr/lib64/libwayland-client.so \
    /usr/lib/x86_64-linux-gnu/libwayland-client.so \
    /usr/lib/aarch64-linux-gnu/libwayland-client.so \
    /usr/lib/arm-linux-gnueabihf/libwayland-client.so; do
    if [ -f "$lib" ]; then
      export LD_PRELOAD="$lib"
      break
    fi
  done
fi