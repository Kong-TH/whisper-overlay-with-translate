import threading

from .base import TranscriptionEngine


class RealtimeSttEngine(TranscriptionEngine):
    name = "realtime-stt"

    def __init__(self, args, logger, publish_result):
        self.args = args
        self.logger = logger
        self.publish_result = publish_result
        self.recorder = None
        self.recorder_ready = threading.Event()
        self.recorder_thread = None

    def initialize(self):
        # Keep model loading inside the engine so future backends can own their setup.
        if self.args.device == "cpu":
            import torch

            # RealtimeSTT currently chooses CUDA by probing torch directly.
            torch.cuda.is_available = lambda: False

        self.logger.info("Importing RealtimeSTT runtime")
        from RealtimeSTT import AudioToTextRecorder

        recorder_config = {
            "init_logging": False,
            # FIXME: once fixed upstream "device": self.args.device,
            "use_microphone": False,
            "spinner": False,
            "model": self.args.model,
            "return_segments": True,
            "language": self.args.language,
            "silero_sensitivity": 0.4,
            "webrtc_sensitivity": 2,
            "post_speech_silence_duration": 0.7,
            "min_length_of_recording": 0.0,
            "min_gap_between_recordings": 0,
            "enable_realtime_transcription": True,
            "realtime_processing_pause": 0,
            "realtime_model_type": self.args.model_realtime,
            "on_realtime_transcription_stabilized": self._text_detected,
        }

        def recorder_worker():
            self.logger.info("Initializing RealtimeSTT...")
            self.recorder = AudioToTextRecorder(**recorder_config)
            self.logger.info("AudioToTextRecorder ready")
            self.recorder_ready.set()
            try:
                while not self.recorder.is_shut_down:
                    text, segments = self.recorder.text()
                    if text == "":
                        continue
                    self.publish_result(
                        {
                            "kind": "result",
                            "text": text,
                            "segments": self._serialize_segments(segments),
                            "backend": self.name,
                        }
                    )
            except (OSError, EOFError) as e:
                self.logger.info(f"recorder thread failed: {e}")

        self.recorder_thread = threading.Thread(target=recorder_worker)
        self.recorder_thread.start()
        self.recorder_ready.wait()

    def start(self):
        self.recorder.start()

    def feed_audio(self, pcm_bytes):
        self.recorder.feed_audio(pcm_bytes)

    def flush(self):
        self.logger.info("flushing on client request")
        # Feed a short silence buffer so RealtimeSTT can close the utterance.
        for _ in range(10):
            self.recorder.feed_audio(bytes(1000))
        self.recorder.stop()
        self.logger.info("flushed")

    def stop(self):
        self.recorder.stop()

    def shutdown(self):
        self.recorder.shutdown()
        if self.recorder_thread is not None:
            self.recorder_thread.join()

    def _text_detected(self, ts):
        text, segments = ts
        self.publish_result(
            {
                "kind": "realtime",
                "text": text,
                "segments": self._serialize_segments(segments),
                "backend": self.name,
            }
        )

    def _serialize_segments(self, segments):
        return [segment._asdict() for segment in segments]
