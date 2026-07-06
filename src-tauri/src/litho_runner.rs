use crate::litho_output::{parse_litho_line, LithoUiEvent};
use crate::litho_sidecar::resolve_litho_binary;
#[cfg(target_os = "linux")]
use crate::privilege::build_elevated_litho_command;
use crate::privilege::{elevation_method, has_privileged_access, spawn_mode};
use liblitho::cancel::{create_cancel_file, remove_cancel_file, request_cancel_via_file};
use liblitho::progress::STDIN_CANCEL_LINE;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Matches `cli_cancel::CANCEL_EXIT_CODE` in the litho CLI.
const LITHO_CANCEL_EXIT_CODE: i32 = 3;

/// Win32 ERROR_CANCELLED when the user denies a UAC prompt.
#[cfg(windows)]
const UAC_CANCELLED_EXIT: i32 = 1223;

#[derive(Default)]
pub struct LithoRunnerState {
    pub child: Option<Child>,
    pub child_stdin: Option<ChildStdin>,
    pub cancel_file: Option<PathBuf>,
    pub running: bool,
    pub cancel_requested: bool,
}


pub type SharedLithoRunner = Arc<Mutex<LithoRunnerState>>;

#[derive(Clone, Debug)]
pub struct LithoRunRequest {
    pub mode: String,
    pub device: String,
    pub image: String,
    pub block_size: usize,
    pub verify: bool,
    /// User confirmed automatic unmount of volumes on the target disk (`--yes` for litho).
    pub auto_unmount: bool,
}

pub fn spawn_litho_operation(
    app: AppHandle,
    state: SharedLithoRunner,
    request: LithoRunRequest,
) -> Result<(), String> {
    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if guard.running {
            return Err("An operation is already in progress.".to_string());
        }
        guard.cancel_requested = false;
    }

    let litho_path = resolve_litho_binary(&app)?;
    liblitho::devices::validate_device_for_io(&request.device)?;

    #[cfg(windows)]
    if !has_privileged_access() {
        return handoff_to_elevated_lithographer(app, &request);
    }

    let cancel_file =
        create_cancel_file().map_err(|e| format!("Failed to create cancel flag file: {e}"))?;

    let device = request.device.clone();
    let mode = request.mode.to_lowercase();
    let subcommand = if mode == "clone" || mode == "backup" {
        "clone"
    } else {
        "flash"
    };

    let mut litho_args = vec![
        "-o".to_string(),
        "gui".to_string(),
        subcommand.to_string(),
        "-f".to_string(),
        request.image.clone(),
        "-d".to_string(),
        device.clone(),
        "-b".to_string(),
        request.block_size.to_string(),
        "--cancel-file".to_string(),
        cancel_file.display().to_string(),
    ];

    if subcommand == "flash" && request.verify {
        litho_args.push("--verify".to_string());
    }

    // Lithographer only spawns litho after explicit user confirmation (Start / mount dialog).
    // --yes lets litho auto-unmount/dismount volumes on the target disk before raw I/O.
    let _ = request.auto_unmount;
    litho_args.push("--yes".to_string());

    let mut cmd = if has_privileged_access() {
        let mut c = Command::new(&litho_path);
        c.args(&litho_args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Console-subsystem litho.exe must not inherit or allocate a visible console;
            // stdout/stderr are piped into the GUI protocol parser below.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            detach_parent_console_before_spawn();
            c.creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS);
        }
        c
    } else {
        #[cfg(target_os = "linux")]
        {
            build_elevated_litho_command(&litho_path, &litho_args)?
        }
        #[cfg(not(target_os = "linux"))]
        {
            return Err("Privileged flash/clone is not supported on this platform.".into());
        }
    };

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::piped());

    let launcher = if has_privileged_access() {
        litho_path.display().to_string()
    } else {
        elevation_method()
    };

    println!(
        "Spawning litho sidecar (mode={}, verify={}, cancel_file={}): {} {:?}",
        spawn_mode(),
        request.verify,
        cancel_file.display(),
        launcher,
        litho_args
    );

    let mut child = cmd.spawn().map_err(|e| {
        #[cfg(windows)]
        {
            format!("Failed to spawn litho: {e}.")
        }
        #[cfg(not(windows))]
        {
            format!("Failed to spawn litho: {e}. Is pkexec installed?")
        }
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture litho stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture litho stderr".to_string())?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to open litho stdin".to_string())?;

    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.running = true;
        guard.child = Some(child);
        guard.child_stdin = Some(stdin);
        guard.cancel_file = Some(cancel_file);
    }

    let app_stdout = app.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            emit_parsed_line(&app_stdout, "stdout", &line);
        }
    });

    let app_stderr = app.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            emit_parsed_line(&app_stderr, "stderr", &line);
        }
    });

    let app_wait = app.clone();
    let state_wait = Arc::clone(&state);
    std::thread::spawn(move || {
        let mut child = {
            let mut guard = match state_wait.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            guard.child_stdin.take();
            guard.child.take()
        };

        let exit_code = child
            .as_mut()
            .and_then(|child| child.wait().ok().and_then(|status| status.code()));

        finish_litho_operation(&app_wait, &state_wait, exit_code);
    });

    Ok(())
}

#[cfg(windows)]
fn lithographer_elevation_cli_args(request: &LithoRunRequest) -> Vec<String> {
    let mut args = vec![
        "--mode".to_string(),
        request.mode.clone(),
        "--device".to_string(),
        request.device.clone(),
        "--image".to_string(),
        request.image.clone(),
        "--block-size".to_string(),
        request.block_size.to_string(),
        "--auto-run".to_string(),
    ];
    if request.verify {
        args.push("--verify".to_string());
    }
    if request.auto_unmount {
        args.push("--auto-unmount".to_string());
    }
    args
}

/// Drop an inherited debug/dev console so a console-subsystem litho child cannot write
/// progress to a stray terminal instead of our stdout pipe.
#[cfg(windows)]
fn detach_parent_console_before_spawn() {
    use winapi::um::wincon::{FreeConsole, GetConsoleWindow};
    use winapi::um::winuser::ShowWindow;
    use winapi::um::winuser::SW_HIDE;

    unsafe {
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_HIDE);
        }
        let _ = FreeConsole();
    }
}

/// UAC cannot pipe stdout from an elevated litho child. Relaunch Lithographer elevated
/// so litho runs as a normal piped subprocess in the elevated GUI session.
#[cfg(windows)]
fn handoff_to_elevated_lithographer(
    app: AppHandle,
    request: &LithoRunRequest,
) -> Result<(), String> {
    let args = lithographer_elevation_cli_args(request);
    println!(
        "Handing off to elevated Lithographer (mode=uac-relaunch): {:?}",
        args
    );

    crate::privilege::relaunch_elevated_lithographer(&args).map_err(|message| {
        let _ = app.emit(
            "litho-event",
            LithoUiEvent::Error {
                message: message.clone(),
            },
        );
        message
    })?;

    let _ = app.emit(
        "litho-event",
        LithoUiEvent::Status {
            phase: Some("preparing".to_string()),
            message: Some(
                "Approve UAC to open an elevated Lithographer window. Progress will stream there."
                    .to_string(),
            ),
        },
    );

    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_millis(600));
        std::process::exit(0);
    });

    Ok(())
}

fn finish_litho_operation(app: &AppHandle, state: &SharedLithoRunner, exit_code: Option<i32>) {
    let (cancel_requested, cancel_file) = {
        let mut guard = match state.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let cancel_requested = guard.cancel_requested;
        let cancel_file = guard.cancel_file.take();
        guard.running = false;
        guard.cancel_requested = false;
        guard.child_stdin = None;
        guard.child = None;
        (cancel_requested, cancel_file)
    };

    if let Some(path) = cancel_file {
        remove_cancel_file(&path);
    }

    emit_completion_event(app, exit_code, cancel_requested);
}

pub fn cancel_litho_operation(state: &SharedLithoRunner) -> Result<(), String> {
    let cancel_file = {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        if !guard.running {
            return Ok(());
        }
        guard.cancel_requested = true;
        guard.cancel_file.clone()
    };

    let path = cancel_file.ok_or_else(|| "Cancel flag file is not available.".to_string())?;
    request_cancel_via_file(&path).map_err(|e| format!("Failed to write cancel flag file: {e}"))?;

    // Stdin is a best-effort fallback when pkexec forwards it (usually it does not).
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    if let Some(stdin) = guard.child_stdin.as_mut() {
        let _ = writeln!(stdin, "{STDIN_CANCEL_LINE}");
        let _ = stdin.flush();
    }

    Ok(())
}

fn emit_completion_event(app: &AppHandle, exit_code: Option<i32>, cancel_requested: bool) {
    match exit_code {
        Some(0) => {
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Done {
                    success: true,
                    cancelled: false,
                },
            );
        }
        Some(LITHO_CANCEL_EXIT_CODE) | _ if cancel_requested => {
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Done {
                    success: false,
                    cancelled: true,
                },
            );
        }
        Some(126) => {
            emit_auth_denied(app);
        }
        #[cfg(windows)]
        Some(UAC_CANCELLED_EXIT) => {
            emit_auth_denied(app);
        }
        Some(code) => {
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Error {
                    message: litho_exit_message(code),
                },
            );
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Done {
                    success: false,
                    cancelled: false,
                },
            );
        }
        None => {
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Error {
                    message: "litho process ended without an exit code.".to_string(),
                },
            );
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Done {
                    success: false,
                    cancelled: false,
                },
            );
        }
    }
}

fn litho_exit_message(code: i32) -> String {
    #[cfg(windows)]
    {
        return match code {
            1 => "Flash/clone failed. The disk may be in use (close File Explorer on that drive), \
                  or access was denied. Run Lithographer from a console (or set LITHOGRAPHER_CONSOLE=1) \
                  for litho error details."
                .to_string(),
            5 => "Flash/clone failed: access denied. Close programs using the target disk and retry."
                .to_string(),
            _ => format!(
                "litho exited with code {code}. Run from a console for detailed litho output."
            ),
        };
    }

    #[cfg(not(windows))]
    {
        format!("litho exited with code {code}")
    }
}

fn emit_auth_denied(app: &AppHandle) {
    let _ = app.emit(
        "litho-event",
        LithoUiEvent::Error {
            message: "Authentication was cancelled or denied.".to_string(),
        },
    );
    let _ = app.emit(
        "litho-event",
        LithoUiEvent::Done {
            success: false,
            cancelled: false,
        },
    );
}

fn emit_parsed_line(app: &AppHandle, stream: &str, line: &str) {
    let event = parse_litho_line(stream, line);
    let _ = app.emit("litho-event", event);
}
