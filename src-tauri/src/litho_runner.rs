use crate::litho_output::{parse_litho_line, LithoUiEvent};
use crate::litho_sidecar::resolve_litho_binary;
use crate::is_running_as_root;
use liblitho::cancel::{create_cancel_file, remove_cancel_file, request_cancel_via_file};
use liblitho::devices::validate_device_safe_for_io;
use liblitho::progress::STDIN_CANCEL_LINE;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Matches `cli_cancel::CANCEL_EXIT_CODE` in the litho CLI.
const LITHO_CANCEL_EXIT_CODE: i32 = 3;

pub struct LithoRunnerState {
    pub child: Option<Child>,
    pub child_stdin: Option<ChildStdin>,
    pub cancel_file: Option<PathBuf>,
    pub running: bool,
    pub cancel_requested: bool,
}

impl Default for LithoRunnerState {
    fn default() -> Self {
        Self {
            child: None,
            child_stdin: None,
            cancel_file: None,
            running: false,
            cancel_requested: false,
        }
    }
}

pub type SharedLithoRunner = Arc<Mutex<LithoRunnerState>>;

#[derive(Clone, Debug)]
pub struct LithoRunRequest {
    pub mode: String,
    pub device: String,
    pub image: String,
    pub block_size: usize,
    pub verify: bool,
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

    let cancel_file = create_cancel_file()
        .map_err(|e| format!("Failed to create cancel flag file: {e}"))?;

    let litho_path = resolve_litho_binary(&app)?;
    validate_device_safe_for_io(&request.device)?;
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

    let mut cmd = if is_running_as_root() {
        let mut c = Command::new(&litho_path);
        c.args(&litho_args);
        c
    } else {
        litho_args.insert(0, litho_path.display().to_string());
        let mut c = Command::new("pkexec");
        c.args(&litho_args);
        c
    };

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    cmd.stdin(Stdio::piped());

    println!(
        "Spawning litho sidecar (verify={}, cancel_file={}): {} {:?}",
        request.verify,
        cancel_file.display(),
        if is_running_as_root() {
            litho_path.display().to_string()
        } else {
            "pkexec".to_string()
        },
        litho_args
    );

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn litho: {e}. Is pkexec installed?"))?;

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

        let (cancel_requested, cancel_file) = {
            let mut guard = match state_wait.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let cancel_requested = guard.cancel_requested;
            let cancel_file = guard.cancel_file.take();
            guard.running = false;
            guard.cancel_requested = false;
            guard.child_stdin = None;
            (cancel_requested, cancel_file)
        };

        if let Some(path) = cancel_file {
            remove_cancel_file(&path);
        }

        emit_completion_event(&app_wait, exit_code, cancel_requested);
    });

    Ok(())
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
    request_cancel_via_file(&path)
        .map_err(|e| format!("Failed to write cancel flag file: {e}"))?;

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
        Some(code) => {
            let _ = app.emit(
                "litho-event",
                LithoUiEvent::Error {
                    message: format!("litho exited with code {code}"),
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

fn emit_parsed_line(app: &AppHandle, stream: &str, line: &str) {
    let event = parse_litho_line(stream, line);
    let _ = app.emit("litho-event", event);
}