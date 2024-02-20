# Lithographer

GUI for [litho](https://github.com/girish946/litho) written in Rust using [tauri](https://tauri.studio/).


<p align="center">
<img src="src/assets/logo.png"></img>
</p>

## Build

### Prerequisites

As of now lithographer only supports building for Linux. You need to have the [Prerequisites for tauri](https://tauri.app/v1/guides/getting-started/prerequisites/) installed on the system.


```bash
$ git clone https://github.com/girish946/lithographer.git
$ cd lithographer
$ cargo tauri build -b appimage
```

## Usage
```bash
$ ./src-tauri/target/release/bundle/appimage/lithographer_0.0.1_amd64.AppImage
```

<img src="src/assets/lithographer-window.png"></img>
