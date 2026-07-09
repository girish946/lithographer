# Lithographer — developer documentation

User-facing overview: [README.md](../README.md).

This document covers architecture, build scripts, elevation internals, cancel protocol, CI, and packaging.

---

## Features (detailed)

| Feature | Notes |
|---------|-------|
| Flash / clone | Real block I/O via `litho` sidecar (`real-io`) |
| Device picker | `liblitho` in-process enumeration; removable first |
| Native file dialogs | `rfd` open (flash) / save (clone) |
| Optimal block size | From device capacity via `liblitho` |
| Optional SHA-256 verify | Passed as `--verify` to litho |
| Cooperative cancel | `--cancel-file` across pkexec / UAC |
| Privilege diagnostics | Badge + `get_startup_diagnostics` |
| Themes | Light / dark, persisted |
| Linux packages | AppImage, deb, rpm; Wayland AppImage patch |
| Windows packages | MSI, NSIS; elevated self-relaunch for pipe capture |

---

## Architecture

```
Lithographer (user session)
  ├─ liblitho in-process  → device list, validation, optimal block size
  └─ litho sidecar        → elevated or direct
        └─ litho -o gui flash|clone … --cancel-file <path> [--verify] [--yes]
```

| Concern | Where |
|---------|--------|
| UI | `src/` (HTML/JS/CSS) |
| Tauri commands / sidecar spawn | `src-tauri/src/` |
| Device enum / validation | path dependency on `../litho` (`liblitho`) |
| Destructive I/O | external `litho` binary with `-o gui` |

### Linux elevation

- Default: `pkexec litho …`
- AppImage: `litho` is copied to `$TMPDIR/lithographer-litho-<pid>` first (`pkexec` cannot execute from FUSE mounts); staged copy removed on exit
- Env override: `LITHOGRAPHER_ELEVATION=sudo` (askpass), `run0` where available
- Whole-app `sudo` runs litho without per-op `pkexec`

### Windows elevation

- Lithographer relaunches **itself** via UAC with `--auto-run …`
- Elevated GUI process spawns `litho.exe` as a **hidden piped child** (elevating litho alone would break stdout capture)

### Cancel across elevation

- stdin/signals do not reliably reach `pkexec` children
- Create `~/.cache/litho/cancel-<pid>-<ts>.flag` (Windows: under `%LOCALAPPDATA%\litho\`)
- Pass `--cancel-file` to litho; write `cancel` into the file on user cancel
- Litho polls ~50 ms

GUI protocol details: [litho developer docs](https://github.com/girish946/litho/blob/main/docs/developer-docs.md#gui-protocol--o-gui).

---

## Repository layout

```
…/litho/                 # sibling crate + CLI
…/lithographer/
  package.json
  src/                   # frontend
  src-tauri/
    src/                 # Rust host
    binaries/            # prepared litho sidecar
    scripts/             # prepare-litho-sidecar, AppImage patch
    tauri*.conf.json
  docs/
```

Clone both repos as siblings:

```bash
git clone https://github.com/girish946/lithographer.git
git clone https://github.com/girish946/litho.git
# Adjust branch if CI uses a non-default litho branch
```

---

## Build

### Prerequisites

**Linux:** [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) (`webkit2gtk-4.1`, etc.), `pkexec`

**Windows:** Tauri prerequisites (WebView2, VS Build Tools), OpenSSL for release litho builds as required by CI

### Commands

```bash
cd lithographer
npm install
npm run tauri:build
```

| Script | Description |
|--------|-------------|
| `npm run tauri:dev` | Prepare sidecar + `tauri dev` |
| `npm run tauri:build` | Prepare sidecar + `tauri build` + AppImage Wayland post-patch |
| `npm run prepare-sidecar` | Build `litho` with `real-io` into `src-tauri/binaries/` |
| `npm run vendor-assets` | Vendor frontend fonts/CSS |
| `npm run generate-icons` | Regenerate icons |

Sidecar build (conceptually):

```bash
cargo build --release --no-default-features --features real-io --bin litho
# then copy/rename into src-tauri/binaries/
```

See `src-tauri/scripts/prepare-litho-sidecar.sh` / `.ps1`.

### Outputs

| Platform | Artifacts |
|----------|-----------|
| Linux | `src-tauri/target/release/bundle/appimage/*_amd64.AppImage` |
| Linux | `…/deb/*.deb`, `…/rpm/*.rpm` |
| Windows | `…/msi/*.msi`, `…/nsis/*.exe` |

### Development loop

```bash
npm run prepare-sidecar   # after litho CLI/protocol changes
npm run tauri:dev
```

### Startup diagnostics

`get_startup_diagnostics` / privilege badge report:

- elevated vs unprivileged
- elevation backend (`pkexec`, `sudo -A`, `run0`, `uac`)
- polkit agent presence (or GNOME built-in polkit)
- preview of the litho spawn command

---

## CI

`.github/workflows/build.yaml` (names may vary):

- **ubuntu** — AppImage, deb, rpm  
- **windows-latest** — MSI, NSIS (OpenSSL + vcpkg `liblzma` for litho)

Jobs clone the **litho** sibling repository (branch configured in the workflow).

---

## Packaging notes

### AppImage Wayland

Post-build hook: `src-tauri/scripts/patch-appimage-wayland.sh` injects `src-tauri/appimage/apprun-wayland-compat.sh` for compositors such as Hyprland.

### Frontend assets

`npm run vendor-assets` embeds fonts/CSS for offline packaging.

---

## Safety (developer)

- Never skip device validation paths when changing spawn args.
- Keep `--cancel-file` when elevating; do not rely on SIGINT alone.
- Rebuild the sidecar after any litho CLI protocol change before testing the GUI.

---

## Related

- [litho](https://github.com/girish946/litho) — CLI, library, TUI  
- [litho developer docs](https://github.com/girish946/litho/blob/main/docs/developer-docs.md)

## License

See `src-tauri/Cargo.toml`.
