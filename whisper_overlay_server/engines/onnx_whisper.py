import threading
import time

from .base import TranscriptionEngine
from ..messages import build_result


PROVIDER_PRESETS = {
    "cpu": ["CPUExecutionProvider"],
    "cuda": ["CUDAExecutionProvider", "CPUExecutionProvider"],
    "tensorrt": [
        "TensorrtExecutionProvider",
        "CUDAExecutionProvider",
        "CPUExecutionProvider",
    ],
    "rocm": ["ROCMExecutionProvider", "CPUExecutionProvider"],
    "openvino": ["OpenVINOExecutionProvider", "CPUExecutionProvider"],
}

AUTO_PROVIDER_PRIORITY = [
    "TensorrtExecutionProvider",
    "CUDAExecutionProvider",
    "ROCMExecutionProvider",
    "OpenVINOExecutionProvider",
    "CPUExecutionProvider",
]


class OnnxWhisperEngine(TranscriptionEngine):
    name = "onnx"

    def __init__(self, args, logger, publish_result):
        self.args = args
        self.logger = logger
        self.publish_result = publish_result
        self.audio_buffer = bytearray()
        self.audio_lock = threading.Lock()
        self.pipeline = None
        self.selected_providers = []
        self.available_providers = []

    def initialize(self):
        # Optional dependencies are imported only when the ONNX backend is selected.
        try:
            import onnxruntime as ort
            from optimum.onnxruntime import ORTModelForSpeechSeq2Seq
            from transformers import AutoProcessor, pipeline
        except ImportError as e:
            raise RuntimeError(
                "The ONNX backend requires onnxruntime, optimum[onnxruntime], "
                "transformers, and numpy. Install the CPU or GPU ONNX Runtime "
                "variant that matches your hardware."
            ) from e

        self.available_providers = ort.get_available_providers()
        self.selected_providers = self._select_providers(self.available_providers)
        provider = self.selected_providers[0]

        self.logger.info(
            "ONNX Runtime available providers: %s",
            ", ".join(self.available_providers),
        )
        self.logger.info(
            "ONNX Runtime selected provider chain: %s",
            " -> ".join(self.selected_providers),
        )
        self.logger.info("Loading ONNX Whisper model: %s", self.args.onnx_model)
        if self.args.task == "translate" and self.args.target_language not in ("", "en"):
            self.logger.warning(
                "Whisper translation outputs English; target language '%s' is recorded "
                "in metadata but is not applied by this backend.",
                self.args.target_language,
            )

        model = ORTModelForSpeechSeq2Seq.from_pretrained(
            self.args.onnx_model,
            provider=provider,
            export=self.args.onnx_export,
        )
        processor = AutoProcessor.from_pretrained(self.args.onnx_model)
        self.pipeline = pipeline(
            "automatic-speech-recognition",
            model=model,
            tokenizer=processor.tokenizer,
            feature_extractor=processor.feature_extractor,
        )

    def start(self):
        with self.audio_lock:
            self.audio_buffer.clear()

    def feed_audio(self, pcm_bytes):
        with self.audio_lock:
            self.audio_buffer.extend(pcm_bytes)

    def flush(self):
        # The prototype backend performs final transcription after the user releases the hotkey.
        with self.audio_lock:
            pcm_bytes = bytes(self.audio_buffer)
            self.audio_buffer.clear()

        if not pcm_bytes:
            return

        started = time.perf_counter()
        audio = self._pcm16_to_float32(pcm_bytes)
        generate_kwargs = {}
        if self.args.language:
            generate_kwargs["language"] = self.args.language
        if self.args.task:
            generate_kwargs["task"] = self.args.task

        result = self.pipeline(
            {"array": audio, "sampling_rate": 16000},
            generate_kwargs=generate_kwargs,
        )
        elapsed_ms = int((time.perf_counter() - started) * 1000)
        text = result["text"] if isinstance(result, dict) else str(result)
        self.publish_result(
            build_result(
                kind="result",
                text=text,
                segments=[],
                backend=self.name,
                task=self.args.task,
                text_role="translated" if self.args.task == "translate" else "source",
                language=self.args.language,
                target_language=self.args.target_language,
                extra={
                    "model": self.args.onnx_model,
                    "onnx_provider": self.selected_providers[0],
                    "timings": {"total_ms": elapsed_ms},
                },
            )
        )

    def stop(self):
        pass

    def shutdown(self):
        pass

    def _select_providers(self, available_providers):
        if self.args.onnx_provider == "auto":
            selected = [
                provider
                for provider in AUTO_PROVIDER_PRIORITY
                if provider in available_providers
            ]
        else:
            requested = PROVIDER_PRESETS[self.args.onnx_provider]
            selected = [provider for provider in requested if provider in available_providers]

        if not selected:
            raise RuntimeError(
                f"ONNX provider '{self.args.onnx_provider}' is unavailable. "
                f"Available providers: {', '.join(available_providers)}"
            )

        if (
            self.args.onnx_provider != "cpu"
            and selected[0] == "CPUExecutionProvider"
            and self.args.onnx_provider != "auto"
        ):
            raise RuntimeError(
                f"ONNX provider '{self.args.onnx_provider}' fell back to CPU. "
                "Install the matching onnxruntime package and GPU runtime libraries."
            )

        return selected

    def _pcm16_to_float32(self, pcm_bytes):
        import numpy as np

        audio = np.frombuffer(pcm_bytes, dtype=np.int16).astype(np.float32)
        return audio / 32768.0
