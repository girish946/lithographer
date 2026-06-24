use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Resolve the litho CLI binary for dev builds and bundled sidecar installs.
pub fn resolve_litho_binary(app: &AppHandle) -> Result<PathBuf, String> {
    let path = resolve_litho_binary_raw(app)?;
    prepare_for_pkexec(&path)
}

fn resolve_litho_binary_raw(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(dev) = dev_litho_binary() {
        return Ok(dev);
    }

    let mut tried = Vec::new();
    for candidate in bundled_sidecar_candidates(app) {
        tried.push(candidate.display().to_string());
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(format!(
        "litho sidecar not found. Searched:\n  - {}\nRebuild with `npm run prepare-sidecar && npm run tauri:build`.",
        tried.join("\n  - ")
    ))
}

/// Tauri `externalBin: ["binaries/litho"]` installs the binary as `usr/bin/litho`
/// in AppImages (not under `usr/lib/lithographer/binaries/…`). Check every layout.
fn bundled_sidecar_candidates(app: &AppHandle) -> Vec<PathBuf> {
    let triple = env!("LITHO_TARGET_TRIPLE");
    let mut candidates = Vec::new();

    if let Ok(appdir) = std::env::var("APPDIR") {
        candidates.push(PathBuf::from(&appdir).join("usr/bin/litho"));
        candidates.push(PathBuf::from(&appdir).join(format!("usr/bin/litho-{triple}")));
        candidates.push(
            PathBuf::from(&appdir).join(format!("usr/lib/lithographer/binaries/litho-{triple}")),
        );
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("litho"));
        candidates.push(resource_dir.join(format!("litho-{triple}")));
        candidates.push(resource_dir.join(format!("binaries/litho-{triple}")));
    }

    for rel in [
        format!("binaries/litho-{triple}"),
        "litho".to_string(),
        format!("litho-{triple}"),
    ] {
        if let Ok(path) = app
            .path()
            .resolve(&rel, tauri::path::BaseDirectory::Resource)
        {
            candidates.push(path);
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("litho"));
            candidates.push(dir.join(format!("litho-{triple}")));
            candidates.push(dir.join(format!("binaries/litho-{triple}")));
            // AppImage mount: exe is usr/bin/lithographer
            if let Some(usr) = dir.parent() {
                candidates.push(usr.join("bin/litho"));
            }
        }
    }

    // Source-tree layout for local `cargo tauri build` without AppImage.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest_dir.join(format!("binaries/litho-{triple}")));

    dedupe_paths(candidates)
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if !out.iter().any(|p| p == &path) {
            out.push(path);
        }
    }
    out
}

/// pkexec runs as root and cannot execute binaries inside an AppImage FUSE mount.
fn prepare_for_pkexec(path: &Path) -> Result<PathBuf, String> {
    let display = path.display().to_string();
    let in_appimage = std::env::var("APPIMAGE").is_ok() || display.contains("/.mount_");
    if !in_appimage {
        return Ok(path.to_path_buf());
    }

    let dest = std::env::temp_dir().join(format!(
        "lithographer-litho-{}",
        std::process::id()
    ));
    fs::copy(path, &dest).map_err(|e| format!("Failed to stage litho sidecar for pkexec: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(&dest) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&dest, perms);
        }
    }
    Ok(dest)
}

fn dev_litho_binary() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // Prefer release (typically built with real-io for sidecar) over debug (often simulated-io).
    for candidate in [
        manifest_dir.join("../../litho/target/release/litho"),
        manifest_dir.join("../../litho/target/debug/litho"),
        manifest_dir.join(format!(
            "binaries/litho-{}",
            env!("LITHO_TARGET_TRIPLE")
        )),
    ] {
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedupe_paths_removes_duplicates() {
        let paths = dedupe_paths(vec![PathBuf::from("/a"), PathBuf::from("/a"), PathBuf::from("/b")]);
        assert_eq!(paths.len(), 2);
    }
}