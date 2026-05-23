# whisper-overlay ONNX Runtime Migration Plan

## Goal

ปรับ `whisper-overlay` ให้รองรับ backend ที่หลากหลายขึ้นสำหรับผู้ใช้หลายสภาพแวดล้อม เช่น CPU-only, NVIDIA CUDA, Intel/OpenVINO, AMD/ROCm หรือ backend อื่นในอนาคต โดยยังรักษา behavior เดิมของโปรเจกต์ให้มากที่สุด:

- Rust client ยังทำหน้าที่ overlay, hotkey, audio capture และ text injection
- Server ยัง expose protocol เดิมให้ client ใช้งานได้
- Backend เดิม `RealtimeSTT`/`faster-whisper` ยังควรใช้งานได้เป็น baseline
- เพิ่ม ONNX Runtime เป็น backend ใหม่แบบ incremental ไม่ rewrite ทั้งระบบ

## Design Principles

1. รักษา client-server split เดิม
   - Rust client ไม่ควรรู้ว่า server ใช้ `RealtimeSTT`, `faster-whisper`, ONNX Runtime หรือ engine อื่น
   - Server เป็นที่เดียวที่เลือก engine และจัดการ model/runtime

2. รักษา protocol ให้ backward-compatible
   - หลีกเลี่ยงการเปลี่ยน framing protocol
   - เพิ่ม field ใหม่ได้ เช่น `backend`, `language`, `translated_text`, `confidence`, `timings`
   - ห้ามลบ field เดิมอย่าง `kind`, `text`, `segments` จนกว่าจะมี migration ชัดเจน

3. แยก backend abstraction ก่อนเพิ่ม ONNX
   - อย่าใส่ ONNX logic ปนใน `handle_client`
   - ทำ interface กลางสำหรับ transcription engine
   - ทำ adapter ให้ backend แต่ละตัวส่ง output เป็นรูปแบบกลางเดียวกัน

4. รองรับ feature degradation อย่างชัดเจน
   - บาง ONNX model อาจไม่มี word-level timestamps หรือ probabilities
   - UI/client ต้องรับ result ที่มีแค่ plain `text` ได้
   - ถ้าไม่มี confidence ให้ใช้ default rendering แทน gradient confidence

5. วัดผลจริงก่อนตัดสินว่า backend ไหนเป็น default
   - ONNX Runtime ไม่ได้เร็วกว่า `faster-whisper` เสมอ
   - ต้อง benchmark บน hardware จริงหลายแบบก่อนเลือก default สำหรับ release

## Current Architecture Summary

Current configuration surface:

- Server settings are CLI-only, for example `--host`, `--port`, `--device`, `--model`, `--model-realtime`, and `--language`
- Client settings are CLI-only, for example `--address`, `--style`, and `--hotkey`
- There is no graphical settings UI yet
- Persistent setup currently depends on desktop startup commands, shell scripts, systemd services, Docker Compose, or NixOS configuration

### Rust Client

Files:

- `src/app.rs`: orchestration หลักของ overlay, connection, audio streaming, UI update
- `src/cli.rs`: CLI options
- `src/hotkeys.rs`: global hotkey ผ่าน `evdev`
- `src/keyboard.rs`: text injection ผ่าน `enigo`
- `src/util.rs`: TCP message framing
- `src/waybar.rs`: Waybar status integration

Flow:

1. Hotkey pressed
2. Client connects to server
3. Client sends raw PCM audio frames
4. Client receives `realtime` and `result` messages
5. UI renders words and confidence
6. Final `result` is typed into focused window

### Python Server

File:

- `realtime-stt-server.py`

Current responsibilities:

- TCP server
- status client handling
- single active stream lock
- RealtimeSTT recorder lifecycle
- raw audio feeding
- realtime/final result publishing

## Proposed Result Message Shape

Keep existing fields:

```json
{
  "kind": "result",
  "text": "hello world",
  "segments": []
}
```

Allow extended fields:

```json
{
  "kind": "result",
  "text": "hello world",
  "translated_text": "สวัสดีชาวโลก",
  "language": "en",
  "target_language": "th",
  "backend": "onnx",
  "model": "whisper-small",
  "segments": [
    {
      "begin": 0.0,
      "end": 1.2,
      "text": "hello world",
      "words": [
        {
          "begin": 0.0,
          "end": 0.5,
          "word": "hello",
          "probability": 0.9
        }
      ]
    }
  ],
  "timings": {
    "decode_ms": 120,
    "total_ms": 180
  }
}
```

Rules:

- `text` is always the primary transcript or translated output depending on mode
- `translated_text` is optional
- `segments` is optional or can be empty for backends without word-level metadata
- client must not assume `segments[].words` always exists

## Phase 1: Stabilize Existing Protocol and Client Parsing

Purpose: make the current app tolerant of different backend outputs before adding ONNX.

Tasks:

1. Add explicit shared result model on the Rust side
   - Move `ModelResult`, `Segment`, and `Word` parsing out of the UI-heavy block in `src/app.rs`
   - Consider a new file such as `src/protocol.rs` or `src/model_result.rs`

2. Make rendering handle both rich and plain results
   - If `segments.words` exists, render confidence-colored words as today
   - If only `text` exists, render escaped plain text
   - If `translated_text` exists and selected mode wants translation, type/render that

3. Fix or verify final-result shutdown logic
   - Current code appears to check `kind != "result"` where the comment suggests `kind == "result"`
   - Add a focused test or manual verification path for hotkey release and final flush behavior

4. Harden TCP receive behavior in Python
   - Replace single `sock.recv(message_length)` with a `recv_exact(sock, n)` helper
   - This prevents partial TCP reads from corrupting JSON/audio messages

Deliverable:

- Existing `RealtimeSTT` backend behaves the same
- Client no longer requires word-level segments to function

## Phase 2: Server Backend Abstraction

Purpose: isolate transcription engine choice from TCP/session handling.

Suggested Python structure:

```text
server/
  __init__.py
  protocol.py
  engines/
    __init__.py
    base.py
    realtime_stt.py
    onnx_whisper.py
```

Minimal interface concept:

```python
class TranscriptionEngine:
    def start(self): ...
    def feed_audio(self, pcm_bytes: bytes): ...
    def flush(self): ...
    def stop(self): ...
    def shutdown(self): ...
```

Output concept:

```python
{
    "kind": "realtime" | "result",
    "text": "...",
    "segments": [...],
    "backend": "realtime-stt" | "onnx"
}
```

Tasks:

1. Extract existing RealtimeSTT logic into `RealtimeSttEngine`
2. Keep server socket/session logic separate from engine internals
3. Add CLI option:
   - `--backend realtime-stt|onnx`
4. Keep `realtime-stt` as default during migration

Deliverable:

- No behavior change for default users
- Backend can be selected from CLI

## Phase 3: ONNX Runtime Backend Prototype

Purpose: add an experimental ONNX backend while keeping fallback safe.

Tasks:

1. Choose first ONNX target
   - Start with Whisper-compatible ONNX model if available with a stable Python API
   - Prefer a maintained model/export path that supports Linux CPU and GPU execution providers

2. Add ONNX Runtime dependencies
   - CPU package path: `onnxruntime`
   - GPU package path: `onnxruntime-gpu`
   - Avoid forcing GPU dependency for CPU-only users

3. Add ONNX backend CLI options:
   - `--onnx-model PATH_OR_ID`
   - `--onnx-provider cpu|cuda|tensorrt|rocm|openvino|auto`
   - `--onnx-device DEVICE`
   - `--compute-type auto|fp32|fp16|int8`

4. Implement basic final transcription first
   - Realtime partial output can be added later
   - For first version, `kind="result"` is enough

5. Normalize ONNX output into common result shape
   - If no word timestamps are available, send `segments: []`
   - Always send `text`
   - Include `backend: "onnx"`

Deliverable:

- `realtime-stt-server.py --backend onnx ...` can produce final transcript
- Rust client displays/types plain text even without word-level metadata

## Phase 4: Translation Support

Purpose: support translated dictation without coupling UI to engine details.

Tasks:

1. Add server options:
   - `--task transcribe|translate`
   - `--language SOURCE_LANGUAGE`
   - `--target-language TARGET_LANGUAGE`

2. Define output behavior:
   - `task=transcribe`: `text` is source-language transcript
   - `task=translate`: `text` may be translated output for typing convenience
   - Optionally include `source_text` and `translated_text` for richer UI later

3. Add client option if needed:
   - `--type-field text|translated_text|source_text`
   - Default should keep old behavior

Deliverable:

- Users can choose transcription or translation mode from server/client configuration
- Result shape remains backward-compatible

## Phase 5: Packaging for Multiple Environments

Purpose: make shared usage practical for users with different hardware.

Tasks:

1. Docker images
   - CPU image
   - NVIDIA CUDA image
   - Optional ONNX Runtime GPU image

2. Nix packages
   - Keep current `realtime-stt` path
   - Add optional ONNX package/dependency path
   - Avoid making CUDA mandatory

3. Documentation matrix
   - CPU-only
   - NVIDIA CUDA
   - Intel/OpenVINO
   - AMD/ROCm, if feasible
   - Known unsupported or untested combinations

4. Runtime diagnostics
   - Log selected backend
   - Log selected ONNX providers
   - Log model path
   - Log whether execution fell back to CPU

Deliverable:

- Users can pick installation path based on hardware
- Failure modes are easier to diagnose

## Phase 6: Graphical Settings UI

Purpose: make backend, provider, model, translation, and hotkey settings accessible to normal users without editing CLI commands or service files by hand.

Design direction:

- Keep CLI flags as the source of truth for scripting and power users
- Add a GTK4 settings window in the Rust client for interactive configuration
- Store user settings in a small config file rather than hardcoding startup commands
- Keep settings independent from any single backend so `realtime-stt`, ONNX, and future engines can share the same UI surface

Suggested config file:

```toml
[client]
address = "localhost:7007"
hotkey = "KEY_RIGHTCTRL"
style = ""
type_field = "text"

[server]
backend = "realtime-stt"
host = "localhost"
port = 7007
language = ""
task = "transcribe"
target_language = ""

[realtime_stt]
device = "cuda"
model = "large-v3"
model_realtime = "base"
model_source = "builtin"
custom_model = ""

[onnx]
model = ""
provider = "auto"
device = "auto"
compute_type = "auto"
model_source = "builtin"
custom_model = ""

[models]
cache_dir = ""
catalog_url = ""
```

Settings UI sections:

1. General
   - Server address
   - Hotkey
   - Overlay style path
   - Text field to type: `text`, `translated_text`, or `source_text`

2. Backend
   - Backend selector: `realtime-stt`, `onnx`
   - Backend status and detected capabilities
   - Provider selector for ONNX: `auto`, `cpu`, `cuda`, `tensorrt`, `rocm`, `openvino`

3. Model
   - Main model preset
   - Realtime model, when supported
   - ONNX model path or model id
   - Custom model path or Hugging Face model id
   - Model source selector: built-in preset, downloaded catalog model, local path, or manual model id
   - Model install/download button for supported remote models
   - Model cache location and disk usage
   - Compute type: `auto`, `fp32`, `fp16`, `int8`

4. Language and translation
   - Source language
   - Task: transcribe or translate
   - Target language

5. Diagnostics
   - Active backend
   - Available ONNX Runtime providers
   - Selected provider chain
   - Whether execution fell back to CPU
   - Selected model path or model id
   - Model availability and download status
   - Server connection status

Settings UI mockup:

The settings UI should be a compact GTK4 utility window, not a full-screen dashboard. Use a left sidebar for pages and a right content pane for dense form controls. Buttons should use icons where GTK/lucide-equivalent symbols are available, with short labels only for primary actions.

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Whisper Overlay Settings                                             [×]    │
├──────────────────┬───────────────────────────────────────────────────────────┤
│ General          │ General                                                   │
│ Backend          │                                                           │
│ Models           │ Server                                                    │
│ Language         │  Address                [ localhost:7007              ]   │
│ Overlay          │  Connection             ● Connected                       │
│ Diagnostics      │                                                           │
│                  │ Input                                                     │
│                  │  Hotkey                 [ KEY_RIGHTCTRL        Record ]   │
│                  │  Type field             [ text                    ▾ ]     │
│                  │                                                           │
│                  │ Overlay                                                   │
│                  │  Style file             [ /path/style.css      Browse ]   │
│                  │                                                           │
│                  │                                [ Revert ] [ Save ]        │
└──────────────────┴───────────────────────────────────────────────────────────┘
```

### Page: General

Purpose: everyday settings users are most likely to change.

Controls:

- Server address text field
- Server connection status indicator
- Hotkey field with a "Record" button
- Type field selector:
  - `text`
  - `source_text`
  - `translated_text`
- Overlay style file chooser
- Save/Revert buttons

Validation:

- Address must be `host:port`
- Hotkey must map to an `evdev::Key`
- If `translated_text` is selected while task is `transcribe`, show a non-blocking warning

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Backend                                                   Active: realtime-stt│
├──────────────────────────────────────────────────────────────────────────────┤
│ Backend                                                                   │
│  Engine                  [ realtime-stt                         ▾ ]         │
│  Task                    [ transcribe                           ▾ ]         │
│                                                                            │
│ RealtimeSTT                                                               │
│  Device                  [ cuda                                  ▾ ]         │
│  Main model              [ large-v3                              ▾ ]         │
│  Realtime model          [ base                                  ▾ ]         │
│                                                                            │
│ ONNX Runtime                                                              │
│  Provider                [ auto                                  ▾ ]         │
│  Compute type            [ auto                                  ▾ ]         │
│                                                                            │
│ Current command                                                            │
│  realtime-stt-server.py --backend realtime-stt --device cuda ... [ Copy ]  │
│                                                                            │
│                                                    [ Revert ] [ Save ]      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Page: Backend

Purpose: select engine/provider without requiring users to understand every CLI flag.

Controls:

- Backend selector: `realtime-stt`, `onnx`
- Task selector: `transcribe`, `translate`
- RealtimeSTT device selector: `auto`, `cpu`, `cuda`
- ONNX provider selector: `auto`, `cpu`, `cuda`, `tensorrt`, `rocm`, `openvino`
- ONNX compute type selector: `auto`, `fp32`, `fp16`, `int8`
- Generated server command preview with copy button

Behavior:

- Show only backend-specific controls for the selected backend
- Keep advanced fields visible but disabled when not applicable
- Show a warning if a selected provider is not reported by server capabilities

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Models                                                        Cache: 4.2 GB  │
├──────────────────────────────────────────────────────────────────────────────┤
│ Source       [ Catalog                                        ▾ ] [Refresh] │
│ Search       [ whisper tiny                                     ]            │
│                                                                            │
│ Catalog                                                                    │
│  ┌─────────────────────────────┬────────┬────────────┬──────────┬────────┐ │
│  │ Model                       │Backend │ Languages  │ Size     │Status  │ │
│  ├─────────────────────────────┼────────┼────────────┼──────────┼────────┤ │
│  │ Whisper tiny.en ONNX        │ONNX    │ English    │ 200 MB   │Install │ │
│  │ Whisper base ONNX           │ONNX    │ Multi      │ 400 MB   │Ready   │ │
│  │ Whisper large-v3            │RSTT    │ Multi      │ cache    │Ready   │ │
│  └─────────────────────────────┴────────┴────────────┴──────────┴────────┘ │
│                                                                            │
│ Selected model                                                             │
│  Id/path                 [ optimum/whisper-tiny.en                    ]    │
│  Compatibility           ONNX · CPU/CUDA · transcribe                      │
│  Quality/speed           Fast · Lower accuracy                             │
│                                                                            │
│                                            [ Cancel Download ] [ Install ]  │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Page: Models

Purpose: make model selection and installation discoverable.

Controls:

- Model source selector:
  - Catalog
  - Local path
  - Manual model id
- Catalog search field
- Model table with columns:
  - model label
  - backend
  - language support
  - task support
  - size
  - install status
- Selected model details panel
- Install/download/cancel buttons
- Cache directory chooser
- Cache size display

Behavior:

- Downloads run in a background task with progress
- Settings are not saved to a newly selected model until the model is available
- Manual model ids are allowed but shown as "unverified"
- Model metadata must make hardware expectations clear before install

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Language and Translation                                                    │
├──────────────────────────────────────────────────────────────────────────────┤
│ Recognition                                                                 │
│  Source language         [ Auto detect                            ▾ ]        │
│                                                                            │
│ Output                                                                     │
│  Task                    [ Translate                              ▾ ]        │
│  Target language         [ English                                ▾ ]        │
│  Type into apps          [ translated_text                        ▾ ]        │
│                                                                            │
│ Notes                                                                      │
│  Whisper translation currently outputs English. Other target languages      │
│  require an additional translation backend in a later phase.                │
│                                                                            │
│                                                    [ Revert ] [ Save ]      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Page: Language

Purpose: configure recognition and translation output.

Controls:

- Source language selector:
  - Auto detect
  - common ISO language codes
  - manual code
- Task selector:
  - Transcribe
  - Translate
- Target language selector
- Type field selector synchronized with General page

Behavior:

- If backend can only translate to English, make that visible
- If selected model is English-only, disable incompatible source language choices
- If selected model does not support translation, disable `translate`

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Overlay                                                                     │
├──────────────────────────────────────────────────────────────────────────────┤
│ Position                                                                    │
│  Anchor                  [ Bottom                                ▾ ]         │
│  Bottom margin           [ 200 px                                ]           │
│  Width                   [ 1600 px                               ]           │
│                                                                            │
│ Text                                                                       │
│  Keep history             [ 6.0 s                                ]           │
│  Confidence colors        [ enabled                              ]           │
│  Plain-text fallback      [ enabled                              ]           │
│                                                                            │
│ Style                                                                      │
│  CSS file                [ /path/style.css                       Browse ]   │
│                                                                            │
│                                                    [ Revert ] [ Save ]      │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Page: Overlay

Purpose: tune overlay behavior without editing CSS or source code.

Controls:

- Anchor selector
- Margin/width numeric inputs
- History duration
- Confidence color toggle
- Plain-text fallback toggle
- CSS file chooser

Behavior:

- Keep defaults identical to current behavior
- Validate numeric values before save
- Advanced layout controls can be hidden under an expander initially

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ Diagnostics                                                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│ Server                                                                      │
│  Status                  ● Connected                                        │
│  Active backend           onnx                                              │
│  Model                    optimum/whisper-tiny.en                           │
│  Task                     translate                                         │
│                                                                            │
│ Runtime                                                                     │
│  ONNX providers           CUDAExecutionProvider, CPUExecutionProvider        │
│  Selected provider chain  CUDAExecutionProvider → CPUExecutionProvider      │
│  CPU fallback             no                                                │
│                                                                            │
│ Actions                                                                     │
│  [ Test Connection ] [ Copy Diagnostics ] [ Open Config File ]              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Page: Diagnostics

Purpose: make support/debugging practical for shared users.

Controls:

- Server connection status
- Active backend
- Active model
- Task/language summary
- ONNX provider list
- Selected provider chain
- CPU fallback indicator
- Test connection button
- Copy diagnostics button
- Open config file button

Behavior:

- Diagnostics should come from a status/capabilities message when available
- Copy output should redact local tokens or credentials if future integrations add them
- Errors should be shown as actionable text, not raw stack traces

Model management requirements:

- Support manually entering a model id or local model path
- Provide a curated model list for common Whisper-compatible models
- Show compatibility metadata before install:
  - backend support: `realtime-stt`, ONNX, or both
  - language support: English-only or multilingual
  - task support: transcribe, translate
  - recommended hardware: CPU, CUDA, TensorRT, OpenVINO, ROCm
  - expected size and disk usage
  - expected quality/speed tier
- Allow downloading/installing supported remote models by name
- Keep model downloads outside the hot audio path
- Never download a model implicitly during dictation without clear user action
- Validate model existence before saving settings when possible
- Allow advanced users to bypass the catalog with a manual model id/path

Suggested curated model catalog shape:

```json
[
  {
    "id": "optimum/whisper-tiny.en",
    "label": "Whisper tiny.en ONNX",
    "backend": ["onnx"],
    "languages": ["en"],
    "tasks": ["transcribe"],
    "size_mb": 200,
    "speed": "fast",
    "quality": "low",
    "recommended_providers": ["cpu", "cuda"]
  },
  {
    "id": "large-v3",
    "label": "Whisper large-v3",
    "backend": ["realtime-stt"],
    "languages": ["multilingual"],
    "tasks": ["transcribe", "translate"],
    "speed": "slow",
    "quality": "high",
    "recommended_providers": ["cuda"]
  }
]
```

Implementation tasks:

1. Add config loading/saving in Rust
   - Suggested file location: `$XDG_CONFIG_HOME/whisper-overlay/config.toml`
   - CLI flags should override config values for the current process
   - Missing config values should fall back to current defaults

2. Add a new CLI subcommand
   - `whisper-overlay settings`
   - Opens the GTK4 settings window

3. Add server capability endpoint or status extension
   - Extend status mode to optionally report backend, provider, model, and capability data
   - Keep existing Waybar fields backward-compatible

4. Decide how settings apply to the server
   - Local server managed by the app: settings UI can restart/update it later
   - External server: settings UI should show generated server command or systemd/Docker hints
   - Do not silently rewrite user-managed systemd/Docker/Nix files

5. Add documentation
   - Explain CLI-only usage
   - Explain GUI settings usage
   - Explain how GUI settings interact with external server deployments
   - Explain model catalog usage and manual model overrides

6. Add model catalog and installation support
   - Start with a bundled static catalog
   - Optionally allow refreshing catalog metadata from a documented URL later
   - Run downloads as background tasks with progress and cancellation
   - Store models in backend-native caches unless the user chooses a custom cache directory
   - Surface download errors without modifying the last known working settings

Deliverable:

- Users can configure common options from a GUI
- CLI workflows remain supported
- Settings are persisted in a documented config file
- GUI shows diagnostics needed to debug provider selection
- Users can select, download, or manually enter compatible models from the settings UI

## Phase 7: Benchmarking and Default Backend Decision

Purpose: avoid choosing ONNX as default based on assumption alone.

Benchmarks:

- Startup time
- First-token or first-partial latency
- Final result latency after hotkey release
- Real-time factor
- RAM/VRAM usage
- CPU/GPU utilization
- Accuracy smoke test on multilingual samples
- Translation quality smoke test

Compare:

- RealtimeSTT/faster-whisper CPU
- RealtimeSTT/faster-whisper CUDA
- ONNX Runtime CPU
- ONNX Runtime CUDA
- ONNX Runtime TensorRT, if practical
- ONNX Runtime OpenVINO, if practical

Deliverable:

- `docs/benchmark.md`
- Recommended backend table
- Default backend decision backed by data

## Risks

1. ONNX backend may not provide word-level timestamps/probabilities
   - Mitigation: make Rust client handle plain text result

2. ONNX Runtime provider setup can be fragile
   - Mitigation: clear provider logging and fallback detection

3. Performance may be worse than CTranslate2 on NVIDIA
   - Mitigation: keep `realtime-stt` backend available

4. Realtime partial transcription may be harder with ONNX
   - Mitigation: implement final result first, then incremental streaming

5. Packaging can become complex
   - Mitigation: keep optional dependencies separated by backend

## Recommended First Implementation Order

1. Fix protocol robustness with Python `recv_exact`
2. Make Rust result rendering accept plain `text`
3. Extract Rust result parsing/rendering away from the main UI block
4. Extract Python `RealtimeSttEngine`
5. Add `--backend realtime-stt|onnx`
6. Add minimal ONNX final-transcription backend
7. Add translation mode
8. Add graphical settings UI and persistent config
9. Add packaging/docs/benchmarks

## Initial Success Criteria

- Existing default workflow still works with `realtime-stt`
- Rust client can render and type results with no `segments.words`
- Python server supports backend selection without duplicating socket code
- ONNX backend can run at least CPU final transcription
- Documentation explains which backend to use for common hardware setups
