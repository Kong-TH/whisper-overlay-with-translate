FROM docker.io/library/python:3.11-slim-bookworm AS base

WORKDIR /app

RUN apt-get update -y && \
  apt-get install -y --no-install-recommends build-essential git python3-dev python3-pip portaudio19-dev && \
  rm -rf /var/lib/apt/lists/*

COPY realtime-stt-server.py /app/realtime-stt-server.py
COPY whisper_overlay_server /app/whisper_overlay_server

EXPOSE 7007
ENV PYTHONPATH="${PYTHONPATH}:/app"
CMD ["python3", "realtime-stt-server.py", "--host", "0.0.0.0"]

FROM base AS cpu

RUN git clone https://github.com/oddlama/RealtimeSTT && \
  pip3 install --no-cache-dir torch==2.3.0 torchaudio==2.3.0 && \
  pip3 install --no-cache-dir -r RealtimeSTT/requirements.txt && \
  pip3 install --no-cache-dir requests && \
  cp -va RealtimeSTT/RealtimeSTT /app

FROM docker.io/nvidia/cuda:12.4.1-runtime-ubuntu22.04 AS gpu

WORKDIR /app

RUN apt-get update -y && \
  apt-get install -y --no-install-recommends build-essential git python3 python3-dev python3-pip libcudnn8 libcudnn8-dev libcublas-12-4 portaudio19-dev && \
  rm -rf /var/lib/apt/lists/*

RUN pip3 install --no-cache-dir torch==2.3.0 torchaudio==2.3.0

RUN git clone https://github.com/oddlama/RealtimeSTT && \
  pip3 install --no-cache-dir -r RealtimeSTT/requirements-gpu.txt && \
  pip3 install --no-cache-dir requests && \
  cp -va RealtimeSTT/RealtimeSTT /app
COPY realtime-stt-server.py /app/realtime-stt-server.py
COPY whisper_overlay_server /app/whisper_overlay_server

EXPOSE 7007
ENV PYTHONPATH="${PYTHONPATH}:/app"
CMD ["python3", "realtime-stt-server.py", "--host", "0.0.0.0"]

FROM base AS onnx-cpu

RUN pip3 install --no-cache-dir "optimum[onnxruntime]" transformers numpy onnxruntime
CMD ["python3", "realtime-stt-server.py", "--host", "0.0.0.0", "--backend", "onnx", "--onnx-provider", "auto"]

FROM gpu AS onnx-gpu

RUN pip3 install --no-cache-dir "optimum[onnxruntime-gpu]" transformers numpy onnxruntime-gpu
CMD ["python3", "realtime-stt-server.py", "--host", "0.0.0.0", "--backend", "onnx", "--onnx-provider", "auto"]
