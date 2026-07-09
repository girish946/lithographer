fn main() {
    let target = std::env::var("TARGET").unwrap_or_else(|_| {
        String::from_utf8_lossy(
            &std::process::Command::new("rustc")
                .arg("--print")
                .arg("host-tuple")
                .output()
                .expect("failed to run rustc --print host-tuple")
                .stdout,
        )
        .trim()
        .to_string()
    });
    println!("cargo:rustc-env=LITHO_TARGET_TRIPLE={target}");
    tauri_build::build()
}
