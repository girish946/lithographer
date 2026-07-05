#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

/// Whether the process can perform privileged block I/O without further elevation.
pub fn has_privileged_access() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::is_running_as_root()
    }
    #[cfg(windows)]
    {
        windows::is_elevated()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

/// Short elevation backend label for logs and diagnostics.
pub fn elevation_method() -> String {
    #[cfg(target_os = "linux")]
    {
        linux::elevation_backend_label(linux::elevation_backend()).to_string()
    }
    #[cfg(windows)]
    {
        "uac".to_string()
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        "unsupported".to_string()
    }
}

#[cfg(target_os = "linux")]
pub use linux::build_elevated_litho_command;

/// How litho will be launched for the next privileged operation.
pub fn spawn_mode() -> String {
    if has_privileged_access() {
        "direct".to_string()
    } else {
        #[cfg(windows)]
        {
            "uac-relaunch".to_string()
        }
        #[cfg(target_os = "linux")]
        {
            linux::elevation_backend_label(linux::elevation_backend()).to_string()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            elevation_method()
        }
    }
}

/// OS / desktop environment label for startup diagnostics.
pub fn platform_environment() -> String {
    #[cfg(windows)]
    {
        windows::environment_label()
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .or_else(|_| std::env::var("GDMSESSION"))
            .unwrap_or_else(|_| "Linux".to_string())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        "unknown".to_string()
    }
}

/// Human-readable elevation agent (polkit agent path or gnome-shell hint).
pub fn elevation_agent_description() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        linux::elevation_agent_description()
    }
    #[cfg(windows)]
    {
        Some("UAC (User Account Control)".to_string())
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// Whether privilege escalation can be requested on this machine.
pub fn elevation_ready() -> bool {
    #[cfg(target_os = "linux")]
    {
        linux::polkit_agent_ready()
    }
    #[cfg(windows)]
    {
        true
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        false
    }
}

#[cfg(windows)]
pub use windows::relaunch_elevated_lithographer;
