# Lithographer

GUI for [litho](https://github.com/girish946/litho) — flash disk images and clone block devices with a native desktop app built on [Tauri 2](https://tauri.app/).

<p align="center">
<img src="src/assets/logo.png" alt="Lithographer logo">
</p>

Lithographer stays **unprivileged**. Device enumeration and validation run in-process via `liblitho`; privileged flash/clone work is delegated to a bundled **`litho` sidecar** that speaks a line-oriented GUI protocol on stdout.

## Features

- **Flash** and **clone** with real block I/O (sidecar built with `real-io`)
- **Device picker** — removable devices listed first; fixed-disk extra confirmation in litho
- **Native file dialog** — open image for flash, save path for clone (`rfd`)
- **Optimal I/O block size** — computed from target device capacity (not a fixed 4 KiB)
- **Optional SHA-256 verify** — checkbox for flash operations (off by default)
- **Cooperative cancel** — cancel flag file works across `pkexec` / UAC elevation boundaries
- **Privilege diagnostics** — live badge (root / polkit / UAC / unprivileged) from startup checks
- **Light / dark themes** with persisted preference
- **Linux:** AppImage, `.deb`, `.rpm` bundles; Wayland compatibility hook for Hyprland and similar compositors
- **Windows:** MSI/NSIS installers; UAC handoff relaunches the app elevated so litho stdout pipes correctly

## Architecture

```
Lithographer (user session)
  ├─ liblitho in-process  → device list, validation, optimal block size
  └─ litho sidecar        → pkexec / direct (Linux) or piped child (Windows)
        └─ litho -o gui flash|clone … --cancel-file <path> [--verify]
```

**Linux elevation:** `pkexec litho …` by default. On AppImage, `litho` is copied to `$TMPDIR/lithographer-litho-<pid>` first because `pkexec` cannot execute binaries inside the FUSE mount. The staged copy is removed when the app exits.

**Windows elevation:** Lithographer relaunches itself via UAC (`--auto-run …`) so `litho.exe` runs as a hidden piped child inside the elevated GUI process (elevating litho directly would break stdout capture).

**Cancel across elevation:** stdin and parent signals do not reliably reach a `pkexec` child. Lithographer creates `~/.cache/litho/cancel-<pid>-<ts>.flag`, passes `--cancel-file` to litho, and writes `cancel` into the file on user cancel. Litho polls the file every ~50 ms.

## Requirements

### Linux

- [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) (`webkit2gtk-4.1`, etc.)
- `pkexec` (polkit) for unprivileged flash/clone
- On **Fedora GNOME**: polkit auth is built into `gnome-shell` (the retired `polkit-gnome-authentication-agent-1` package is not required)

### Windows

- [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) (WebView2, Visual Studio Build Tools)
- OpenSSL (CI uses the runner pre-install at `C:\Program Files\OpenSSL`)
- Administrator approval via UAC for flash/clone

## Repository layout

Clone **lithographer** and its sibling dependency **litho** (required for `liblitho` and the bundled sidecar):

```bash
git clone https://github.com/girish946/lithographer.git
git clone --branch windows-implementation https://github.com/girish946/litho.git
```

The repos must sit side by side (`…/litho` next to `…/lithographer`). GitHub Actions checks out `litho` from the `windows-implementation` branch the same way.

## Build

```bash
cd lithographer
npm install
npm run tauri:build
```

### npm scripts

| Script | Description |
|--------|-------------|
| `npm run tauri:dev` | Prepare sidecar + `tauri dev` |
| `npm run tauri:build` | Prepare sidecar + `tauri build` + post-build AppImage Wayland patch |
| `npm run prepare-sidecar` | Build `litho` with `real-io` into `src-tauri/binaries/` |
| `npm run vendor-assets` | Bundle frontend fonts/CSS locally (offline-friendly) |
| `npm run generate-icons` | Regenerate app icons from source artwork |

The sidecar build (`src-tauri/scripts/prepare-litho-sidecar.sh`) runs:

```bash
cargo build --release --no-default-features --features real-io --bin litho
```

### Build outputs

| Platform | Artifacts |
|----------|-----------|
| Linux | `src-tauri/target/release/bundle/appimage/lithographer_*_amd64.AppImage` |
| Linux | `src-tauri/target/release/bundle/deb/*.deb`, `rpm/*.rpm` |
| Windows | `src-tauri/target/release/bundle/msi/*.msi`, `nsis/*.exe` |

### Docker / older glibc (Linux)

From the project root (`litho-proj/`), `make build` uses a Docker image targeting Ubuntu 22.04 for broader glibc compatibility. See the root `Makefile` and `Dockerfile`.

## Usage

### Linux

```bash
./src-tauri/target/release/bundle/appimage/lithographer_0.1.0_amd64.AppImage
```

The AppImage includes a post-build Wayland compatibility hook (see [`docs/tauri-2-appimage-wayland-fix.md`](docs/tauri-2-appimage-wayland-fix.md)). On some setups the `.deb` package is more reliable because it uses system WebKitGTK.

### Windows

Run the MSI/NSIS installer output, or during development:

```bash
npm run tauri:dev
```

### Privilege elevation options (Linux)

| Method | Auth UI | When to use |
|--------|---------|-------------|
| **`pkexec` (default)** | GNOME Shell / polkit dialog (password, fingerprint) | Normal Fedora/Ubuntu GNOME sessions |
| **`LITHOGRAPHER_ELEVATION=sudo`** | Graphical `sudo` askpass (`SUDO_ASKPASS`, `ksshaskpass`, `zenity`, …) | Polkit misbehaving |
| **`LITHOGRAPHER_ELEVATION=run0`** | Polkit via systemd | Fedora 41+ with `run0` on PATH |
| **Run app elevated** | One prompt at startup | `sudo ./lithographer*.AppImage` — litho spawns directly, no per-operation `pkexec` |

```bash
# Polkit path (default)
./lithographer_0.1.0_amd64.AppImage

# sudo askpass fallback
LITHOGRAPHER_ELEVATION=sudo ./lithographer_0.1.0_amd64.AppImage

# Whole session elevated
sudo ./lithographer_0.1.0_amd64.AppImage
```

### Privilege elevation (Windows)

When you start flash/clone without Administrator rights, Lithographer prompts for UAC and relaunches with `--auto-run` so progress streams in the elevated window. The original unprivileged window exits after handoff.

## Development

```bash
# One-time: build litho sidecar
npm run prepare-sidecar

# Dev loop (hot reload)
npm run tauri:dev
```

After changing litho CLI protocol or cancel behaviour, rebuild the sidecar before testing Lithographer:

```bash
npm run prepare-sidecar
```

### Startup diagnostics

The privilege badge and `get_startup_diagnostics` Tauri command report:

- elevated vs unprivileged state
- elevation backend (`pkexec`, `sudo -A`, `run0`, `uac`)
- polkit agent or `gnome-shell (built-in polkit)` on Fedora GNOME
- preview of the litho spawn command

## CI

GitHub Actions (`.github/workflows/build.yaml`):

- **ubuntu-22.04** — AppImage, `.deb`, `.rpm`
- **windows-latest** — MSI and NSIS (OpenSSL + vcpkg `liblzma` for the litho sidecar)

Both jobs clone `litho` from the `windows-implementation` branch.

## Screenshots

<img src="src/assets/lithographer-window.png" alt="Lithographer main window">

## Safety

- Always double-check the target device. Flashing the wrong disk destroys data.
- Prefer removable USB drives for flash targets.
- Device validation (system disk, mounts, partition paths) runs in litho before any write.

## Related

- [litho](https://github.com/girish946/litho) — CLI, library, and `litho-tui`
- [Tauri 2 AppImage Wayland fix notes](docs/tauri-2-appimage-wayland-fix.md)

## License

See `src-tauri/Cargo.toml`.