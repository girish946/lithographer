mod litho_output;
mod litho_runner;
mod litho_sidecar;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

use litho_runner::{
    cancel_litho_operation as stop_litho_child, spawn_litho_operation, LithoRunRequest,
    LithoRunnerState, SharedLithoRunner,
};

use std::fs;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use liblitho::devices::{self as litho_devices, DeviceInfo as LithoDeviceInfo};

#[derive(serde::Serialize)]
struct StartupDiagnostics {
    desktop_environment: String,
    current_user: String,
    is_root: bool,
    polkit_agent_path: Option<String>,
    polkit_agent_is_executable: bool,
    polkit_status: String,
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

pub(crate) fn is_running_as_root() -> bool {
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

fn perform_startup_checks() -> StartupDiagnostics {
    let desktop_environment = get_desktop_environment();
    let current_user = get_current_user();
    let is_root = is_running_as_root();
    let polkit_agent_path = find_polkit_auth_agent();
    let polkit_agent_is_executable = polkit_agent_path
        .as_ref()
        .map(|p| is_file_executable(p))
        .unwrap_or(false);
    // Privileged re-launch is deferred until the user starts an operation.
    let polkit_status = if polkit_agent_path.as_ref().map_or(false, |p| is_file_executable(p)) {
        "Polkit agent present. Elevation deferred until operation is requested.".to_string()
    } else {
        "No usable polkit authentication agent detected at startup.".to_string()
    };

    StartupDiagnostics {
        desktop_environment,
        current_user,
        is_root,
        polkit_agent_path,
        polkit_agent_is_executable,
        polkit_status,
    }
}

#[tauri::command]
fn get_startup_diagnostics() -> StartupDiagnostics {
    perform_startup_checks()
}

/// Spawn the litho CLI sidecar (via pkexec when not root) and stream GUI protocol
/// lines back to the frontend as `litho-event` payloads.
///
/// When `block_size` is omitted, picks the largest I/O buffer allowed for the target
/// device (same table as historical Lithographer `execute` / litho-tui).
#[tauri::command]
fn start_litho_operation(
    app: tauri::AppHandle,
    state: tauri::State<'_, SharedLithoRunner>,
    mode: String,
    device: String,
    image: String,
    block_size: Option<usize>,
    verify: Option<bool>,
) -> Result<(), String> {
    let diag = perform_startup_checks();
    if !diag.is_root && !diag.polkit_agent_is_executable {
        return Err(
            "No polkit authentication agent found. Start your desktop polkit agent.".to_string(),
        );
    }

    let known_devices = query_storage_devices()?;
    let known_paths: Vec<String> = known_devices.iter().map(|d| d.path.clone()).collect();
    litho_devices::validate_listed_block_device(&device, &known_paths)?;

    let io_block_size = block_size
        .unwrap_or_else(|| litho_devices::optimal_io_block_size(&device));
    let verify = verify.unwrap_or(false);

    spawn_litho_operation(
        app,
        Arc::clone(state.inner()),
        LithoRunRequest {
            mode,
            device,
            image,
            block_size: io_block_size,
            verify,
        },
    )
}

#[tauri::command]
fn cancel_litho_operation(state: tauri::State<'_, SharedLithoRunner>) -> Result<(), String> {
    stop_litho_child(state.inner())
}

#[tauri::command]
fn get_litho_sidecar_path(app: tauri::AppHandle) -> Result<String, String> {
    litho_sidecar::resolve_litho_binary(&app).map(|p| p.to_string_lossy().to_string())
}

// Device enumeration and disk I/O use the litho Rust library (liblitho) in-process.

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
                path: raw.device_name.clone(),
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

const IMAGE_EXTENSIONS: &[&str] = &["img", "iso", "raw", "dd", "bin", "xz", "wim", "dmg"];

/// Native open/save dialog for flash source or clone destination.
#[tauri::command]
fn pick_image_path(mode: String) -> Result<Option<String>, String> {
    let is_clone = mode.eq_ignore_ascii_case("clone") || mode.eq_ignore_ascii_case("backup");

    let mut dialog = rfd::FileDialog::new();
    dialog = dialog
        .set_title(if is_clone {
            "Choose output image file"
        } else {
            "Choose image to flash"
        })
        .add_filter("Disk images", IMAGE_EXTENSIONS)
        .add_filter("All files", &["*"]);

    let picked = if is_clone {
        dialog.save_file()
    } else {
        dialog.pick_file()
    };

    Ok(picked.map(|p| p.to_string_lossy().into_owned()))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let litho_runner: SharedLithoRunner = Arc::new(Mutex::new(LithoRunnerState::default()));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(litho_runner)
        .invoke_handler(tauri::generate_handler![
            greet,
            get_startup_diagnostics,
            get_launch_params,
            get_storage_devices,
            pick_image_path,
            start_litho_operation,
            cancel_litho_operation,
            get_litho_sidecar_path
        ])
        .setup(|_app| {
            let diag = perform_startup_checks();

            println!("=== Lithographer Startup Checks ===");
            println!("Desktop Environment : {}", diag.desktop_environment);
            println!("Current User        : {}", diag.current_user);
            println!("Running as root     : {}", diag.is_root);
            println!("Polkit status       : {}", diag.polkit_status);
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

            // Privileged flash/clone runs via the litho CLI sidecar (`start_litho_operation`).

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

            match litho_sidecar::resolve_litho_binary(_app.handle()) {
                Ok(path) => println!("Litho sidecar         : {}", path.display()),
                Err(e) => eprintln!("⚠️  Litho sidecar missing: {e}"),
            }

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
        assert!(!diag.polkit_status.is_empty());

        // Print for visibility when running with --nocapture
        println!("=== TEST: Startup Diagnostics ===");
        println!("DE: {}", diag.desktop_environment);
        println!("User: {}", diag.current_user);
        println!("Root: {}", diag.is_root);
        println!("Polkit agent: {:?}", diag.polkit_agent_path);
        println!("Executable: {}", diag.polkit_agent_is_executable);
        println!("Polkit status: {}", diag.polkit_status);
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

}
