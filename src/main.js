const { invoke } = window.__TAURI__.tauri;
const { open, save } = window.__TAURI__.dialog;
const { emit, listen } = window.__TAURI__.event;

var selected_file = "";
var selected_disk = "";
var clone_or_flash = "";
var target_size = 0;
var is_writing = false;

var removable_devices = [];
var non_removable_devices = [];

function add_storage_device_names() {
  invoke("get_storage_devices").then((res) => {
    console.log(res);
    var diskSelect = document.getElementById("diskSelect");
    for (var i = 0; i < res.length; i++) {
      console.log(res[i]);
      var value = res[i];
      const device_name = JSON.parse(value); // Parse the JSON string to an object
      console.log(device_name);
      var option = document.createElement("option");

      option.text = device_name.model_name;
      option.value = value;

      if (device_name.removable) {
        option.text = option.text + " (Removable)";
        removable_devices.push(option);
      } else {
        option.text = option.text + " (Not Removable)";
        non_removable_devices.push(option);
      }
    }

    for (var i = 0; i < removable_devices.length; i++) {
      diskSelect.appendChild(removable_devices[i]);
    }
    for (var i = 0; i < non_removable_devices.length; i++) {
      diskSelect.appendChild(non_removable_devices[i]);
    }
    diskSelect.selectedIndex = 0;
  });
}

function get_root() {
  invoke("get_root").then((res) => {
    console.log(res);
    var status = document.getElementById("status-lebal");
    if (res) {
      status.innerHTML = "Running as root";
    } else {
      status.innerHTML = "Not running as root";
    }
  });
}

function write_file_on_click(e) {
  console.log(e);
  clone_or_flash = "flash";
  open({
    multiple: false,
  }).then((res) => {
    console.log(res);
    var filename = res;
    selected_file = filename;
    var status = document.getElementById("status-lebal");
    status.innerHTML = `File selected: ${selected_file}`;
  });
}

function clone_disk_on_click(e) {
  console.log(e);
  clone_or_flash = "clone";
  save({
    multiple: false,
  }).then((res) => {
    console.log(res);
    selected_file = res;
  });
}

async function start_process_on_click(e) {
  if (!is_writing) {
    is_writing = true;
    console.log(e);
    const command_line = `litho ${clone_or_flash} -f ${selected_file} -d ${selected_disk} -b ${target_size}`;//16777216`;
    console.log(command_line);

    invoke("execute", { operation: clone_or_flash, device: selected_disk, image: selected_file, size: target_size });

    await listen("percent", (event) => {
      console.log(event);
      var status = document.getElementById("status-lebal");
      status.innerHTML = event.payload.percentage + "%";
      updateProgressBar(event.payload.percentage);
      if (percent == 100) {
        is_writing = false;
      }
    });
  }
}

function select_device_on_click(e) {
  if (e.target.value == "default") {
    return;
  }
  device_details = JSON.parse(e.target.value);
  selected_disk = device_details.device_name;
  target_size = device_details.size;
  console.log("selested device:" + device_details);
  console.log(selected_disk);
}


function updateProgressBar(percentage) {

  const progressBarFill = document.getElementById('progress');
  progressBarFill.value = percentage;
}

window.addEventListener("DOMContentLoaded", () => {
  add_storage_device_names();
  var flast_element = document.getElementById("writeFile");
  flast_element.addEventListener("click", write_file_on_click);

  var clocne_element = document.getElementById("cloneDiskToFile");
  clocne_element.addEventListener("click", clone_disk_on_click);

  var start_elelment = document.getElementById("startProcess");
  start_elelment.addEventListener("click", start_process_on_click);

  var disk_select_element = document.getElementById("diskSelect");
  disk_select_element.addEventListener("change", select_device_on_click);

  get_root();
});

