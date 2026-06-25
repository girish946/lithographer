use crate::litho_output::{parse_litho_line, LithoUiEvent};
use crate::litho_sidecar::resolve_litho_binary;
use liblitho::devices::validate_device_safe_for_io;
use crate::is_running_as_root;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub struct LithoRunnerState {
    pub child: Option<Child>,
    pub running: bool,
}

impl Default for LithoRunnerState {
    fn default() -> Self {
        Self {
            child: None,
            running: false,
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
        let guard = state.lock().map_err(|e| e.to_string())?;
        if guard.running {
            return Err("An operation is already in progress.".to_string());
        }
    }

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
    cmd.stdin(Stdio::null());

    println!(
        "Spawning litho sidecar (verify={}): {} {:?}",
        request.verify,
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

    {
        let mut guard = state.lock().map_err(|e| e.to_string())?;
        guard.running = true;
        guard.child = Some(child);
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
        let exit_code = {
            let mut guard = match state_wait.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            let mut code = None;
            if let Some(child) = guard.child.as_mut() {
                code = child.wait().ok().and_then(|s| s.code());
            }
            guard.child = None;
            guard.running = false;
            code
        };

        match exit_code {
            Some(0) => {
                let _ = app_wait.emit("litho-event", LithoUiEvent::Done { success: true });
            }
            Some(126) => {
                let _ = app_wait.emit(
                    "litho-event",
                    LithoUiEvent::Error {
                        message: "Authentication was cancelled or denied.".to_string(),
                    },
                );
                let _ = app_wait.emit("litho-event", LithoUiEvent::Done { success: false });
            }
            Some(code) => {
                let _ = app_wait.emit(
                    "litho-event",
                    LithoUiEvent::Error {
                        message: format!("litho exited with code {code}"),
                    },
                );
                let _ = app_wait.emit("litho-event", LithoUiEvent::Done { success: false });
            }
            None => {
                let _ = app_wait.emit(
                    "litho-event",
                    LithoUiEvent::Error {
                        message: "litho process ended without an exit code.".to_string(),
                    },
                );
                let _ = app_wait.emit("litho-event", LithoUiEvent::Done { success: false });
            }
        }
    });

    Ok(())
}

pub fn cancel_litho_operation(state: &SharedLithoRunner) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    if let Some(child) = guard.child.as_mut() {
        child
            .kill()
            .map_err(|e| format!("Failed to cancel litho process: {e}"))?;
    }
    guard.child = None;
    guard.running = false;
    Ok(())
}

fn emit_parsed_line(app: &AppHandle, stream: &str, line: &str) {
    let event = parse_litho_line(stream, line);
    let _ = app.emit("litho-event", event);
}