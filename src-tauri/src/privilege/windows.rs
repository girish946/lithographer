use std::ffi::OsStr;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use winapi::shared::minwindef::DWORD;
use winapi::shared::winerror::ERROR_CANCELLED;
use winapi::um::handleapi::CloseHandle;
use winapi::um::processthreadsapi::{GetCurrentProcess, GetExitCodeProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::shellapi::{
    ShellExecuteExW, ShellExecuteW, SHELLEXECUTEINFOW, SEE_MASK_NOCLOSEPROCESS,
};
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::INFINITE;
use winapi::um::winnt::{TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use winapi::um::winuser::SW_SHOW;

const UAC_CANCELLED_EXIT: i32 = 1223;

/// Human-readable Windows version for startup diagnostics.
pub fn environment_label() -> String {
    use winapi::um::sysinfoapi::GetVersionExW;
    use winapi::um::winnt::OSVERSIONINFOEXW;

    let mut info = OSVERSIONINFOEXW {
        dwOSVersionInfoSize: mem::size_of::<OSVERSIONINFOEXW>() as u32,
        dwMajorVersion: 0,
        dwMinorVersion: 0,
        dwBuildNumber: 0,
        dwPlatformId: 0,
        szCSDVersion: [0; 128],
        wServicePackMajor: 0,
        wServicePackMinor: 0,
        wSuiteMask: 0,
        wProductType: 0,
        wReserved: 0,
    };

    let arch = std::env::consts::ARCH;
    let ok = unsafe { GetVersionExW(&mut info as *mut _ as *mut _) };
    if ok == 0 {
        return format!("Windows ({arch})");
    }

    let product = if info.dwMajorVersion >= 10 && info.dwBuildNumber >= 22000 {
        "Windows 11"
    } else if info.dwMajorVersion >= 10 {
        "Windows 10"
    } else {
        "Windows"
    };

    format!(
        "{product} {major}.{minor} (build {build}, {arch})",
        major = info.dwMajorVersion,
        minor = info.dwMinorVersion,
        build = info.dwBuildNumber
    )
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION {
            TokenIsElevated: 0,
        };
        let mut size = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );
        CloseHandle(token);

        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Spawn an elevated Lithographer child via UAC with the same flash/clone selections.
/// The unprivileged caller should exit after `Ok(())` so only the elevated window remains.
pub fn relaunch_elevated_lithographer(args: &[String]) -> Result<(), String> {
    if is_elevated() {
        return Err("Already running with administrator privileges.".into());
    }

    let exe = std::env::current_exe().map_err(|e| format!("Could not resolve executable: {e}"))?;
    let parameters = windows_argument_string(args);

    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(&parameters);

    let status = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            ptr::null(),
            SW_SHOW,
        )
    };

    if (status as isize) <= 32 {
        if (status as isize) == 1223 {
            return Err(
                "UAC elevation was cancelled or denied. Flash/clone requires administrator access."
                    .into(),
            );
        }
        return Err(format!(
            "UAC elevation failed (ShellExecute code {}). Try running Lithographer as administrator.",
            status as isize
        ));
    }

    Ok(())
}

/// Launch litho elevated via UAC and wait (no stdout capture). Prefer
/// `relaunch_elevated_lithographer` so litho runs as a piped child instead.
#[allow(dead_code)]
pub fn spawn_elevated_litho_and_wait(litho_path: &Path, args: &[String]) -> Result<Option<i32>, String> {
    let verb = wide("runas");
    let file = wide(&litho_path.to_string_lossy());
    let parameters = wide(&windows_argument_string(args));

    let mut info = unsafe { mem::zeroed::<SHELLEXECUTEINFOW>() };
    info.cbSize = mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = parameters.as_ptr();
    info.nShow = SW_SHOW;

    let ok = unsafe { ShellExecuteExW(&mut info) };
    if ok == 0 {
        let code = unsafe { winapi::um::errhandlingapi::GetLastError() };
        if code == ERROR_CANCELLED {
            return Ok(Some(UAC_CANCELLED_EXIT));
        }
        return Err(format!(
            "UAC elevation failed (Win32 error {code}). Try running Lithographer as administrator."
        ));
    }

    if info.hProcess.is_null() {
        return Err("UAC elevation succeeded but no process handle was returned.".into());
    }

    let wait = unsafe { WaitForSingleObject(info.hProcess, INFINITE) };
    if wait != 0 {
        unsafe {
            CloseHandle(info.hProcess);
        }
        return Err(format!("Failed waiting for elevated litho process (code {wait})."));
    }

    let mut exit_code: DWORD = 0;
    let got_code = unsafe { GetExitCodeProcess(info.hProcess, &mut exit_code) };
    unsafe {
        CloseHandle(info.hProcess);
    }

    if got_code == 0 {
        return Err("Failed to read litho exit code.".into());
    }

    Ok(Some(exit_code as i32))
}

fn windows_argument_string(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if needs_windows_quoting(arg) {
                format!("\"{}\"", arg.replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn needs_windows_quoting(arg: &str) -> bool {
    arg.is_empty()
        || arg.chars().any(|c| c.is_whitespace() || c == '"')
        || arg.contains('\\')
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}