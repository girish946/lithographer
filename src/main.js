const { invoke } = window.__TAURI__.tauri;

function add_storage_device_names() {
  invoke("get_storage_devices").then((res) => {
    console.log(res);
    var x = document.getElementById("diskSelect");
    res.map(myFunction);

    function myFunction(value, index, array) {
      var option = document.createElement("option");
      const device_name = JSON.parse(value); // Parse the JSON string to an object
      console.log(device_name);
      option.text = device_name.model_name;
      option.value = device_name.device_name;
      x.add(option);
    }

  });

}

window.addEventListener("DOMContentLoaded", () => {
  add_storage_device_names();
});
