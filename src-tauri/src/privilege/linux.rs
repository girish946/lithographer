use std::fs;
use std::path::Path;
use std::process::Command;

/// How litho is elevated when the GUI is unprivileged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElevationBackend {
    Pkexec,
    SudoAskpass,
    Run0,
}

pub fn is_running_as_root() -> bool {
    if let Ok(output) = Command::new("id").arg("-u").output() {
        let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        uid == "0"
    } else {
        false
    }
}

/// Whether privilege escalation can plausibly show an auth dialog on this session.
pub fn polkit_agent_ready() -> bool {
    match elevation_backend() {
        ElevationBackend::Pkexec => {
            find_polkit_auth_agent().is_some() || gnome_shell_polkit_ready() || pkexec_on_path()
        }
        ElevationBackend::Run0 => run0_on_path(),
        ElevationBackend::SudoAskpass => command_on_path("sudo") && resolve_sudo_askpass().is_ok(),
    }
}

pub fn elevation_backend() -> ElevationBackend {
    match std::env::var("LITHOGRAPHER_ELEVATION")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "sudo" | "sudo-askpass" => ElevationBackend::SudoAskpass,
        "run0" => ElevationBackend::Run0,
        _ => ElevationBackend::Pkexec,
    }
}

pub fn elevation_backend_label(backend: ElevationBackend) -> &'static str {
    match backend {
        ElevationBackend::Pkexec => "pkexec",
        ElevationBackend::SudoAskpass => "sudo -A",
        ElevationBackend::Run0 => "run0",
    }
}

/// Build a command that runs `litho` with `args` using the configured elevation backend.
pub fn build_elevated_litho_command(litho_path: &Path, args: &[String]) -> Result<Command, String> {
    let backend = elevation_backend();
    match backend {
        ElevationBackend::Pkexec => build_pkexec_command(litho_path, args),
        ElevationBackend::SudoAskpass => build_sudo_askpass_command(litho_path, args),
        ElevationBackend::Run0 => build_run0_command(litho_path, args),
    }
}

fn build_pkexec_command(litho_path: &Path, args: &[String]) -> Result<Command, String> {
    if !pkexec_on_path() {
        return Err("pkexec not found. Install polkit or set LITHOGRAPHER_ELEVATION=sudo.".into());
    }
    let mut cmd = Command::new("pkexec");
    cmd.arg(litho_path);
    cmd.args(args);
    Ok(cmd)
}

fn build_run0_command(litho_path: &Path, args: &[String]) -> Result<Command, String> {
    if !run0_on_path() {
        return Err(
            "run0 not found. Install systemd (Fedora 41+) or set LITHOGRAPHER_ELEVATION=pkexec."
                .into(),
        );
    }
    let mut cmd = Command::new("run0");
    cmd.arg(litho_path);
    cmd.args(args);
    Ok(cmd)
}

fn build_sudo_askpass_command(litho_path: &Path, args: &[String]) -> Result<Command, String> {
    let askpass = resolve_sudo_askpass()?;
    let mut cmd = Command::new("sudo");
    cmd.env("SUDO_ASKPASS", askpass);
    cmd.arg("-A");
    cmd.arg(litho_path);
    cmd.args(args);
    Ok(cmd)
}

fn resolve_sudo_askpass() -> Result<String, String> {
    if let Ok(path) = std::env::var("SUDO_ASKPASS") {
        if !path.is_empty() && Path::new(&path).is_file() {
            return Ok(path);
        }
    }

    for path in [
        "/usr/bin/ksshaskpass",
        "/usr/bin/kdialog",
        "/usr/bin/zenity",
        "/usr/libexec/openssh/gnome-ssh-askpass",
        "/usr/libexec/ssh-askpass",
        "/usr/bin/ssh-askpass",
        "/usr/bin/x11-ssh-askpass",
    ] {
        if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
            if path.ends_with("zenity") {
                return write_zenity_askpass_helper();
            }
            return Ok(path.to_string());
        }
    }

    Err(
        "No graphical sudo askpass found. Set SUDO_ASKPASS, install ksshaskpass/zenity, \
         or use LITHOGRAPHER_ELEVATION=pkexec."
            .into(),
    )
}

fn write_zenity_askpass_helper() -> Result<String, String> {
    let helper = std::env::temp_dir().join(format!(
        "lithographer-zenity-askpass-{}",
        std::process::id()
    ));
    let script = "#!/bin/sh\nexec zenity --password --title=\"Lithographer requires administrator access\"\n";
    fs::write(&helper, script)
        .map_err(|e| format!("Failed to write zenity askpass helper: {e}"))?;
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&helper)
            .map_err(|e| format!("Failed to chmod zenity askpass helper: {e}"))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&helper, perms)
            .map_err(|e| format!("Failed to chmod zenity askpass helper: {e}"))?;
    }
    Ok(helper.to_string_lossy().into_owned())
}

fn pkexec_on_path() -> bool {
    command_on_path("pkexec")
}

fn run0_on_path() -> bool {
    command_on_path("run0")
}

fn command_on_path(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Fedora/GNOME: polkit-gnome was retired; gnome-shell provides the auth UI in-session.
fn gnome_shell_polkit_ready() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| std::env::var("GNOME_DESKTOP_SESSION_ID").map(|_| "GNOME".to_string()))
        .unwrap_or_default()
        .to_lowercase();
    if !desktop.contains("gnome") {
        return false;
    }

    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();

    if user.is_empty() {
        return false;
    }

    if let Ok(output) = Command::new("ps")
        .args(["-u", &user, "-o", "comm=", "--no-headers"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
        return stdout.lines().any(|line| line.trim() == "gnome-shell");
    }

    false
}

fn find_polkit_auth_agent() -> Option<String> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();

    if let Ok(output) = Command::new("ps")
        .args(["-u", &user, "-o", "pid,comm,args", "--no-headers"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let lower = line.to_lowercase();
            if lower.contains("polkit")
                && (lower.contains("agent") || lower.contains("-authentication-agent"))
            {
                for token in line.split_whitespace() {
                    if token.starts_with('/') && token.contains("polkit") {
                        if fs::metadata(token).map(|m| m.is_file()).unwrap_or(false) {
                            return Some(token.to_string());
                        }
                    }
                }
            }
        }
    }

    for path in [
        "/usr/libexec/polkit-gnome-authentication-agent-1",
        "/usr/libexec/polkit-kde-authentication-agent-1",
        "/usr/libexec/polkit-mate-authentication-agent-1",
        "/usr/bin/lxpolkit",
        "/usr/libexec/xfce-polkit",
    ] {
        if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
            return Some(path.to_string());
        }
    }

    if gnome_shell_polkit_ready() {
        return Some("gnome-shell (built-in polkit)".to_string());
    }

    None
}

pub fn elevation_agent_description() -> Option<String> {
    find_polkit_auth_agent()
}
