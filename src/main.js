const { invoke } = window.__TAURI__.tauri;
const { open, save } = window.__TAURI__.dialog;
const { emit, listen } = window.__TAURI__.event;

var selected_file = "";
var selected_disk = "";
var clone_or_flash = "";
var is_writing = false;

function add_storage_device_names() {
  invoke("get_storage_devices").then((res) => {
    console.log(res);
    var x = document.getElementById("diskSelect");
    res.map(update_disk_options);

    function update_disk_options(value, _index, _array) {
      const device_name = JSON.parse(value); // Parse the JSON string to an object
      console.log(device_name);
      var option = document.createElement("option");
      option.text = device_name.model_name;
      option.value = device_name.device_name;

      if (device_name.removable) {
        option.text = option.text + " (Removable)";
        option.css = "color: green";
        option.color = "green";

      } else {
        option.text = option.text + " (Not Removable)";
        // option.color = "red";
      }
      x.add(option);
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
    selected_file = filename;// .replace(/^.*[\\/]/, '');
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
    const command_line = `litho ${clone_or_flash} -f ${selected_file} -d ${selected_disk} -b 16777216`;
    console.log(command_line);
    // updateProgressBar(100);
    invoke("execute", { operation: clone_or_flash, device: selected_disk, image: selected_file });
    // invoke("test");
    await listen("percent", (event) => {
      console.log(event);
      var status = document.getElementById("status-lebal");
      status.innerHTML = event.payload.percentage + "%";
      updateProgressBar(event.payload.percentage);
    });
  }
}

function select_device_on_click(e) {
  selected_disk = e.target.value;
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
});

