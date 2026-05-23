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

## Phase 6: Benchmarking and Default Backend Decision

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
8. Add packaging/docs/benchmarks

## Initial Success Criteria

- Existing default workflow still works with `realtime-stt`
- Rust client can render and type results with no `segments.words`
- Python server supports backend selection without duplicating socket code
- ONNX backend can run at least CPU final transcription
- Documentation explains which backend to use for common hardware setups

