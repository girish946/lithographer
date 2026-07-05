//! Attach or allocate a Windows console so `println!` / `eprintln!` reach a terminal.
//!
//! GUI-subsystem executables (and child processes spawned by `cargo tauri dev`) do not
//! inherit stdout/stderr unless we attach to the parent console or allocate one.

#[cfg(windows)]
pub fn ensure() {
    use std::sync::Once;

    static INIT: Once = Once::new();
    INIT.call_once(attach_or_allocate_console);
}

#[cfg(windows)]
fn launch_args_want_console() -> bool {
    if std::env::var_os("LITHOGRAPHER_CONSOLE").is_some_and(|v| !v.is_empty() && v != "0") {
        return true;
    }
    // Elevated relaunch after UAC should not open a stray console; litho runs as a hidden
    // piped child and streams progress into the GUI instead.
    let args: Vec<String> = std::env::args().skip(1).collect();
    !args.iter().any(|arg| arg == "--auto-run")
}

#[cfg(windows)]
fn attach_or_allocate_console() {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::consoleapi::AllocConsole;
    use winapi::um::errhandlingapi::GetLastError;
    use winapi::um::winbase::{STD_ERROR_HANDLE, STD_OUTPUT_HANDLE};
    use winapi::um::wincon::{AttachConsole, SetConsoleTitleW, ATTACH_PARENT_PROCESS};

    if !launch_args_want_console() {
        return;
    }

    let attached = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) != 0 };
    if !attached {
        let want_console = cfg!(debug_assertions)
            || std::env::var_os("LITHOGRAPHER_CONSOLE").is_some_and(|v| !v.is_empty() && v != "0");
        if !want_console {
            return;
        }
        if unsafe { AllocConsole() } == 0 {
            // Already has a console, or allocation failed — still try to wire std handles.
            let _ = unsafe { GetLastError() };
        } else {
            let title: Vec<u16> = OsStr::new("Lithographer")
                .encode_wide()
                .chain(Some(0))
                .collect();
            unsafe {
                SetConsoleTitleW(title.as_ptr());
            }
        }
    }

    reopen_std_handle(STD_OUTPUT_HANDLE);
    reopen_std_handle(STD_ERROR_HANDLE);
}

#[cfg(windows)]
fn reopen_std_handle(std_handle: winapi::shared::minwindef::DWORD) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::fileapi::{CreateFileW, OPEN_EXISTING};
    use winapi::um::handleapi::INVALID_HANDLE_VALUE;
    use winapi::um::processenv::SetStdHandle;
    use winapi::um::winnt::{FILE_ATTRIBUTE_NORMAL, GENERIC_WRITE, FILE_SHARE_WRITE};

    let conout: Vec<u16> = OsStr::new("CONOUT$")
        .encode_wide()
        .chain(Some(0))
        .collect();

    unsafe {
        let handle = CreateFileW(
            conout.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if handle != INVALID_HANDLE_VALUE {
            SetStdHandle(std_handle, handle);
        }
    }
}

#[cfg(not(windows))]
pub fn ensure() {}