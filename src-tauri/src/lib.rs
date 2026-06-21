// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

use std::fs;
use std::process::Command;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tauri::Manager;

use liblitho::devices::{self as litho_devices, DeviceInfo as LithoDeviceInfo};

#[derive(serde::Serialize)]
struct StartupDiagnostics {
    desktop_environment: String,
    current_user: String,
    is_root: bool,
    polkit_agent_path: Option<String>,
    polkit_agent_is_executable: bool,
    // Polkit-related privileged execution test.
    // We now run the litho binary's --help through pkexec so this single step
    // verifies both the polkit auth agent and that the bundled litho binary works.
    litho_polkit_result: String,
    // Direct (non-privileged) litho binary checks
    litho_binary_path: Option<String>,
    litho_can_execute: bool,
    litho_help_result: String,
}

#[derive(serde::Serialize, Clone, Debug)]
struct LaunchParams {
    mode: Option<String>,
    device: Option<String>,
    image: Option<String>,
}

fn parse_launch_args() -> LaunchParams {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut params = LaunchParams {
        mode: None,
        device: None,
        image: None,
    };

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--mode" | "-m" => {
                if i + 1 < args.len() {
                    params.mode = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--device" | "-d" => {
                if i + 1 < args.len() {
                    params.device = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            "--image" | "-i" | "--file" => {
                if i + 1 < args.len() {
                    params.image = Some(args[i + 1].clone());
                    i += 1;
                }
            }
            _ if !arg.starts_with('-') => {
                // positional args: first = image, second = device
                if params.image.is_none() {
                    params.image = Some(arg.clone());
                } else if params.device.is_none() {
                    params.device = Some(arg.clone());
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Normalize mode value
    if let Some(ref m) = params.mode {
        let lower = m.to_lowercase();
        if lower == "clone" || lower == "backup" {
            params.mode = Some("clone".to_string());
        } else {
            params.mode = Some("flash".to_string());
        }
    }

    params
}

#[tauri::command]
fn get_launch_params() -> LaunchParams {
    parse_launch_args()
}

fn get_desktop_environment() -> String {
    std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("DESKTOP_SESSION"))
        .or_else(|_| std::env::var("GDMSESSION"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn get_current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn is_running_as_root() -> bool {
    if let Ok(output) = Command::new("id").arg("-u").output() {
        let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        uid == "0"
    } else {
        false
    }
}

fn extract_executable_path_from_ps(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        if token.starts_with('/') && token.contains("polkit") {
            return Some(token.to_string());
        }
    }
    None
}

fn find_polkit_auth_agent() -> Option<String> {
    let user = get_current_user();
    let de = get_desktop_environment().to_lowercase();

    // 1. Try to detect a running polkit authentication agent for the current user
    if let Ok(output) = Command::new("ps")
        .args(["-u", &user, "-o", "pid,comm,args", "--no-headers"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let lower = line.to_lowercase();
            if lower.contains("polkit") && (lower.contains("agent") || lower.contains("-authentication-agent")) {
                if let Some(path) = extract_executable_path_from_ps(line) {
                    if fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
                        return Some(path);
                    }
                }
            }
        }
    }

    // 2. Fallback: known agent locations, prioritized by detected desktop environment
    let candidates: Vec<&str> = if de.contains("gnome") {
        vec![
            "/usr/libexec/polkit-gnome-authentication-agent-1",
            "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
            "/usr/lib/gnome-polkit/polkit-gnome-authentication-agent-1",
        ]
    } else if de.contains("kde") || de.contains("plasma") {
        vec![
            "/usr/libexec/polkit-kde-authentication-agent-1",
            "/usr/lib/polkit-kde-authentication-agent-1",
            "/usr/lib/x86_64-linux-gnu/libexec/polkit-kde-authentication-agent-1",
        ]
    } else if de.contains("xfce") {
        vec![
            "/usr/libexec/xfce-polkit",
            "/usr/lib/xfce4/polkit/xfce-polkit",
            "/usr/bin/xfce-polkit",
        ]
    } else if de.contains("mate") {
        vec![
            "/usr/libexec/polkit-mate-authentication-agent-1",
            "/usr/lib/mate-polkit/polkit-mate-authentication-agent-1",
        ]
    } else if de.contains("lx") || de.contains("lxd") || de.contains("lubuntu") {
        vec![
            "/usr/bin/lxpolkit",
            "/usr/libexec/lxpolkit",
            "/usr/lib/lxpolkit/lxpolkit",
        ]
    } else if de.contains("cinnamon") {
        vec![
            "/usr/libexec/cinnamon-polkit",
            "/usr/bin/cinnamon-polkit",
        ]
    } else {
        // Broad fallback list for unknown DEs
        vec![
            "/usr/libexec/polkit-gnome-authentication-agent-1",
            "/usr/libexec/polkit-kde-authentication-agent-1",
            "/usr/libexec/polkit-mate-authentication-agent-1",
            "/usr/bin/lxpolkit",
            "/usr/libexec/xfce-polkit",
        ]
    };

    for path in candidates {
        if let Ok(meta) = fs::metadata(path) {
            if meta.is_file() {
                return Some(path.to_string());
            }
        }
    }

    None
}

fn is_file_executable(path: &str) -> bool {
    match fs::metadata(path) {
        Ok(meta) => {
            if !meta.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                let mode = meta.permissions().mode();
                (mode & 0o111) != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        }
        Err(_) => false,
    }
}

/// Try to execute the bundled litho binary with --help.
/// Returns (can_execute, result_message).
fn test_litho_binary_help(litho_path: &std::path::Path) -> (bool, String) {
    if !litho_path.exists() {
        return (
            false,
            format!("Binary does not exist at {}", litho_path.display()),
        );
    }

    // Try a simple --help invocation. This should work without any privileges.
    match Command::new(litho_path).arg("--help").output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let exit_code = output.status.code().unwrap_or(-1);
            let success = output.status.success();

            let preview = if !stdout.is_empty() {
                stdout.lines().next().unwrap_or("").to_string()
            } else if !stderr.is_empty() {
                stderr.lines().next().unwrap_or("").to_string()
            } else {
                "(no output)".to_string()
            };

            if success {
                (true, format!("SUCCESS (exit {}) — {}", exit_code, preview))
            } else {
                (
                    false,
                    format!(
                        "FAILED (exit {}) — stdout: {} | stderr: {}",
                        exit_code, preview, stderr
                    ),
                )
            }
        }
        Err(e) => (
            false,
            format!("Failed to spawn litho binary: {}", e),
        ),
    }
}

/// Test running the litho binary via pkexec (i.e. through the polkit auth agent).
/// We use `litho --help` instead of a generic `ls` so that this single step
/// verifies both:
///   - the polkit authentication agent is present and functional
///   - the bundled litho binary (our main asset) can be executed
#[allow(dead_code)]
fn test_litho_via_polkit(litho_path: Option<&std::path::Path>) -> String {
    let litho = match litho_path {
        Some(p) if p.exists() => p.to_string_lossy().to_string(),
        _ => return "No litho binary available to test via polkit".to_string(),
    };

    // Use `timeout` when available to prevent blocking on an auth prompt.
    let has_timeout = Command::new("timeout").arg("--version").output().is_ok();

    let mut cmd = if has_timeout {
        let mut c = Command::new("timeout");
        c.args(["2s", "pkexec", &litho, "--help"]);
        c
    } else {
        let mut c = Command::new("pkexec");
        c.args([&litho, "--help"]);
        c
    };

    match cmd.output() {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let code = output.status.code().unwrap_or(-1);

            if code == 0 {
                let first_line = stdout.lines().next().unwrap_or("");
                format!("SUCCESS (exit 0) — ran litho --help via pkexec. Preview: {}", first_line)
            } else if code == 124 || code == 137 {
                // timeout killed it — this usually means the polkit agent presented an authentication dialog
                "TIMED OUT after 2s (polkit agent likely presented an authentication dialog for litho)".to_string()
            } else if stderr.to_lowercase().contains("no authentication agent")
                || stderr.to_lowercase().contains("not authorized")
                || stderr.to_lowercase().contains("could not find")
                || stderr.to_lowercase().contains("authorization failed")
            {
                format!("POLKIT AGENT PROBLEM (exit {}) — {}", code, stderr)
            } else {
                format!("FAILED (exit {}) — stdout: {} | stderr: {}", code, stdout, stderr)
            }
        }
        Err(e) => {
            format!("FAILED TO SPAWN pkexec: {}", e)
        }
    }
}

fn perform_startup_checks() -> StartupDiagnostics {
    let desktop_environment = get_desktop_environment();
    let current_user = get_current_user();
    let is_root = is_running_as_root();
    let polkit_agent_path = find_polkit_auth_agent();
    let polkit_agent_is_executable = polkit_agent_path
        .as_ref()
        .map(|p| is_file_executable(p))
        .unwrap_or(false);
    // Polkit elevation test for the litho binary (or app re-launch) is now deferred.
    // We only record whether an agent appears to be available.
    // The actual privileged re-launch will be triggered later via the relaunch_elevated API.
    let litho_polkit_result = if polkit_agent_path.as_ref().map_or(false, |p| is_file_executable(p)) {
        "Polkit agent present. Elevation test and re-launch deferred until operation is requested.".to_string()
    } else {
        "No usable polkit authentication agent detected at startup.".to_string()
    };

    StartupDiagnostics {
        desktop_environment,
        current_user,
        is_root,
        polkit_agent_path,
        polkit_agent_is_executable,
        litho_polkit_result,
        litho_binary_path: None,
        litho_can_execute: false,
        litho_help_result: "Not checked (litho path resolution requires app context)".to_string(),
    }
}

#[tauri::command]
fn get_startup_diagnostics(app: tauri::AppHandle) -> StartupDiagnostics {
    let mut diag = perform_startup_checks();

    match find_litho_binary(&app) {
        Ok(litho_path) => {
            diag.litho_binary_path = Some(litho_path.to_string_lossy().to_string());

            // Direct (non-privileged) check that the binary itself can run.
            // We deliberately do *not* invoke pkexec / polkit at launch anymore
            // to avoid showing an authentication dialog immediately.
            let (can_execute, help_result) = test_litho_binary_help(&litho_path);
            diag.litho_can_execute = can_execute;
            diag.litho_help_result = help_result;

            // The polkit result was already set in perform_startup_checks to indicate
            // that full privileged testing / re-launch is deferred.
            // We only keep the agent detection info here.
        }
        Err(e) => {
            diag.litho_binary_path = None;
            diag.litho_can_execute = false;
            diag.litho_help_result = format!("Failed to locate bundled litho binary: {}", e);
            diag.litho_polkit_result = format!("Failed to locate bundled litho binary: {}", e);
        }
    }

    diag
}

/// Returns the absolute path to the bundled `litho` binary (shipped as a resource).
/// This path is inside the AppImage / bundle at runtime.
#[tauri::command]
fn get_litho_binary_path(app: tauri::AppHandle) -> Result<String, String> {
    // For most callers we return the bundled location.
    // Callers that will pass the path to pkexec should use get_usable_litho_path_for_privileged instead.
    find_litho_binary(&app)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// Returns a path to `litho` that is safe to pass to pkexec / privileged contexts.
/// When inside an AppImage the original binary is on a FUSE mount that root
/// usually cannot read, so we copy it to the app's cache directory.
#[tauri::command]
fn get_usable_litho_path_for_privileged(app: tauri::AppHandle) -> Result<String, String> {
    let bundled = find_litho_binary(&app)?;
    get_usable_litho_path(&app, &bundled)
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

/// Returns a path to the current application executable that is safe to pass to pkexec.
/// 
/// When running as an AppImage, we prefer the $APPIMAGE environment variable
/// (the real .AppImage file on disk) because the FUSE mount is usually not
/// accessible to the root user invoked by pkexec.
fn get_app_executable_for_privileged() -> Result<std::path::PathBuf, String> {
    // Best case for AppImages: $APPIMAGE points to the real file on a normal filesystem.
    if let Ok(appimage) = std::env::var("APPIMAGE") {
        let p = std::path::PathBuf::from(appimage);
        if p.exists() {
            // Ensure it is executable (defensive).
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&p) {
                    let mut perms = meta.permissions();
                    if perms.mode() & 0o111 == 0 {
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&p, perms);
                    }
                }
            }
            return Ok(p);
        }
    }

    // Fallback: current_exe(). For normal installs / development this is usually fine.
    // If we are inside a mount point without APPIMAGE we may still have problems,
    // but callers can handle the error from pkexec.
    std::env::current_exe()
        .map_err(|e| format!("Failed to determine current executable path: {}", e))
}

/// Request that the application be re-launched with the given arguments
/// under privilege elevation (via pkexec).
///
/// This is the on-demand API. It is not called automatically at startup.
/// The caller (usually the frontend) should pass the current mode/device/image
/// the user wants to operate on. The elevated instance will receive them
/// via the normal launch argument parsing and can pre-fill the UI (or
/// even start the operation directly if desired).
///
/// On success this function will cause the current (unprivileged) instance
/// to exit after spawning the elevated copy.
#[tauri::command]
fn relaunch_elevated(
    app: tauri::AppHandle,
    mode: Option<String>,
    device: Option<String>,
    image: Option<String>,
) -> Result<(), String> {
    let exe = get_app_executable_for_privileged()?;

    let mut cmd_args: Vec<String> = Vec::new();

    if let Some(m) = mode {
        let lower = m.to_lowercase();
        let normalized = if lower == "clone" || lower == "backup" {
            "clone".to_string()
        } else {
            "flash".to_string()
        };
        cmd_args.push("--mode".to_string());
        cmd_args.push(normalized);
    }

    if let Some(d) = device {
        cmd_args.push("--device".to_string());
        cmd_args.push(d);
    }

    if let Some(i) = image {
        cmd_args.push("--image".to_string());
        cmd_args.push(i);
    }

    // If we are already running as root we can just restart the current process
    // with the desired arguments (no pkexec needed).
    if is_running_as_root() {
        let mut cmd = std::process::Command::new(&exe);
        cmd.args(&cmd_args);
        // Spawn a new instance and exit this one.
        let _ = cmd.spawn();
        app.exit(0);
        return Ok(());
    }

    // Normal case: ask pkexec to run our executable with the args.
    let mut cmd = std::process::Command::new("pkexec");
    cmd.arg(&exe);
    cmd.args(&cmd_args);

    match cmd.spawn() {
        Ok(_) => {
            // Successfully asked for elevation. The polkit agent (if any)
            // will present a dialog. Exit the current unprivileged instance.
            app.exit(0);
            Ok(())
        }
        Err(e) => Err(format!(
            "Failed to spawn pkexec for re-launch: {}. Is a polkit agent running?",
            e
        )),
    }
}

/// Returns a path to the current application binary that can safely be passed to pkexec.
/// Exposed so the frontend (or other code) can inspect what would be re-launched.
#[tauri::command]
fn get_usable_app_path_for_privileged() -> Result<String, String> {
    get_app_executable_for_privileged()
        .map(|p| p.to_string_lossy().to_string())
}

fn get_usable_litho_path(app: &tauri::AppHandle, bundled: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let path_str = bundled.to_string_lossy();
    let is_appimage = path_str.contains("/.mount_") || std::env::var("APPIMAGE").is_ok();

    if !is_appimage {
        return Ok(bundled.to_path_buf());
    }

    // Stage a copy to the app cache dir (normal filesystem, visible to pkexec)
    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| format!("Failed to resolve cache dir: {}", e))?;
    std::fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

    let dest = cache_dir.join("litho");

    let needs_update = !dest.exists() || {
        let src_len = std::fs::metadata(bundled).map(|m| m.len()).unwrap_or(0);
        let dst_len = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
        src_len != dst_len
    };

    if needs_update {
        std::fs::copy(bundled, &dest)
            .map_err(|e| format!("Failed to stage litho for privileged use: {}", e))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest)
                .map_err(|e| e.to_string())?
                .permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms).map_err(|e| e.to_string())?;
        }
    }

    Ok(dest)
}

/// Robust finder for the litho binary.
/// Tries the official Resource resolution first, then several fallbacks
/// that are useful inside AppImages, .deb installs, and during development.
fn find_litho_binary(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    // 1. Official Tauri resource resolution (works in dev + most bundles).
    // The bundler (when "resources": ["resources/litho"]) typically places it under
    // a "resources/" subdir inside the platform resource directory.
    for candidate in ["resources/litho", "litho"] {
        if let Ok(path) = app.path().resolve(candidate, tauri::path::BaseDirectory::Resource) {
            if path.exists() {
                return Ok(path);
            }
        }
    }

    // 2. Next to the current executable (common for AppImages / installed packages)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Check both flat and the resources/ subdir (matches what the bundler produces)
            for name in ["litho", "resources/litho"] {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Ok(candidate);
                }
            }

            // 3. Typical AppImage / deb layout: exe in usr/bin, resources under usr/lib/<product> (with or without resources/ subdir)
            for name in ["litho", "resources/litho"] {
                let candidate = dir.join("../lib/lithographer").join(name);
                if candidate.exists() {
                    return Ok(candidate.canonicalize().unwrap_or(candidate));
                }
                let candidate2 = dir.join("../../lib/lithographer").join(name);
                if candidate2.exists() {
                    return Ok(candidate2.canonicalize().unwrap_or(candidate2));
                }
            }
        }
    }

    // 5. Common installed locations (for .deb / system packages)
    for prefix in ["/usr/lib/lithographer", "/usr/local/lib/lithographer", "/opt/lithographer"] {
        for name in ["litho", "resources/litho"] {
            let candidate = std::path::Path::new(prefix).join(name);
            if candidate.exists() {
                return Ok(candidate.to_path_buf());
            }
        }
    }

    // 6. Last resort: look for it relative to the AppImage mount point
    if let Ok(exe) = std::env::current_exe() {
        let exe_str = exe.to_string_lossy();
        if exe_str.contains("/.mount_") {
            // Find the mount root by walking parents
            for ancestor in exe.ancestors() {
                let a = ancestor.to_string_lossy();
                if a.contains("/.mount_") {
                    // Check both flat and resources/ subdir under the typical lib location
                    for name in ["litho", "resources/litho"] {
                        let candidate = ancestor.join("usr/lib/lithographer").join(name);
                        if candidate.exists() {
                            return Ok(candidate);
                        }
                    }
                    break;
                }
            }
        }
    }

    Err("Could not locate the bundled litho binary in any expected location".to_string())
}

// Device enumeration using the litho Rust library API directly
// (liblitho::devices::get_storage_devices and DeviceInfo).
// Note: We depend on the litho package for its Rust APIs, but we are
// NOT bundling the separate `litho` CLI binary for now.

#[derive(serde::Serialize, Clone, Debug)]
pub struct StorageDeviceInfo {
    /// Human friendly name, e.g. "SanDisk Ultra" or basename of the device
    pub name: String,
    /// Full device path, e.g. "/dev/sdb"
    pub path: String,
    /// Human readable size, e.g. "59.6 GB"
    pub size: String,
    pub removable: bool,
    pub model: String,
}

fn format_size_from_sectors(sectors: u64) -> String {
    const SECTOR_SIZE: u64 = 512;
    let bytes = sectors.saturating_mul(SECTOR_SIZE);
    if bytes >= 1_000_000_000_000 {
        format!("{:.1} TB", bytes as f64 / 1_000_000_000_000.0)
    } else if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1_000_000_000.0)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1_000_000.0)
    } else if bytes >= 1_000 {
        format!("{:.1} KB", bytes as f64 / 1_000.0)
    } else {
        format!("{} B", bytes)
    }
}

/// Query storage devices by calling the Rust API in the litho crate directly.
fn query_storage_devices() -> Result<Vec<StorageDeviceInfo>, String> {
    let raw_devices: Vec<LithoDeviceInfo> = litho_devices::get_storage_devices()
        .map_err(|e| format!("Failed to get storage devices: {}", e))?;

    let mut devices: Vec<StorageDeviceInfo> = raw_devices
        .into_iter()
        .map(|raw| {
            let vendor = raw.vendor_name.trim();
            let model = raw.model_name.trim();

            let display_name = if !vendor.is_empty() || !model.is_empty() {
                format!("{} {}", vendor, model).trim().to_string()
            } else if let Some(basename) = raw.device_name.split('/').last() {
                basename.to_string()
            } else {
                "Unknown Device".to_string()
            };

            let name = if display_name.is_empty() {
                "Unknown Device".to_string()
            } else {
                display_name
            };

            StorageDeviceInfo {
                name,
                path: raw.device_name,
                size: format_size_from_sectors(raw.size),
                removable: raw.removable != 0,
                model: model.to_string(),
            }
        })
        .collect();

    // Sort: removable devices first, then by path for stable order
    devices.sort_by(|a, b| {
        match (a.removable, b.removable) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.path.cmp(&b.path),
        }
    });

    Ok(devices)
}

#[tauri::command]
fn get_storage_devices() -> Result<Vec<StorageDeviceInfo>, String> {
    query_storage_devices()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            get_startup_diagnostics,
            get_litho_binary_path,
            get_usable_litho_path_for_privileged,
            get_usable_app_path_for_privileged,
            get_launch_params,
            get_storage_devices,
            relaunch_elevated
        ])
        .setup(|app| {
            // Full diagnostics (this also runs the litho --help check)
            let diag = get_startup_diagnostics(app.handle().clone());

            println!("=== Lithographer Startup Checks ===");
            println!("Desktop Environment : {}", diag.desktop_environment);

            println!("Litho binary path   : {}", diag.litho_binary_path.as_deref().unwrap_or("<not resolved>"));
            println!("litho via polkit    : {}", diag.litho_polkit_result);

            // Direct (non-privileged) verification that the binary itself responds to --help.
            // Privileged re-launch via polkit is now available on demand via the relaunch_elevated API.
            println!("litho --help (direct): {}", diag.litho_help_result);

            println!("Current User        : {}", diag.current_user);
            println!("Running as root     : {}", diag.is_root);
            if let Some(ref agent) = diag.polkit_agent_path {
                println!("Polkit auth agent   : {}", agent);
                println!("Agent executable    : {}", diag.polkit_agent_is_executable);
            } else {
                println!("Polkit auth agent   : NOT FOUND");
            }

            println!("=====================================");

            // Launch parameters (from CLI args) for pre-populating the form
            let launch = parse_launch_args();
            println!("Launch params       : mode={:?}, device={:?}, image={:?}", launch.mode, launch.device, launch.image);

            // Privileged re-launch of the *application* (with the same args) is now
            // available on demand via the `relaunch_elevated` Tauri command.
            // It is not invoked automatically at startup.

            // After startup diagnostics, query storage devices using the Rust API directly
            // (imported liblitho::devices::get_storage_devices).
            println!("--- Querying storage devices ---");
            match query_storage_devices() {
                Ok(devs) => {
                    println!("Found {} storage device(s):", devs.len());
                    for d in &devs {
                        let kind = if d.removable { "removable" } else { "fixed" };
                        println!("  {}  ({} • {})", d.path, d.size, kind);
                    }
                }
                Err(e) => {
                    println!("Storage device query failed: {}", e);
                }
            }
            println!("--------------------------------");

            if diag.is_root {
                println!("Note: App is running as root. Most privileged operations will work directly.");
            } else if diag.polkit_agent_path.is_none() || !diag.polkit_agent_is_executable {
                eprintln!("⚠️  WARNING: No usable polkit authentication agent found.");
                eprintln!("   GUI privilege escalation (for writing to devices) may not work.");
                eprintln!("   Consider installing the appropriate polkit agent for your desktop environment.");
            } else {
                println!("Polkit agent looks ready for privilege escalation requests.");
            }

            if !diag.litho_can_execute {
                eprintln!("⚠️  WARNING: Could not successfully execute the bundled litho binary (litho --help failed).");
                eprintln!("   Image writing operations will not work until this is fixed.");
            } else {
                println!("litho binary is executable (direct --help succeeded).");
            }

            // Note: full privileged elevation (pkexec re-launch of the app with args)
            // is no longer performed automatically at startup.
            // The relaunch_elevated command should be called later when the user
            // actually wants to perform a privileged flash/clone operation.

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_diagnostics_runs_without_panic() {
        let diag = perform_startup_checks();

        // Basic sanity: we always get some strings
        assert!(!diag.desktop_environment.is_empty());
        assert!(!diag.current_user.is_empty());
        assert!(!diag.litho_polkit_result.is_empty());

        // Print for visibility when running with --nocapture
        println!("=== TEST: Startup Diagnostics ===");
        println!("DE: {}", diag.desktop_environment);
        println!("User: {}", diag.current_user);
        println!("Root: {}", diag.is_root);
        println!("Polkit agent: {:?}", diag.polkit_agent_path);
        println!("Executable: {}", diag.polkit_agent_is_executable);
        println!("litho via polkit: {}", diag.litho_polkit_result);
        println!("Litho path: {:?}", diag.litho_binary_path);
        println!("Litho can execute: {}", diag.litho_can_execute);
        println!("Litho --help (direct): {}", diag.litho_help_result);
        println!("Litho via polkit: {}", diag.litho_polkit_result);
        println!("=================================");
    }

    #[test]
    fn test_desktop_env_detection() {
        let de = get_desktop_environment();
        println!("Detected desktop environment: {}", de);
        // Should not be empty
        assert!(!de.is_empty());
    }

    #[test]
    fn test_root_detection() {
        let root = is_running_as_root();
        println!("is_root() = {}", root);
        // In normal test runs we are not root
        // (this is informational)
    }

    #[test]
    fn test_litho_binary_help_check() {
        // Resolve relative to the crate's manifest dir (src-tauri/ when testing the lib).
        // The binary lives at src-tauri/resources/litho.
        let litho_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("litho");

        println!("Testing litho --help against: {}", litho_path.display());

        let (can_execute, result) = test_litho_binary_help(&litho_path);

        println!("  can_execute = {}", can_execute);
        println!("  result      = {}", result);

        // The binary should exist and --help should succeed
        assert!(litho_path.exists(), "litho binary should be present for this test");
        assert!(can_execute, "litho --help should succeed: {}", result);
        assert!(result.contains("SUCCESS"), "result should indicate success");
    }
}
