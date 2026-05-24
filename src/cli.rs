use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(version, about)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand, Clone)]
pub enum Command {
    WaybarStatus {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,
    },
    Settings,
    AudioSources {
        /// Probe each visible source briefly and print a peak RMS level.
        #[arg(long, default_value_t = false)]
        probe: bool,
    },
    Overlay {
        #[clap(flatten)]
        connection_opts: ConnectionOpts,

        /// An optional stylesheet for the overlay, which replaces the internal style.
        #[arg(short, short, long, default_value=None)]
        style: Option<PathBuf>,

        /// Specifies the hotkey to activate voice input. You can use any
        /// key or button name from [evdev::Key](https://docs.rs/evdev/latest/evdev/struct.Key.html)
        #[arg(long, default_value = "KEY_RIGHTCTRL")]
        hotkey: String,

        /// Audio source kind to capture. Desktop/application capture depends on host audio backend support.
        #[arg(long, default_value = "microphone")]
        audio_source_kind: String,

        /// Audio source id or device name. Use `whisper-overlay audio-sources` to list visible sources.
        #[arg(long, default_value = "default")]
        audio_source: String,

        /// Capture mode: push-to-talk, toggle-live-caption, or always-on-live-caption.
        #[arg(long, default_value = "push-to-talk")]
        capture_mode: String,

        /// Type final captions into the focused app. Defaults to dictation-only behavior.
        #[arg(long, default_value_t = false)]
        type_into_focused_app: bool,

        /// Disable text injection even in push-to-talk dictation mode.
        #[arg(long, default_value_t = false)]
        no_text_injection: bool,

        /// Seconds between finalization passes in live-caption modes. Set 0 to disable.
        #[arg(long, default_value_t = 6.0)]
        caption_finalize_interval: f64,
    },
}

#[derive(Debug, Args, Clone)]
pub struct ConnectionOpts {
    /// The address of the the whisper streaming instance (host:port)
    #[clap(short, long, default_value = "localhost:7007")]
    pub address: String,
}
