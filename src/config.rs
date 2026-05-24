use color_eyre::eyre::{Context, ContextCompat, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

const CONFIG_DIR_NAME: &str = "whisper-overlay";
const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub client: ClientConfig,
    pub audio: AudioConfig,
    pub caption: CaptionConfig,
    pub server: ServerConfig,
    pub realtime_stt: RealtimeSttConfig,
    pub onnx: OnnxConfig,
    pub models: ModelsConfig,
    pub overlay: OverlayConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            client: ClientConfig::default(),
            audio: AudioConfig::default(),
            caption: CaptionConfig::default(),
            server: ServerConfig::default(),
            realtime_stt: RealtimeSttConfig::default(),
            onnx: OnnxConfig::default(),
            models: ModelsConfig::default(),
            overlay: OverlayConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClientConfig {
    pub address: String,
    pub hotkey: String,
    pub style: String,
    pub type_field: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            address: "localhost:7007".to_string(),
            hotkey: "KEY_RIGHTCTRL".to_string(),
            style: String::new(),
            type_field: "text".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioConfig {
    pub source_kind: String,
    pub source_id: String,
    pub capture_mode: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            source_kind: "microphone".to_string(),
            source_id: "default".to_string(),
            capture_mode: "push-to-talk".to_string(),
            sample_rate: 16000,
            channels: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptionConfig {
    pub type_into_focused_app: bool,
    pub show_partial_results: bool,
    pub keep_visible_when_idle: bool,
    pub idle_hide_seconds: f64,
    pub finalize_interval_seconds: f64,
}

impl Default for CaptionConfig {
    fn default() -> Self {
        Self {
            type_into_focused_app: false,
            show_partial_results: true,
            keep_visible_when_idle: true,
            idle_hide_seconds: 4.0,
            finalize_interval_seconds: 6.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub backend: String,
    pub host: String,
    pub port: u16,
    pub language: String,
    pub task: String,
    pub target_language: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            backend: "realtime-stt".to_string(),
            host: "localhost".to_string(),
            port: 7007,
            language: String::new(),
            task: "transcribe".to_string(),
            target_language: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RealtimeSttConfig {
    pub device: String,
    pub model: String,
    pub model_realtime: String,
    pub model_source: String,
    pub custom_model: String,
}

impl Default for RealtimeSttConfig {
    fn default() -> Self {
        Self {
            device: "cuda".to_string(),
            model: "large-v3".to_string(),
            model_realtime: "base".to_string(),
            model_source: "builtin".to_string(),
            custom_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OnnxConfig {
    pub model: String,
    pub provider: String,
    pub device: String,
    pub compute_type: String,
    pub model_source: String,
    pub custom_model: String,
}

impl Default for OnnxConfig {
    fn default() -> Self {
        Self {
            model: "optimum/whisper-tiny.en".to_string(),
            provider: "auto".to_string(),
            device: "auto".to_string(),
            compute_type: "auto".to_string(),
            model_source: "builtin".to_string(),
            custom_model: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ModelsConfig {
    pub cache_dir: String,
    pub catalog_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    pub anchor: String,
    pub bottom_margin: i32,
    pub width: i32,
    pub keep_history_seconds: f64,
    pub confidence_colors: bool,
    pub plain_text_fallback: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            anchor: "bottom".to_string(),
            bottom_margin: 200,
            width: 1600,
            keep_history_seconds: 6.0,
            confidence_colors: true,
            plain_text_fallback: true,
        }
    }
}

pub fn config_path() -> Result<PathBuf> {
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context("Could not determine config directory from XDG_CONFIG_HOME or HOME")?;

    Ok(config_home.join(CONFIG_DIR_NAME).join(CONFIG_FILE_NAME))
}

pub fn load_config() -> Result<AppConfig> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let text = fs::read_to_string(&path)
        .wrap_err_with(|| format!("Could not read config file {}", path.display()))?;
    toml::from_str(&text)
        .wrap_err_with(|| format!("Could not parse config file {}", path.display()))
}

pub fn save_config(config: &AppConfig) -> Result<PathBuf> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .wrap_err_with(|| format!("Could not create config directory {}", parent.display()))?;
    }

    let text = toml::to_string_pretty(config).context("Could not serialize config")?;
    fs::write(&path, text)
        .wrap_err_with(|| format!("Could not write config file {}", path.display()))?;
    Ok(path)
}
