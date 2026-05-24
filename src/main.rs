use clap::Parser;
use color_eyre::eyre::Result;
use std::sync::OnceLock;
use tokio::runtime::Runtime;

mod app;
mod audio;
mod cli;
mod config;
mod hotkeys;
mod keyboard;
mod protocol;
mod settings;
mod util;
mod waybar;

pub fn runtime() -> &'static Runtime {
    static RUNTIME: OnceLock<Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| Runtime::new().expect("Setting up tokio runtime needs to succeed."))
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let args = cli::Cli::parse();

    match args.command {
        cli::Command::WaybarStatus { connection_opts } => {
            runtime()
                .block_on(async move { waybar::main_waybar_status(&connection_opts).await })?;
        }
        command @ cli::Command::Overlay { .. } => {
            app::launch_app(command)?;
        }
        cli::Command::Settings => {
            settings::launch_settings_app()?;
        }
        cli::Command::AudioSources { probe } => {
            let sources = audio::list_audio_sources()?;
            if sources.is_empty() {
                eprintln!(
                    "No input or monitor sources were reported by the current audio backend."
                );
            }
            for source in sources {
                let level = if probe {
                    match audio::probe_audio_source(&source) {
                        Ok(rms) => format!("\tpeak_rms={rms:.4}"),
                        Err(err) => format!("\tprobe_error={err}"),
                    }
                } else {
                    String::new()
                };
                let default_marker = if source.is_default { " default" } else { "" };
                println!(
                    "{}\t{}\t{}{}{}",
                    source.kind.as_str(),
                    source.id,
                    source.name,
                    default_marker,
                    level
                );
            }
        }
    }

    Ok(())
}
