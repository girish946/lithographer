use std::fs;
use std::process::Command;

pub fn is_running_as_root() -> bool {
    if let Ok(output) = Command::new("id").arg("-u").output() {
        let uid = String::from_utf8_lossy(&output.stdout).trim().to_string();
        uid == "0"
    } else {
        false
    }
}

pub fn polkit_agent_ready() -> bool {
    find_polkit_auth_agent().is_some()
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

    None
}