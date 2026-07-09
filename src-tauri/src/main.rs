// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod win_console;

fn main() {
    // Must run before any `println!` so Rust stdio picks up the console handles.
    #[cfg(windows)]
    win_console::ensure();

    #[cfg(target_os = "linux")]
    std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");

    lithographer_lib::run()
}
