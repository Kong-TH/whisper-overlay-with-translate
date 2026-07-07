[Installation and Usage](#-installation-and-usage)

## 💬 whisper-overlay

A wayland overlay providing speech-to-text functionality for any application via a global push-to-talk hotkey.
Anything you are saying while holding the hotkey will be transcribed in real-time and shown on-screen.
The live transcriptions use a faster but less accurate model but as soon as you pause speaking or release
the hotkey, the transcription will be updated using a second, more accurate model.
This resulting text will then be tryped into the window that is currently focused.

- On-screen, realtime live transcriptions via CUDA and faster-whisper
- The server-client based architecture allows you to host the model on another machine
- Native waybar integration for status display
- Utilizes `layer-shell` and `virtual-keyboard-v1` to support most wayland compositors

This makes use of the [RealtimeSTT](https://github.com/KoljaB/RealtimeSTT) python library to provide
live transcriptions, which in turn uses [faster-whisper](https://github.com/SYSTRAN/faster-whisper)
for both the actual realtime and high-fidelity transcription model.

Requirements:

- A wayland compositor (sway, hyprland, ...)
- Rust toolchain with `cargo` for building the overlay client
- Native development packages for GTK 4, gtk4-layer-shell, libevdev, and audio input
- A GPU with CUDA support is highly recommended, otherwise translation will have a significantly latency even
  on a modern CPU (1 second latency for live transcription and ~5 seconds for the result)

## 🚀 Quick Start

This is the shortest path for a local checkout. See [Installation](#-installation)
for distro packages, container targets, GPU setup, and manual server setup.

```bash
git clone https://github.com/oddlama/whisper-overlay
cd whisper-overlay

# Start the server. The compose default is CPU-only for broad compatibility.
docker compose up --build
# or:
# podman compose up --build

# Build and install the overlay client from this checkout.
# If cargo or native headers are missing, install the client dependencies below first.
cargo install --path .
whisper-overlay overlay
```

Use the settings UI for common client/server preferences:

```bash
whisper-overlay settings
```

Now press and hold <kbd>Right Ctrl</kbd> to transcribe. For a permanent installation
I recommend starting the server as a systemd service and adding the `whisper-overlay overlay`
as a startup command to your desktop environment / compositor.

Settings are stored in `$XDG_CONFIG_HOME/whisper-overlay/config.toml` or
`~/.config/whisper-overlay/config.toml` when `XDG_CONFIG_HOME` is unset.

## ⚙️ Usage

In principle you just need to start `./realtime-stt-server.py` and it will be listening for requests on `localhost:7007`.
You can then start `whisper-overlay overlay` to transcribe text. The default hotkey is <kbd>Right Ctrl</kbd>,
but you can change this by specifying any name from [evdev::Key](https://docs.rs/evdev/latest/evdev/struct.Key.html),
for example `KEY_F12` for <kbd>F12</kbd>. Beware that the hotkey is only observed and will still be passed to the application that is focused.

#### Server (realtime-stt-server)

If you want to change the server settings, it comes with the following options:

```bash
> realtime-stt-server.py --help
usage: realtime-stt-server.py [-h] [--host HOST] [--port PORT] [--backend {realtime-stt,onnx}] [--device DEVICE] [--model MODEL]
                              [--model-realtime MODEL_REALTIME] [--language LANGUAGE] [--task {transcribe,translate}]
                              [--target-language TARGET_LANGUAGE] [--onnx-model ONNX_MODEL]
                              [--onnx-provider {auto,cpu,cuda,tensorrt,rocm,openvino}] [--onnx-device ONNX_DEVICE]
                              [--compute-type {auto,fp32,fp16,int8}] [--onnx-export] [--debug]

options:
  -h, --help            show this help message and exit
  --host HOST           The host to listen on [default: 'localhost']
  --port PORT           The port to listen on [default: 7007]
  --backend {realtime-stt,onnx}
                        The transcription backend to use [default: 'realtime-stt']
  --device DEVICE       Device to run the models on, defaults to cuda if available, else cpu [default: 'cuda']
  --model MODEL         Main model used to generate the final transcription [default: 'large-v3']
  --model-realtime MODEL_REALTIME
                        Faster model used to generate live transcriptions [default: 'base']
  --language LANGUAGE   Set the spoken language. Leave empty to auto-detect. [default: '']
  --task {transcribe,translate}
                        Whether to transcribe or translate speech when supported [default: 'transcribe']
  --target-language TARGET_LANGUAGE
                        Target language for translation-capable backends. Leave empty for backend default [default: '']
  --onnx-model ONNX_MODEL
                        ONNX Whisper model path or Hugging Face model id [default: 'optimum/whisper-tiny.en']
  --onnx-provider {auto,cpu,cuda,tensorrt,rocm,openvino}
                        ONNX Runtime provider preset [default: 'auto']
  --onnx-device ONNX_DEVICE
                        Reserved ONNX device selector for future provider-specific options [default: 'auto']
  --compute-type {auto,fp32,fp16,int8}
                        Reserved ONNX compute type selector [default: 'auto']
  --onnx-export         Export a Transformers checkpoint to ONNX when loading with Optimum [default: unset]
  --debug               Enable debug log output [default: unset]
```

The `onnx` backend is an experimental final-result backend. It buffers the
current utterance in memory and transcribes or translates it when the hotkey is released.
Install the optional runtime dependencies only when using it:

```bash
# CPU
pip install "optimum[onnxruntime]" transformers numpy onnxruntime

# NVIDIA GPU
pip install "optimum[onnxruntime-gpu]" transformers numpy onnxruntime-gpu
```

See [Backend and Hardware Matrix](./docs/backend-matrix.md) for CPU, CUDA,
ONNX, Docker, Podman, and other container target guidance.
See [Benchmarking](./docs/benchmark.md) for backend comparison methodology.

#### Client (whisper-overlay)

The actual overlay can also be customized, for example by providing your own gtk style
(refer to [the builtin style.css](./src/style.css) as a reference), or by changing the hotkey.
It has the following options:

```bash
> whisper-overlay overlay --help
Usage: whisper-overlay overlay [OPTIONS]

Options:
  -a, --address <ADDRESS>
          The address of the the whisper streaming instance (host:port) [default: localhost:7007]
  -s, --style <STYLE>
          An optional stylesheet for the overlay, which replaces the internal style
      --hotkey <HOTKEY>
          Specifies the hotkey to activate voice input. You can use any key or button name from [evdev::Key](https://docs.rs/evdev/latest/evdev/struct.Key.html) [default: KEY_RIGHTCTRL]
      --audio-source-kind <AUDIO_SOURCE_KIND>
          Audio source kind to capture. Desktop/application capture depends on host audio backend support [default: microphone]
      --audio-source <AUDIO_SOURCE>
          Audio source id or device name. Use `whisper-overlay audio-sources` to list visible sources [default: default]
      --capture-mode <CAPTURE_MODE>
          Capture mode: push-to-talk, toggle-live-caption, or always-on-live-caption [default: push-to-talk]
      --type-into-focused-app
          Type final captions into the focused app. Defaults to dictation-only behavior
      --no-text-injection
          Disable text injection even in push-to-talk dictation mode
      --caption-finalize-interval <CAPTION_FINALIZE_INTERVAL>
          Seconds between finalization passes in live-caption modes. Set 0 to disable [default: 6]
  -h, --help               Print help
```

List audio sources visible to the host audio backend:

```bash
whisper-overlay audio-sources
```

Probe visible sources to see which one is currently carrying real audio:

```bash
whisper-overlay audio-sources --probe
```

Sources with `peak_rms` close to `0.0000` are effectively silent during the
probe window. Start playback first, then run the probe again to find the monitor
source that carries desktop audio.

Record the exact source path that the overlay would use before debugging model
accuracy or caption timing:

```bash
whisper-overlay record-audio \
  --audio-source-kind desktop-output \
  --audio-source "pulse:easyeffects_sink.monitor" \
  --seconds 30 \
  --output capture.wav
```

The diagnostic file is written as 16 kHz mono PCM WAV, matching the stream sent
to the speech server. Listen to this file first. If it is muffled, delayed,
silent, or contains the wrong application, fix the selected audio source before
tuning models or caption timing.

The default capture mode is microphone push-to-talk dictation. The settings UI
shows user-facing audio choices such as Microphone, Desktop audio, Speaker/output,
and Application audio, then filters the device dropdown for that choice.
Live-caption modes keep the overlay active for captions and do not type into the
focused app unless `--type-into-focused-app` is passed.

```bash
# Existing behavior: hold the hotkey and dictate from the default microphone.
whisper-overlay overlay

# Toggle live captions from the default microphone with the hotkey.
whisper-overlay overlay --capture-mode toggle-live-caption

# Try desktop/system audio when the audio backend exposes a monitor source.
whisper-overlay overlay \
  --audio-source-kind desktop-output \
  --audio-source default \
  --capture-mode toggle-live-caption

# Select a specific visible source by id/name from `audio-sources`.
whisper-overlay overlay \
  --audio-source-kind desktop-output \
  --audio-source "pulse:easyeffects_sink.monitor" \
  --capture-mode always-on-live-caption
```

Live-caption modes periodically ask the server to finalize the current segment
and continue listening. This gives the slower, more accurate final model a chance
to correct the fast realtime preview:

```bash
whisper-overlay overlay \
  --audio-source-kind desktop-output \
  --audio-source "pulse:easyeffects_sink.monitor" \
  --capture-mode always-on-live-caption \
  --caption-finalize-interval 6
```

Lower intervals produce updates sooner but may cut sentences too aggressively.
Higher intervals preserve longer context but increase delay. Set
`--caption-finalize-interval 0` to use realtime preview only.

Desktop and per-application capture depend on the host audio stack. PipeWire or
PulseAudio monitor sources are the most portable first path on modern Linux
desktops. If `audio-sources` does not show a monitor/source for the sound you
want, expose a monitor/loopback source in the system audio settings first.

Some ALSA/PipeWire setups print ALSA errors while probing devices, for example
`unable to open slave` or `Found no matching channel map`. These messages are
not always fatal. If the overlay later prints `Input device: ...`, audio capture
opened successfully. If live caption repeatedly outputs a short phrase such as
`thank you` while no useful audio is playing, it is usually a silence/hallucination
case or the wrong source was selected. List sources again and choose a real
monitor source; the client also drops very quiet PCM chunks before sending audio
to the server.

## 📦 Installation

The project has two runtime pieces:

- `realtime-stt-server.py`: Python speech-to-text server, usually run in a container.
- `whisper-overlay`: Rust/GTK Wayland client, installed locally with Cargo.

### 1. Install Client Build Dependencies

The overlay client is a native Rust/GTK application, so `cargo install --path .`
requires the Rust toolchain and system development headers.

On openSUSE:

```bash
sudo zypper install rustup gcc pkg-config gtk4-devel gtk4-layer-shell-devel libevdev-devel alsa-devel
rustup default stable
export PATH="$HOME/.cargo/bin:$PATH"
```

Add the Cargo binary directory to your shell startup file if `whisper-overlay`
is not found after `cargo install --path .`. Some distribution-packaged
`rustup` installs do not create `$HOME/.cargo/env`, so adding
`$HOME/.cargo/bin` directly is the most portable option:

```bash
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

Verify the toolchain before building:

```bash
cargo --version
rustc --version
```

On other distributions, install the equivalent packages for:

- Rust and Cargo
- C compiler toolchain
- `pkg-config`
- GTK 4 development headers
- gtk4-layer-shell development headers
- libevdev development headers
- ALSA development headers

### 2. Start The Server With Containers

The default compose setup builds the CPU RealtimeSTT image and starts the server
on `0.0.0.0:7007`. CPU is slower, but it avoids requiring NVIDIA container
runtime setup on first launch.

Docker Compose:

```bash
docker compose up --build
# or, on older systems:
docker-compose up --build
```

Podman Compose:

```bash
podman compose up --build
# or, on older systems:
podman-compose up --build
```

The compose file supports multiple Dockerfile targets through
`WHISPER_OVERLAY_DOCKER_TARGET`:

| Target | Backend | Use case |
| --- | --- | --- |
| `cpu` | RealtimeSTT | CPU-compatible default |
| `gpu` | RealtimeSTT | NVIDIA CUDA runtime |
| `onnx-cpu` | ONNX | Experimental CPU ONNX backend |
| `onnx-gpu` | ONNX | Experimental GPU ONNX backend |

For NVIDIA CUDA:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=gpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --device cuda" \
docker compose up --build
```

For Podman GPU, the host must expose NVIDIA devices through CDI/NVIDIA
Container Toolkit. If `podman compose` does not expose the GPU, use direct
`podman run --device nvidia.com/gpu=all` as shown in
[Backend and Hardware Matrix](./docs/backend-matrix.md).

Check whether Podman can see the GPU before starting the server:

```bash
podman run --rm --device nvidia.com/gpu=all --security-opt=label=disable docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04 nvidia-smi
```

If that prints the GPU table, start the CUDA server with the Podman GPU
override:

```bash
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --device cuda --model medium --model-realtime small" \
podman compose -f docker-compose.yml -f docker-compose.podman-gpu.yml up --build
```

The server is using CUDA when startup logs include `device=cuda`, for example:

```text
INFO RealtimeSTT settings: device=cuda model=medium realtime_model=small language=auto
INFO AudioToTextRecorder ready
INFO Server ready to accept connections
```

During the first model download, Hugging Face may warn about unauthenticated
requests. Set `HF_TOKEN` in the server environment if you need higher rate
limits or more reliable model downloads.

If the test command cannot find `nvidia.com/gpu=all`, install/configure NVIDIA
Container Toolkit for Podman and generate NVIDIA CDI devices, for example:

```bash
sudo nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml
```

On openSUSE, install the NVIDIA Container Toolkit from NVIDIA's CUDA repository
first when `nvidia-ctk` is not available:

```bash
sudo zypper addrepo https://developer.download.nvidia.com/compute/cuda/repos/suse16/x86_64/ cuda
sudo zypper refresh
sudo zypper install -y nvidia-container-toolkit
```

The package normally regenerates the CDI specification during installation. If
the Podman GPU test still fails, regenerate it manually and verify that the CDI
device exists:

```bash
sudo nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml
nvidia-ctk cdi list
podman run --rm --device nvidia.com/gpu=all docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04 nvidia-smi
```

If the test prints `Failed to initialize NVML: Insufficient Permissions`, retry
with `--security-opt=label=disable`. The included
`docker-compose.podman-gpu.yml` override already sets this option:

```bash
podman run --rm --device nvidia.com/gpu=all --security-opt=label=disable docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04 nvidia-smi
```

For ONNX CPU:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-cpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --backend onnx --onnx-provider cpu" \
docker compose up --build
```

See [Backend and Hardware Matrix](./docs/backend-matrix.md) for direct
`docker`, `podman`, `nerdctl`, GPU, ONNX, and troubleshooting commands.

### 3. Install The Overlay Client

After the server is running, build and install the Rust client from this
checkout:

```bash
cargo install --path .
whisper-overlay overlay
```

If installation succeeds but the shell prints `whisper-overlay: command not
found`, refresh the current shell PATH:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
hash -r
whisper-overlay overlay
```

Use a different hotkey if needed:

```bash
whisper-overlay overlay --hotkey KEY_F12
```

Open the settings UI:

```bash
whisper-overlay settings
```

<details>
<summary>

### ❄️ NixOS
</summary>

This application comes with a NixOS module and overlay so you can easily access the relevant packages
and host the realtime-stt-server. First, add this flake as an input:

```nix
{
  inputs = {
    # ...
    whisper-overlay.url = "github:oddlama/whisper-overlay";
    whisper-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Then add the nixos module exposed by this flake,
and enable the realtime-stt-server in your `configuration.nix`. Also add the relevant package to your system or user,
so you can start it later.

```nix
{
  imports = [
    inputs.whisper-overlay.nixosModules.default
  ];

  # Also make sure to enable cuda support in nixpkgs, otherwise transcription will
  # be painfully slow. But be prepared to let your computer build packages for 2-3 hours.
  nixpkgs.config.cudaSupport = true;

  services.realtime-stt-server.enable = true;
  environment.systemPackages = [pkgs.whisper-overlay];
}
```

The server will now be started automatically with your system,
and you can run `whisper-overlay overlay` as your user.
You might want to add this.

</details>
<details>
<summary>

### 🧰 Manually
</summary>

Manual setup is useful when developing the Python server without containers.
For general use, the container setup above is more reproducible.

```bash
# Create virtualenv
python -m venv venv
source venv/bin/activate

# Install RealtimeSTT
git clone https://github.com/oddlama/RealtimeSTT
cd RealtimeSTT
pip install -r requirements.txt
cd ..

# Run server script
python ./realtime-stt-server.py --device cpu
```

Then build and run the client:

```bash
cargo build --release
./target/release/whisper-overlay overlay
```

</details>

## 🌟 Waybar integration

The whisper-overlay natively supports a waybar status command to
display the server status in your waybar.

Add this to your waybar config:

```jsonc
"custom/whisper_overlay": {
    "escape": true,
    "exec": "/path/to/whisper-overlay waybar-status",
    "format": "{icon} {}",
    "format-icons": {
        "disconnected": "<span foreground='gray'></span>",
        "connected": "<span foreground='#4ab0fa'></span>",
        "connected-active": "<span foreground='red'></span>"
    },
    "return-type": "json",
    "tooltip": true
},
```

And instanciate the module somewhere:

```jsonc
"modules-left": [
    // ...
    "custom/whisper_overlay"
    // ...
],
```

## ❌ Limitations

#### Requires RealtimeSTT fork

Currently, you need to use my fork of [RealtimeSTT](https://github.com/oddlama/RealtimeSTT) which allows the client
to read token probabilities and fixes some shutdown issues. Already requested this to be upstreamed,
so hopefully this won't be required for long.

#### Single active client

The provided `realtime-stt-server` implementation allows you to host the server either locally on your machine, or on another machine
in your network. Our end of the implementation is techincally ready for multiple clients, but due to the way `RealtimeSTT` works, it cannot process
multiple requests simultaneously at this point in time. So you will have to wait for other clients to disconnect before your transcription can begin.

#### Wayland only

Currently, this project _requires_ the use of a wayland compositor that supports the layer-shell and virtual-keyboard-v1 protocol extensions.
Thus it should work out-of-the-box on any wlroots based compositor (sway, ...) and on hyprland. X11 support is currently not planned.
There is a branch with a partial implementation for X11, but getting GTK4 to create a reliable overlay window has proven to be hard and
auto-type doesn't work properly with enigo (the rust library in use for virtual input). But I'm of course happy to accept contributions
in that regard if someone knows how to address the remaining issues.

#### Global hotkeys via evdev

The global hotkey is detected using `evdev`, since I didn't manage to get the GlobalShortcuts desktop portal
to work with windows using the layer-shell protocol ([related issue](https://github.com/bilelmoussaoui/ashpd/issues/213)).
In the future this might change, but for now your user must be in the `input` group for this to work.

## 📜 License

Licensed under the MIT license ([LICENSE](LICENSE) or <https://opensource.org/licenses/MIT>).
Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project by you, shall be licensed as above, without any additional terms or conditions.
