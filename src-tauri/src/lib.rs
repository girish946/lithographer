mod litho_output;
mod litho_runner;
mod litho_sidecar;
mod privilege;

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
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use liblitho::devices::{self as litho_devices, DeviceInfo as LithoDeviceInfo};

#[derive(serde::Serialize)]
struct StartupDiagnostics {
    platform: String,
    desktop_environment: String,
    current_user: String,
    is_elevated: bool,
    elevation_method: String,
    elevation_agent: Option<String>,
    elevation_ready: bool,
    elevation_status: String,
    spawn_mode: String,
    litho_spawn_preview: String,
    /// Legacy alias for `is_elevated` (Linux root / Windows Administrator).
    is_root: bool,
    /// Legacy alias for `elevation_agent` on Linux (polkit agent path).
    polkit_agent_path: Option<String>,
    /// Legacy alias for `elevation_ready`.
    polkit_agent_is_executable: bool,
    /// Legacy alias for `elevation_status`.
    polkit_status: String,
}

#[derive(serde::Serialize, Clone, Debug)]
struct LaunchParams {
    mode: Option<String>,
    device: Option<String>,
    image: Option<String>,
    auto_run: bool,
    verify: bool,
    block_size: Option<usize>,
}

fn parse_launch_args() -> LaunchParams {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut params = LaunchParams {
        mode: None,
        device: None,
        image: None,
        auto_run: false,
        verify: false,
        block_size: None,
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
            "--block-size" | "-b" => {
                if i + 1 < args.len() {
                    params.block_size = args[i + 1].parse().ok();
                    i += 1;
                }
            }
            "--auto-run" => {
                params.auto_run = true;
            }
            "--verify" => {
                params.verify = true;
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
    privilege::has_privileged_access()
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

fn platform_name() -> String {
    if cfg!(windows) {
        "windows".to_string()
    } else if cfg!(target_os = "linux") {
        "linux".to_string()
    } else {
        std::env::consts::OS.to_string()
    }
}

fn litho_gui_args_preview() -> &'static str {
    "-o gui <flash|clone> -f <image> -d <device> -b <block_size> --cancel-file <path> [--verify]"
}

fn litho_spawn_preview(sidecar: &Path, is_elevated: bool) -> String {
    let litho = sidecar.display();
    let args = litho_gui_args_preview();

    if is_elevated {
        return format!("{litho} {args}");
    }

    #[cfg(windows)]
    {
        return format!(
            "UAC runas → lithographer --auto-run … → {litho} {args}"
        );
    }

    #[cfg(not(windows))]
    {
        format!("pkexec {litho} {args}")
    }
}

fn litho_spawn_preview_unresolved(is_elevated: bool) -> String {
    litho_spawn_preview(Path::new("<litho-sidecar>"), is_elevated)
}

fn perform_startup_checks_with_sidecar(sidecar: Option<&Path>) -> StartupDiagnostics {
    let platform = platform_name();
    let desktop_environment = privilege::platform_environment();
    let current_user = get_current_user();
    let is_elevated = privilege::has_privileged_access();
    let elevation_method = privilege::elevation_method().to_string();
    let spawn_mode = privilege::spawn_mode().to_string();

    #[cfg(windows)]
    let (elevation_agent, elevation_ready, elevation_status) = {
        let agent = Some("UAC (User Account Control)".to_string());
        let ready = true;
        let status = if is_elevated {
            "Running elevated (Administrator). Flash/clone operations will run directly.".to_string()
        } else {
            "UAC available. Litho will request administrator approval when an operation starts."
                .to_string()
        };
        (agent, ready, status)
    };

    #[cfg(not(windows))]
    let (elevation_agent, elevation_ready, elevation_status) = {
        let agent = find_polkit_auth_agent();
        let ready = agent
            .as_ref()
            .map(|p| is_file_executable(p))
            .unwrap_or(false);
        let status = if ready {
            "Polkit agent present. Elevation deferred until operation is requested.".to_string()
        } else {
            "No usable polkit authentication agent detected at startup.".to_string()
        };
        (agent, ready, status)
    };

    let litho_spawn_preview = match sidecar {
        Some(path) => litho_spawn_preview(path, is_elevated),
        None => litho_spawn_preview_unresolved(is_elevated),
    };

    StartupDiagnostics {
        platform,
        desktop_environment,
        current_user,
        is_elevated,
        elevation_method,
        elevation_agent: elevation_agent.clone(),
        elevation_ready,
        elevation_status: elevation_status.clone(),
        spawn_mode,
        litho_spawn_preview,
        is_root: is_elevated,
        polkit_agent_path: elevation_agent,
        polkit_agent_is_executable: elevation_ready,
        polkit_status: elevation_status,
    }
}

fn perform_startup_checks() -> StartupDiagnostics {
    perform_startup_checks_with_sidecar(None)
}

fn log_startup_report(diag: &StartupDiagnostics, sidecar: &Result<std::path::PathBuf, String>) {
    println!("=== Lithographer Startup Checks ===");
    println!("Platform            : {}", diag.platform);
    println!("Environment         : {}", diag.desktop_environment);
    println!("Current user        : {}", diag.current_user);
    println!("Elevated            : {}", diag.is_elevated);
    println!("Elevation method    : {}", diag.elevation_method);
    println!("Elevation ready     : {}", diag.elevation_ready);
    if let Some(ref agent) = diag.elevation_agent {
        println!("Elevation agent     : {}", agent);
    } else {
        println!("Elevation agent     : NOT FOUND");
    }
    println!("Elevation status    : {}", diag.elevation_status);
    println!("Litho spawn mode    : {}", diag.spawn_mode);
    println!("Litho command       : {}", diag.litho_spawn_preview);
    println!("=====================================");

    let launch = parse_launch_args();
    println!(
        "Launch params       : mode={:?}, device={:?}, image={:?}",
        launch.mode, launch.device, launch.image
    );

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

    if diag.is_elevated {
        #[cfg(windows)]
        println!("Note: App is running elevated. Litho sidecar will be spawned directly.");
        #[cfg(not(windows))]
        println!("Note: App is running as root. Litho sidecar will be spawned directly.");
    } else if !diag.elevation_ready {
        #[cfg(windows)]
        eprintln!("WARNING: Administrator elevation (UAC) is not available.");
        #[cfg(not(windows))]
        {
            eprintln!("WARNING: No usable polkit authentication agent found.");
            eprintln!("   GUI privilege escalation (for writing to devices) may not work.");
            eprintln!("   Install the polkit agent for your desktop environment.");
        }
    } else {
        #[cfg(windows)]
        println!("UAC looks ready. Litho will prompt for administrator approval per operation.");
        #[cfg(not(windows))]
        println!("Polkit agent looks ready for privilege escalation requests.");
    }

    match sidecar {
        Ok(path) => println!("Litho sidecar path    : {}", path.display()),
        Err(err) => eprintln!("Litho sidecar missing : {err}"),
    }
}

#[tauri::command]
fn get_startup_diagnostics(app: tauri::AppHandle) -> StartupDiagnostics {
    let sidecar = litho_sidecar::resolve_litho_binary(&app).ok();
    perform_startup_checks_with_sidecar(sidecar.as_ref().map(|p| p.as_path()))
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
        #[cfg(windows)]
        return Err("Administrator elevation (UAC) is not available on this system.".to_string());
        #[cfg(not(windows))]
        return Err(
            "No polkit authentication agent found. Start your desktop polkit agent.".to_string(),
        );
    }

    let launch = parse_launch_args();
    if launch.auto_run {
        // UAC handoff replays the same device path; WMI may enumerate slightly later.
        litho_devices::validate_device_safe_for_io(&device)?;
    } else {
        let known_devices = query_storage_devices()?;
        let known_paths: Vec<String> = known_devices.iter().map(|d| d.path.clone()).collect();
        litho_devices::validate_listed_block_device(&device, &known_paths)?;
    }

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

/// Resize the main window height to match rendered content (width unchanged).
#[tauri::command]
fn fit_window_to_content(
    window: tauri::WebviewWindow,
    content_height: f64,
) -> Result<(), String> {
    use tauri::{LogicalSize, Size};

    const MIN_HEIGHT: f64 = 360.0;

    let scale = window.scale_factor().map_err(|e| e.to_string())?;
    let outer = window.outer_size().map_err(|e| e.to_string())?;
    let logical_width = outer.width as f64 / scale;
    let height = content_height.max(MIN_HEIGHT);

    window
        .set_size(Size::Logical(LogicalSize {
            width: logical_width,
            height,
        }))
        .map_err(|e| e.to_string())
}

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
            fit_window_to_content,
            start_litho_operation,
            cancel_litho_operation,
            get_litho_sidecar_path
        ])
        .setup(|_app| {
            let sidecar = litho_sidecar::resolve_litho_binary(_app.handle());
            let diag =
                perform_startup_checks_with_sidecar(sidecar.as_ref().ok().map(|p| p.as_path()));
            log_startup_report(&diag, &sidecar);

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
