// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use litho;
use nix::unistd::Uid;
use serde::{Deserialize, Serialize};
use std::process;
use tauri::Window;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceInfo {
    pub device_name: String,
    pub vendor_name: String,
    pub model_name: String,
    pub removable: u8,
    pub size: u64,
}

#[derive(Clone, serde::Serialize)]
struct Progress {
    percentage: f64,
}

const BLOCKS: [u64; 14] = [
    4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608,
    16777216, 33554432,
];

#[tauri::command]
async fn get_root() -> Result<bool, String> {
    if !Uid::effective().is_root() {
        println!("not root");
        Ok(false)
    } else {
        Ok(true)
    }
}

#[tauri::command]
async fn execute(
    operation: String,
    device: String,
    image: String,
    size: u64,
    window: Window,
) -> Result<String, String> {
    println!(
        "operation: {}, device: {}, image: {}, size:{}",
        operation, device, image, size
    );

    let callback = |percentage| {
        // println!("callback progress: {}", percentage);
        match window.emit("percent", Some(Progress { percentage })) {
            Ok(_) => {}
            Err(e) => {
                println!("error occured while emitting progress: {}", e);
            }
        };
    };
    let mut block_size: u64 = 0;
    if size < BLOCKS[0] || size > BLOCKS[13] {
        println!("size is not in the range of 4096 to 33554432");
        return Err("".to_string());
    }
    // calculate the max block size
    for i in 0..BLOCKS.len() {
        if size <= BLOCKS[i] {
            block_size = BLOCKS[i];
            break;
        }
    }

    if operation == "flash".to_string() {
        let _ = litho::flash(image, device, block_size as usize, false, callback);
    } else if operation == "clone".to_string() {
        let _ = litho::clone(device, image, block_size as usize, false, callback);
    }
    Ok("".to_string())
}

fn validate_and_execute(
    operation: String,
    device: String,
    image: String,
) -> Result<String, String> {
    println!(
        "operation: {}, device: {}, image: {}",
        operation, device, image
    );
    if litho::devices::is_removable_device(&device).unwrap() {
        println!("device is removable");
    } else {
        println!("device is not removable");
    }
    Ok("".to_string())
}

#[tauri::command]
fn get_storage_devices() -> Result<Vec<String>, String> {
    litho::devices::get_storage_devices()
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            match app.get_cli_matches() {
                Ok(matches) => {
                    // println!("{:?}", matches);
                    match matches.subcommand {
                        Some(subcommand) => {
                            println!("subcommand found: {:?}", subcommand);
                            subcommand.matches.args.iter().for_each(|(key, value)| {
                                println!("key: {:?}, value: {}", key, value.value);
                            });
                            let operation = subcommand.name;
                            let image = subcommand
                                .matches
                                .args
                                .get("file")
                                .unwrap()
                                .value
                                .as_str()
                                .unwrap()
                                .to_string();
                            // .clone()
                            // .to_string();
                            let device = subcommand
                                .matches
                                .args
                                .get("disk")
                                .unwrap()
                                .value
                                .as_str()
                                .unwrap()
                                .to_string()
                                .clone();

                            let _ = validate_and_execute(operation, device, image);
                            process::exit(0);
                        }
                        None => {
                            // println!("no subcommand found");
                        }
                    }
                }
                Err(e) => {
                    println!("error occured while parsing the cli args: {}", e);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_storage_devices,
            execute,
            get_root
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
