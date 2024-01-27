// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use litho;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{fs, path::PathBuf};
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceInfo {
    pub device_name: String,
    pub vendor_name: String,
    pub model_name: String,
    pub removable: u8,
}

#[tauri::command]
fn execute(operation: String, device: String, image: String) -> Result<String, String> {
    println!(
        "operation: {}, device: {}, image: {}",
        operation, device, image
    );
    if operation == "flash".to_string() {
        let _ = litho::flash(image, device, 4096, false);
    } else if operation == "clone".to_string() {
        let _ = litho::clone(device, image, 4096, false);
    }
    Ok("".to_string())
}

/// function to read the file contents as a string
fn get_file_content(input_file: String) -> Result<String, Box<dyn std::error::Error>> {
    let content = match fs::read_to_string(input_file.clone()) {
        Ok(s) => s,
        Err(e) => {
            format!(
                "error coccured while reading :{}: {}",
                input_file.clone(),
                e.to_string()
            );
            "".to_string()
        }
    };

    Ok(content)
}

#[tauri::command]
fn get_storage_devices() -> Result<Vec<String>, String> {
    let paths = match fs::read_dir("/sys/block/") {
        Ok(paths) => paths,
        Err(e) => {
            println!("could not get the subdirs of /sys/block/: {}", e);
            return Err("".to_string());
        }
    };
    let mut devices: Vec<String> = Vec::new();

    for path in paths {
        let p = match path {
            Ok(p) => p,
            Err(e) => {
                println!("could not get the path: {}", e);
                return Err("".to_string());
            }
        };
        let mut dev = p.path().clone();
        let device_end_name = match p.path().clone().file_name() {
            Some(device) => match device.to_str() {
                Some(dev) => dev.to_string(),
                None => {
                    println!("could not get the device name");
                    "".to_string()
                }
            },

            None => {
                println!("could not get the device name ");
                "".to_string()
            }
        };

        dev.push("device");
        if dev.exists() {
            dev.push("vendor");
            let vendor_name_file = match dev.clone().into_os_string().to_str() {
                Some(name) => name.to_string(),
                None => "".to_string(),
            };

            let mut dev_vendor_name = "".to_string();
            if !vendor_name_file.is_empty() {
                dev_vendor_name = match get_file_content(vendor_name_file) {
                    Ok(name) => name.replace("\n", ""),
                    Err(e) => {
                        println!("error occured while reading vendor name: {}", e);
                        "".to_string()
                    }
                };
            }
            dev.pop();
            dev.push("model");
            let model_name_file = match dev.clone().into_os_string().to_str() {
                Some(name) => name.to_string(),
                None => "".to_string(),
            };

            let model_name = match get_file_content(model_name_file) {
                Ok(model) => model.replace("\n", ""),
                Err(e) => {
                    println!("error occured while reading model name: {}", e);
                    "".to_string()
                }
            };
            dev.pop();
            dev.pop();
            dev.push("removable");
            let mut removable = "".to_string();
            if dev.exists() {
                removable = match fs::read_to_string(dev.clone()) {
                    Ok(removable) => removable,
                    Err(e) => {
                        println!("error occured while reading removable: {}", e);
                        "".to_string()
                    }
                }
                // println!("removable: {}", removable);
            }

            if !device_end_name.is_empty() {
                let mut dev_path = PathBuf::from("/dev/");
                dev_path.push(device_end_name);
                if dev_path.exists() {
                    if removable == "1\n" {
                        let dev_info = DeviceInfo {
                            device_name: dev_path.display().to_string(),
                            vendor_name: dev_vendor_name,
                            model_name,
                            removable: 1,
                        };
                        let device_json = json!(dev_info).to_string();
                        devices.push(device_json);
                    } else if removable == "0\n" {
                        let dev_info = DeviceInfo {
                            device_name: dev_path.display().to_string(),
                            vendor_name: dev_vendor_name,
                            model_name,
                            removable: 0,
                        };
                        let dev_json = json!(dev_info).to_string();
                        devices.push(dev_json);
                    }
                }
            }
        }
    }
    return Ok(devices);
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_storage_devices, execute])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
