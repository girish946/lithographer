const { invoke } = window.__TAURI__.tauri;


window.addEventListener("DOMContentLoaded", () => {
  invoke("get_storage_devices").then((res) => {
    console.log(res);
  });
});
