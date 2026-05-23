# Benchmarking

Benchmarks should decide backend defaults with data from real machines, not assumptions.

## Scope

Measure:

- startup time
- first realtime or final result latency
- final result latency after flush
- real-time factor
- RAM/VRAM usage
- CPU/GPU utilization
- transcript quality smoke checks
- translation quality smoke checks

The repository includes a black-box benchmark client at `tools/benchmark_server.py`. It talks to the server using the same TCP protocol as the Rust overlay, sends a 16 kHz mono PCM16 WAV file, flushes, and records result latency.

## Audio Fixture

Use short WAV files with known expected text:

- 16 kHz
- mono
- PCM16
- 5 to 20 seconds per sample
- include at least English and one multilingual sample

Example validation with ffprobe:

```bash
ffprobe sample.wav
```

Convert a file with ffmpeg:

```bash
ffmpeg -i input.wav -ac 1 -ar 16000 -sample_fmt s16 sample.wav
```

## Running Benchmarks

Benchmark an already running server:

```bash
python3 tools/benchmark_server.py \
  --address localhost:7007 \
  --audio samples/en-short.wav \
  --runs 3 \
  --output benchmark-realtime-stt-cuda.json
```

Start and benchmark a server command:

```bash
python3 tools/benchmark_server.py \
  --audio samples/en-short.wav \
  --server-command "python3 realtime-stt-server.py --backend realtime-stt --device cpu --model base" \
  --runs 3 \
  --output benchmark-realtime-stt-cpu.json
```

ONNX CPU:

```bash
python3 tools/benchmark_server.py \
  --audio samples/en-short.wav \
  --server-command "python3 realtime-stt-server.py --backend onnx --onnx-provider cpu --onnx-model optimum/whisper-tiny.en" \
  --runs 3 \
  --output benchmark-onnx-cpu.json
```

ONNX auto provider:

```bash
python3 tools/benchmark_server.py \
  --audio samples/en-short.wav \
  --server-command "python3 realtime-stt-server.py --backend onnx --onnx-provider auto --onnx-model optimum/whisper-tiny.en" \
  --runs 3 \
  --output benchmark-onnx-auto.json
```

## Result Shape

The benchmark writes JSON:

```json
{
  "address": "localhost:7007",
  "audio": "samples/en-short.wav",
  "runs": [
    {
      "first_result_ms": 320,
      "final_result_ms": 1250,
      "text": "hello world",
      "backend": "onnx",
      "model": "optimum/whisper-tiny.en",
      "task": "transcribe"
    }
  ],
  "summary": {
    "runs": 3,
    "first_result_ms_avg": 330.67,
    "final_result_ms_avg": 1240.33,
    "final_result_ms_min": 1210,
    "final_result_ms_max": 1290
  }
}
```

## Manual Metrics

Record these alongside JSON results:

| Metric | Command or source |
| --- | --- |
| CPU usage | `top`, `htop`, or `pidstat` |
| RAM usage | `ps`, `smem`, or container stats |
| NVIDIA GPU usage | `nvidia-smi dmon` or `nvidia-smi` |
| Docker resource use | `docker stats` |
| Model cache size | `du -sh ~/.cache` or selected cache dir |

## Comparison Table Template

| Backend | Hardware | Model | Provider | Task | First result avg | Final result avg | RAM | VRAM | Notes |
| --- | --- | --- | --- | --- | ---: | ---: | ---: | ---: | --- |
| realtime-stt | CPU | base | cpu | transcribe | | | | | |
| realtime-stt | NVIDIA | large-v3 | cuda | transcribe | | | | | |
| onnx | CPU | optimum/whisper-tiny.en | cpu | transcribe | | | | | |
| onnx | NVIDIA | optimum/whisper-tiny.en | cuda | transcribe | | | | | |

## Default Backend Decision Rules

Keep `realtime-stt` as the default unless ONNX demonstrates all of these:

- equal or better latency for the recommended model on common hardware
- acceptable quality on multilingual samples
- stable provider detection and fallback
- installation path that is no more difficult than the current default
- no regression for realtime partial transcription expectations

Prefer ONNX for a specific environment when:

- the target hardware has a strong ONNX Runtime provider
- final-result dictation is acceptable
- packaging and model availability are simpler than RealtimeSTT

## Known Current Limitations

- The ONNX backend is final-result only.
- The benchmark does not measure overlay rendering or virtual keyboard typing.
- The benchmark does not automatically capture CPU/GPU utilization yet.
- Translation quality must be reviewed manually until expected-output scoring is added.
