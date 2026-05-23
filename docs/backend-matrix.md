# Backend and Hardware Matrix

This document summarizes the supported server backends and practical setup paths.

## Quick Matrix

| Environment | Backend | Docker target | Provider option | Notes |
| --- | --- | --- | --- | --- |
| CPU-only | `realtime-stt` | `cpu` | `--device cpu` | Best compatibility fallback. Latency depends heavily on model size. |
| NVIDIA CUDA | `realtime-stt` | `gpu` | `--device cuda` | Current default and strongest baseline for realtime transcription. |
| ONNX CPU | `onnx` | `onnx-cpu` | `--onnx-provider cpu` | Experimental final-result backend. No realtime partial output yet. |
| ONNX auto | `onnx` | `onnx-cpu` or `onnx-gpu` | `--onnx-provider auto` | Selects the best available ONNX Runtime provider and falls back to CPU. |
| ONNX NVIDIA | `onnx` | `onnx-gpu` | `--onnx-provider cuda` | Requires `onnxruntime-gpu` and compatible NVIDIA runtime libraries. |
| ONNX TensorRT | `onnx` | `onnx-gpu` | `--onnx-provider tensorrt` | Experimental. Provider availability depends on the installed ONNX Runtime build. |
| ONNX OpenVINO | `onnx` | custom | `--onnx-provider openvino` | Planned packaging path for Intel hardware. |
| ONNX ROCm | `onnx` | custom | `--onnx-provider rocm` | Planned packaging path for AMD GPU environments. |

## Container Targets

Build the default GPU RealtimeSTT server:

```bash
docker build --target gpu -t realtime-stt-server .
```

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

For NVIDIA GPU ONNX:

```bash
WHISPER_OVERLAY_DOCKER_TARGET=onnx-gpu \
WHISPER_OVERLAY_SERVER_COMMAND="python3 realtime-stt-server.py --host 0.0.0.0 --backend onnx --onnx-provider auto" \
docker-compose up --build
```

## Direct Container Run

Docker:

```bash
docker run --rm -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server
```

Podman:

```bash
podman run --rm -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server
```

nerdctl:

```bash
nerdctl run --rm -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server
```

For NVIDIA GPU containers, Docker usually needs the NVIDIA container runtime:

```bash
docker run --rm --gpus all -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server
```

Podman GPU setup depends on the host NVIDIA Container Toolkit/CDI configuration.
When CDI is configured, a typical command is:

```bash
podman run --rm --device nvidia.com/gpu=all -p 7007:7007 -v whisper-overlay-cache:/root/.cache realtime-stt-server
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
