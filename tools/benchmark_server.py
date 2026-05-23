#!/usr/bin/env python3

import argparse
import json
import os
import queue
import socket
import struct
import subprocess
import sys
import threading
import time
import wave


def send_json(sock, message):
    payload = json.dumps(message).encode("utf-8")
    sock.sendall(struct.pack("!I", len(payload)))
    sock.sendall(payload)


def send_audio(sock, payload):
    if payload:
        sock.sendall(struct.pack("!I", len(payload) | 0x80000000))
        sock.sendall(payload)


def recv_exact(sock, length):
    chunks = []
    remaining = length
    while remaining > 0:
        chunk = sock.recv(remaining)
        if not chunk:
            return None
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def recv_json(sock):
    length_bytes = recv_exact(sock, 4)
    if length_bytes is None:
        return None
    length = struct.unpack("!I", length_bytes)[0]
    if length & 0x80000000:
        raise RuntimeError("server unexpectedly sent raw audio")
    payload = recv_exact(sock, length)
    if payload is None:
        return None
    return json.loads(payload.decode("utf-8"))


def read_pcm16_mono_16khz(path):
    with wave.open(path, "rb") as wav:
        channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        sample_rate = wav.getframerate()
        if channels != 1 or sample_width != 2 or sample_rate != 16000:
            raise ValueError(
                f"{path} must be 16 kHz mono PCM16 WAV "
                f"(got channels={channels}, sample_width={sample_width}, sample_rate={sample_rate})"
            )
        return wav.readframes(wav.getnframes())


def wait_for_server(address, timeout_seconds):
    host, port = split_address(address)
    deadline = time.monotonic() + timeout_seconds
    last_error = None
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=1.0):
                return
        except OSError as e:
            last_error = e
            time.sleep(0.25)
    raise TimeoutError(f"server did not become ready at {address}: {last_error}")


def split_address(address):
    host, port = address.rsplit(":", 1)
    return host, int(port)


def benchmark_once(address, audio_bytes, chunk_bytes, timeout_seconds):
    host, port = split_address(address)
    with socket.create_connection((host, port), timeout=timeout_seconds) as sock:
        sock.settimeout(timeout_seconds)
        send_json(sock, {"mode": "stream"})

        waiting = recv_json(sock)
        if waiting != {"status": "waiting for lock"}:
            raise RuntimeError(f"unexpected waiting response: {waiting}")

        acquired = recv_json(sock)
        if acquired != {"status": "lock acquired"}:
            raise RuntimeError(f"unexpected lock response: {acquired}")

        stream_started = time.perf_counter()
        for offset in range(0, len(audio_bytes), chunk_bytes):
            send_audio(sock, audio_bytes[offset : offset + chunk_bytes])
        send_json(sock, {"action": "flush"})

        first_result_ms = None
        final_result = None
        deadline = time.perf_counter() + timeout_seconds
        while time.perf_counter() < deadline:
            message = recv_json(sock)
            if message is None:
                break
            if message.get("kind") in ("realtime", "result") and first_result_ms is None:
                first_result_ms = elapsed_ms(stream_started)
            if message.get("kind") == "result":
                final_result = message
                break

        if final_result is None:
            raise TimeoutError("timed out waiting for final result")

        total_ms = elapsed_ms(stream_started)
        return {
            "first_result_ms": first_result_ms,
            "final_result_ms": total_ms,
            "text": final_result.get("text", ""),
            "backend": final_result.get("backend"),
            "model": final_result.get("model"),
            "task": final_result.get("task"),
            "raw_result": final_result,
        }


def elapsed_ms(start):
    return int((time.perf_counter() - start) * 1000)


def run_server(command, env):
    if not command:
        return None, None
    process = subprocess.Popen(
        command,
        shell=True,
        env=env,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    logs = []
    log_queue = queue.Queue()

    def collect_logs():
        for line in process.stdout:
            line = line.rstrip()
            logs.append(line)
            log_queue.put(line)

    thread = threading.Thread(target=collect_logs, daemon=True)
    thread.start()
    return process, logs


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--address", default="localhost:7007")
    parser.add_argument("--audio", required=True, help="16 kHz mono PCM16 WAV file")
    parser.add_argument("--server-command", default="", help="Optional command to start the server")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--chunk-ms", type=int, default=100)
    parser.add_argument("--startup-timeout", type=float, default=120.0)
    parser.add_argument("--result-timeout", type=float, default=60.0)
    parser.add_argument("--output", default="benchmark-result.json")
    args = parser.parse_args()

    audio_bytes = read_pcm16_mono_16khz(args.audio)
    chunk_bytes = int(16000 * 2 * (args.chunk_ms / 1000.0))
    env = os.environ.copy()
    server, server_logs = run_server(args.server_command, env)

    try:
        if server is not None:
            try:
                wait_for_server(args.address, args.startup_timeout)
            except Exception as e:
                tail = "\n".join((server_logs or [])[-40:])
                raise RuntimeError(f"{e}\nserver log tail:\n{tail}") from e

        runs = []
        for index in range(args.runs):
            run = benchmark_once(args.address, audio_bytes, chunk_bytes, args.result_timeout)
            run["run"] = index + 1
            runs.append(run)
            print(
                f"run={index + 1} first_result_ms={run['first_result_ms']} "
                f"final_result_ms={run['final_result_ms']} text={run['text']!r}"
            )

        summary = summarize(runs)
        result = {
            "address": args.address,
            "audio": args.audio,
            "runs": runs,
            "summary": summary,
        }
        with open(args.output, "w", encoding="utf-8") as f:
            json.dump(result, f, indent=2, ensure_ascii=False)
        print(f"wrote {args.output}")
    finally:
        if server is not None:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()


def summarize(runs):
    final_latencies = [run["final_result_ms"] for run in runs]
    first_latencies = [
        run["first_result_ms"] for run in runs if run["first_result_ms"] is not None
    ]
    return {
        "runs": len(runs),
        "first_result_ms_avg": average(first_latencies),
        "final_result_ms_avg": average(final_latencies),
        "final_result_ms_min": min(final_latencies),
        "final_result_ms_max": max(final_latencies),
    }


def average(values):
    if not values:
        return None
    return round(sum(values) / len(values), 2)


if __name__ == "__main__":
    try:
        main()
    except Exception as e:
        print(f"benchmark failed: {e}", file=sys.stderr)
        sys.exit(1)
