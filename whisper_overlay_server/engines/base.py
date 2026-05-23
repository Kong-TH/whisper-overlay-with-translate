from abc import ABC, abstractmethod


class TranscriptionEngine(ABC):
    name = "base"

    @abstractmethod
    def initialize(self):
        """Load models and start background workers before accepting clients."""

    @abstractmethod
    def start(self):
        """Start accepting audio for the active client."""

    @abstractmethod
    def feed_audio(self, pcm_bytes):
        """Process a chunk of 16 kHz mono PCM audio kept entirely in memory."""

    @abstractmethod
    def flush(self):
        """Finish the current utterance and publish the final result when available."""

    @abstractmethod
    def stop(self):
        """Stop accepting audio for the active client."""

    @abstractmethod
    def shutdown(self):
        """Release model resources and stop background workers."""
