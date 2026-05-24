# Backend and Hardware Matrix

This document summarizes the supported server backends and practical setup paths.

## Quick Matrix

| Environment | Backend | Docker target | Provider option | Notes |
| --- | --- | --- | --- | --- |
| CPU-only | `realtime-stt` | `cpu` | `--device cpu` | Best compatibility fallback. Latency depends heavily on model size. |
| NVIDIA CUDA | `realtime-stt` | `gpu` | `--device cuda` | Strongest baseline for realtime transcription. Requires NVIDIA container runtime/CDI setup. |
| ONNX CPU | `onnx` | `onnx-cpu` | `--onnx-provider cpu` | Experimental final-result backend. No realtime partial output yet. |
| ONNX auto | `onnx` | `onnx-cpu` or `onnx-gpu` | `--onnx-provider auto` | Selects the best available ONNX Runtime provider and falls back to CPU. |
| ONNX NVIDIA | `onnx` | `onnx-gpu` | `--onnx-provider cuda` | Requires `onnxruntime-gpu` and compatible NVIDIA runtime libraries. |
| ONNX TensorRT | `onnx` | `onnx-gpu` | `--onnx-provider tensorrt` | Experimental. Provider availability depends on the installed ONNX Runtime build. |
| ONNX OpenVINO | `onnx` | custom | `--onnx-provider openvino` | Planned packaging path for Intel hardware. |
| ONNX ROCm | `onnx` | custom | `--onnx-provider rocm` | Planned packaging path for AMD GPU environments. |

## Container Targets

Build a GPU RealtimeSTT server:

```bash
docker build --target gpu -t realtime-stt-server .
```

The Dockerfile uses fully qualified base images so Podman does not need to
prompt for registry selection:

- `docker.io/library/python:3.11-slim-bookworm`
- `docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04`

Build a CPU RealtimeSTT server:

```bash
docker build --target cpu -t realtime-stt-server .
```

Build an ONNX CPU server:

```bash
docker build --target onnx-cpu -t realtime-stt-server .
```

Build an ONNX GPU server:

```bash
docker build --target onnx-gpu -t realtime-stt-server .
```

The same targets work with Podman:

```bash
podman build --target gpu -t realtime-stt-server .
podman build --target cpu -t realtime-stt-server .
podman build --target onnx-cpu -t realtime-stt-server .
podman build --target onnx-gpu -t realtime-stt-server .
```

They also work with nerdctl:

```bash
nerdctl build --target gpu -t realtime-stt-server .
nerdctl build --target cpu -t realtime-stt-server .
nerdctl build --target onnx-cpu -t realtime-stt-server .
nerdctl build --target onnx-gpu -t realtime-stt-server .
```

## Compose

Docker Compose:

```bash
docker-compose up --build
# or:
docker compose up --build
```

Podman Compose:

```bash
podman compose up --build
# or, on older systems:
podman-compose up --build
```

The compose default uses the CPU target and starts the server with
`--device cpu`. This gives first-time users a working container without NVIDIA
runtime configuration. To use the CUDA image, select the GPU target and pass
`--device cuda`. Docker Compose also needs GPU access from the NVIDIA container
runtime, for example with a local compose override that adds a GPU reservation:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=gpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --device cuda" \
docker compose up --build
```

For Podman, `podman compose` support for GPU device reservations depends on the
host CDI/NVIDIA Container Toolkit setup and the compose provider. If GPU devices
are not visible in compose, use direct `podman run --device nvidia.com/gpu=all`
from the section below.

```bash
WHISPER_OVERLAY_DOCKER_TARGET=gpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --device cuda" \
podman compose up --build
```

The repository also includes a Podman CDI override that passes the GPU device
into the service:

```bash
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --device cuda --model medium --model-realtime small" \
podman compose -f docker-compose.yml -f docker-compose.podman-gpu.yml up --build
```

Use `medium` plus `small` as a conservative first CUDA profile for 8 GB GPUs.
If it is stable, try a larger final model; if the container is killed or CUDA
runs out of memory, move back down to `small` or `base`.

Confirm CUDA usage from the server startup log:

```text
INFO RealtimeSTT settings: device=cuda model=medium realtime_model=small language=auto
INFO AudioToTextRecorder ready
INFO Server ready to accept connections
```

nerdctl Compose:

```bash
nerdctl compose up --build
```

The compose file can select a target with an environment variable:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-cpu docker-compose up --build
```

For Podman:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-cpu podman compose up --build
```

For nerdctl:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-cpu nerdctl compose up --build
```

Override the server command when needed:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-cpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --backend onnx --onnx-provider cpu" \
docker-compose up --build
```

The compose file runs the command through `sh -c` so overrides with spaces work
with Docker Compose and Podman Compose.

For NVIDIA GPU ONNX:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-gpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --backend onnx --onnx-provider auto" \
docker-compose up --build
```

## Direct Container Run

Docker:

```bash
docker run --rm -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server \
  python3 realtime-stt-server.py --host 0.0.0.0 --device cpu
```

Podman:

```bash
podman run --rm -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server \
  python3 realtime-stt-server.py --host 0.0.0.0 --device cpu
```

nerdctl:

```bash
nerdctl run --rm -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server \
  python3 realtime-stt-server.py --host 0.0.0.0 --device cpu
```

For NVIDIA GPU containers, Docker usually needs the NVIDIA container runtime:

```bash
docker run --rm --gpus all -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server \
  python3 realtime-stt-server.py --host 0.0.0.0 --device cuda
```

Podman GPU setup depends on the host NVIDIA Container Toolkit/CDI configuration.
When CDI is configured, a typical command is:

```bash
podman run --rm --device nvidia.com/gpu=all -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server \
  python3 realtime-stt-server.py --host 0.0.0.0 --device cuda
```

Before running the project image, test GPU visibility with the NVIDIA CUDA
runtime image:

```bash
podman run --rm --device nvidia.com/gpu=all --security-opt=label=disable docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04 nvidia-smi
```

If that command fails before printing the GPU table, the host driver may be
working but Podman still does not have a CDI device. Generate the CDI definition
after installing NVIDIA Container Toolkit:

```bash
sudo nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml
```

Then retry the `podman run --device nvidia.com/gpu=all ... nvidia-smi` test.

On openSUSE, NVIDIA publishes the toolkit packages from the CUDA repository. A
typical setup is:

```bash
sudo zypper addrepo https://developer.download.nvidia.com/compute/cuda/repos/suse16/x86_64/ cuda
sudo zypper refresh
sudo zypper install -y nvidia-container-toolkit
```

The package may create the `nvidia-cdi-refresh` systemd units and regenerate the
CDI file during installation. Confirm before starting the speech server:

```bash
nvidia-ctk cdi list
podman run --rm --device nvidia.com/gpu=all --security-opt=label=disable docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04 nvidia-smi
```

## Runtime Diagnostics

The server logs the selected backend, task, language, and target language at startup.

The `realtime-stt` backend logs:

- requested device
- main model
- realtime model
- language

The `onnx` backend logs:

- available ONNX Runtime providers
- selected provider chain
- model id or path
- warning when a requested provider cannot be used

## Benchmarking

Use [Benchmarking](./benchmark.md) before changing default backend recommendations.
The benchmark client measures server result latency through the same TCP protocol
used by the overlay.

## Notes

- The ONNX backend currently produces final results after hotkey release.
- The ONNX backend does not provide word-level probabilities yet, so the client uses plain text rendering.
- Keep model downloads and conversions out of the dictation hot path.
- Use smaller models for CPU-only setups unless latency is not important.

## Troubleshooting

If container builds fail while installing `PyAudio` or `webrtcvad` with an error
like `x86_64-linux-gnu-gcc failed: No such file or directory`, the image is
missing a native build toolchain. The provided Dockerfile installs
`build-essential` and `python3-dev` for the CPU and GPU targets because these
Python packages may need to compile native extensions during `pip install`.

If Podman Compose prints a command like
`${WHISPER_OVERLAY_SERVER_COMMAND:-python3: not found`, rebuild with the current
compose file. The server command must be executed through `sh -c` for compose
providers that do not handle default environment values with spaces the same way.

If startup fails with `ModuleNotFoundError: No module named 'requests'` while
importing `faster_whisper`, rebuild with the current Dockerfile. Some
RealtimeSTT requirement sets do not pull `requests` explicitly, so the container
targets install it after the upstream requirements file.

If `nvidia-smi` works on the host but the container prints `WARNING: The NVIDIA
Driver was not detected`, the GPU driver is installed correctly on the host but
was not passed into the container. Fix the Podman/Docker GPU runtime setup first;
changing `--device cuda` alone is not enough.

If the container resolves the CDI device but prints `Failed to initialize NVML:
Insufficient Permissions`, add `--security-opt=label=disable` to the Podman
command. NVIDIA documents this as the fix for SELinux-style labeling preventing
the container from using host driver files.
