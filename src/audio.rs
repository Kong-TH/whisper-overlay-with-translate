use color_eyre::eyre::{bail, Context, ContextCompat, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, SizedSample, StreamConfig, SupportedStreamConfig};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

const TARGET_SAMPLE_RATE: u32 = 16_000;
const SILENCE_RMS_THRESHOLD: f32 = 0.006;
const PULSE_SOURCE_PREFIX: &str = "pulse:";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSourceKind {
    Microphone,
    DesktopOutput,
    OutputDevice,
    Application,
}

impl AudioSourceKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "microphone" => Ok(Self::Microphone),
            "desktop-output" => Ok(Self::DesktopOutput),
            "output-device" => Ok(Self::OutputDevice),
            "application" => Ok(Self::Application),
            other => bail!(
                "Unsupported audio source kind '{other}'. Expected microphone, desktop-output, output-device, or application"
            ),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::DesktopOutput => "desktop-output",
            Self::OutputDevice => "output-device",
            Self::Application => "application",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AudioCaptureConfig {
    pub source_kind: AudioSourceKind,
    pub source_id: String,
}

#[derive(Debug, Clone)]
pub struct AudioSourceDescriptor {
    pub id: String,
    pub name: String,
    pub kind: AudioSourceKind,
    pub is_default: bool,
}

#[derive(Debug, Default)]
struct AudioLevelStats {
    chunks_seen: u32,
    chunks_sent: u32,
    chunks_silent: u32,
    peak_rms: f32,
}

impl AudioLevelStats {
    fn observe(&mut self, rms: f32, sent: bool) {
        self.chunks_seen += 1;
        self.peak_rms = self.peak_rms.max(rms);
        if sent {
            self.chunks_sent += 1;
        } else {
            self.chunks_silent += 1;
        }

        if self.chunks_seen >= 250 {
            println!(
                "Audio level: peak_rms={:.4}, sent_chunks={}, silent_chunks={}",
                self.peak_rms, self.chunks_sent, self.chunks_silent
            );
            *self = Self::default();
        }
    }
}

pub fn list_audio_sources() -> Result<Vec<AudioSourceDescriptor>> {
    let mut sources = Vec::new();
    add_pulse_sources(&mut sources);

    let host = cpal::default_host();
    let default_input_name = host
        .default_input_device()
        .and_then(|device| device.name().ok());

    if let Some(default_device) = host.default_input_device() {
        let name = default_device
            .name()
            .unwrap_or_else(|_| "default input".to_string());
        push_unique_source(
            &mut sources,
            AudioSourceDescriptor {
                id: name.clone(),
                name,
                kind: AudioSourceKind::Microphone,
                is_default: true,
            },
        );
    }

    match host.input_devices() {
        Ok(devices) => {
            for device in devices {
                let name = device
                    .name()
                    .unwrap_or_else(|_| "unknown input".to_string());
                let kind = AudioSourceKind::Microphone;
                let is_default = default_input_name.as_ref() == Some(&name);

                push_unique_source(
                    &mut sources,
                    AudioSourceDescriptor {
                        id: name.clone(),
                        name,
                        kind,
                        is_default,
                    },
                );
            }
        }
        Err(err) => eprintln!("warning: could not enumerate input devices: {err}"),
    }

    Ok(sources)
}

pub fn probe_audio_source(source: &AudioSourceDescriptor) -> Result<f32> {
    if let Some(name) = pulse_source_name(&source.id) {
        return probe_pulse_source(name);
    }

    Ok(0.0)
}

pub fn record_audio_to_wav(
    config: AudioCaptureConfig,
    duration: Duration,
    output: &Path,
) -> Result<()> {
    if config.source_kind == AudioSourceKind::Application {
        bail!("Per-application audio capture is not available through the current audio backend");
    }

    let samples = if matches!(
        config.source_kind,
        AudioSourceKind::DesktopOutput | AudioSourceKind::OutputDevice
    ) {
        let source_name = select_pulse_source_name(&config)?.with_context(|| {
            "Desktop audio recording requires a PulseAudio/PipeWire monitor source. \
             Run `whisper-overlay audio-sources --probe`, play audio, then select a source starting with `pulse:`."
        })?;
        record_pulse_samples(&source_name, duration)?
    } else {
        record_cpal_samples(&config, duration)?
    };

    let peak = pcm_rms(&samples);
    write_wav_i16_mono(output, TARGET_SAMPLE_RATE, &samples)
        .with_context(|| format!("Could not write WAV file to {}", output.display()))?;
    println!(
        "Recorded {:.2}s to {} ({} samples, peak_rms={:.4})",
        duration.as_secs_f64(),
        output.display(),
        samples.len(),
        peak
    );
    Ok(())
}

pub fn spawn_audio_capture(
    config: AudioCaptureConfig,
    bytes: Arc<Mutex<Vec<u8>>>,
    audio_tx: watch::Sender<()>,
    audio_active: Arc<Mutex<bool>>,
    audio_shutdown_rx: std::sync::mpsc::Receiver<()>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        if let Err(err) =
            run_audio_capture(config, bytes, audio_tx, audio_active, audio_shutdown_rx)
        {
            eprintln!("audio capture failed: {err:#}");
        }
    })
}

fn run_audio_capture(
    config: AudioCaptureConfig,
    bytes: Arc<Mutex<Vec<u8>>>,
    audio_tx: watch::Sender<()>,
    audio_active: Arc<Mutex<bool>>,
    audio_shutdown_rx: std::sync::mpsc::Receiver<()>,
) -> Result<()> {
    if config.source_kind == AudioSourceKind::Application {
        bail!("Per-application audio capture is not available through the current audio backend");
    }

    if matches!(
        config.source_kind,
        AudioSourceKind::DesktopOutput | AudioSourceKind::OutputDevice
    ) {
        let source_name = select_pulse_source_name(&config)?.with_context(|| {
            "Desktop audio capture requires a PulseAudio/PipeWire monitor source. \
             Run `whisper-overlay audio-sources --probe`, play audio, then select a source starting with `pulse:`."
        })?;
        return run_pulse_capture(
            &source_name,
            bytes,
            audio_tx,
            audio_active,
            audio_shutdown_rx,
        );
    }

    let host = cpal::default_host();
    let device =
        select_input_device(&host, &config).context("Could not select an audio capture device")?;
    let device_name = device
        .name()
        .unwrap_or_else(|_| "unknown input".to_string());
    let supported_config = select_supported_config(&device)
        .with_context(|| format!("Could not find a supported input config for {device_name}"))?;

    println!(
        "Input device: {} [{}] {} Hz, {} channel(s), {:?}",
        device_name,
        config.source_kind.as_str(),
        supported_config.sample_rate().0,
        supported_config.channels(),
        supported_config.sample_format()
    );

    let stream = build_input_stream(&device, &supported_config, bytes, audio_tx, audio_active)?;
    stream.play().context("Failed to start audio stream")?;

    let _ = audio_shutdown_rx.recv();
    Ok(())
}

fn add_pulse_sources(sources: &mut Vec<AudioSourceDescriptor>) {
    let Ok(output) = Command::new("pactl")
        .args(["-f", "json", "list", "sources"])
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }

    let Ok(values) = serde_json::from_slice::<Vec<Value>>(&output.stdout) else {
        return;
    };

    for value in values {
        let name = value
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let description = value
            .get("description")
            .and_then(|value| value.as_str())
            .unwrap_or(name)
            .to_string();
        let media_class = value
            .pointer("/properties/media.class")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let device_class = value
            .pointer("/properties/device.class")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let is_monitor = !value
            .get("monitor_source")
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .is_empty()
            || device_class == "monitor"
            || media_class == "Audio/Sink"
            || name.ends_with(".monitor");
        let kind = if is_monitor {
            AudioSourceKind::DesktopOutput
        } else {
            AudioSourceKind::Microphone
        };

        push_unique_source(
            sources,
            AudioSourceDescriptor {
                id: format!("{PULSE_SOURCE_PREFIX}{name}"),
                name: description,
                kind,
                is_default: false,
            },
        );
    }
}

fn push_unique_source(sources: &mut Vec<AudioSourceDescriptor>, source: AudioSourceDescriptor) {
    if sources.iter().any(|existing| existing.id == source.id) {
        return;
    }
    sources.push(source);
}

fn pulse_source_name(source_id: &str) -> Option<&str> {
    source_id.strip_prefix(PULSE_SOURCE_PREFIX)
}

fn select_pulse_source_name(config: &AudioCaptureConfig) -> Result<Option<String>> {
    if let Some(name) = pulse_source_name(&config.source_id) {
        return Ok(Some(name.to_string()));
    }

    let sources = list_audio_sources()?;
    if config.source_id != "default" {
        if let Some(source) = sources.iter().find(|source| {
            source.kind == AudioSourceKind::DesktopOutput
                && source
                    .name
                    .to_lowercase()
                    .contains(&config.source_id.to_lowercase())
        }) {
            return Ok(pulse_source_name(&source.id).map(str::to_string));
        }
    }

    Ok(sources
        .iter()
        .find(|source| source.kind == AudioSourceKind::DesktopOutput)
        .and_then(|source| pulse_source_name(&source.id))
        .map(str::to_string))
}

fn run_pulse_capture(
    source_name: &str,
    bytes: Arc<Mutex<Vec<u8>>>,
    audio_tx: watch::Sender<()>,
    audio_active: Arc<Mutex<bool>>,
    audio_shutdown_rx: std::sync::mpsc::Receiver<()>,
) -> Result<()> {
    println!("Input device: {source_name} [pulse monitor] 16000 Hz, 1 channel(s), I16");
    let mut child = Command::new("parec")
        .args([
            "--raw",
            "--format=s16le",
            "--rate=16000",
            "--channels=1",
            "--device",
            source_name,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not start parec for source {source_name}"))?;
    let mut stdout = child.stdout.take().context("parec stdout was not piped")?;
    let mut buffer = vec![0_u8; 4096];
    let mut level_stats = AudioLevelStats::default();

    loop {
        if audio_shutdown_rx.try_recv().is_ok() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        }

        let read = stdout
            .read(&mut buffer)
            .with_context(|| format!("Could not read audio from parec source {source_name}"))?;
        if read == 0 {
            break;
        }
        if read % 2 != 0 {
            continue;
        }

        if !*audio_active.lock().expect("Could not lock audio stop") {
            continue;
        }

        let pcm = bytemuck::cast_slice::<u8, i16>(&buffer[..read]);
        let rms = pcm_rms(pcm);
        if rms < SILENCE_RMS_THRESHOLD {
            level_stats.observe(rms, false);
            continue;
        }
        level_stats.observe(rms, true);

        bytes
            .lock()
            .expect("Could not lock mutex to write audio data")
            .extend_from_slice(&buffer[..read]);
        let _ = audio_tx.send(());
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(())
}

fn record_pulse_samples(source_name: &str, duration: Duration) -> Result<Vec<i16>> {
    println!("Recording source: {source_name} [pulse monitor] 16000 Hz, 1 channel(s), I16");
    let target_bytes = duration_to_target_bytes(duration);
    let mut child = Command::new("parec")
        .args([
            "--raw",
            "--format=s16le",
            "--rate=16000",
            "--channels=1",
            "--device",
            source_name,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not start parec for source {source_name}"))?;
    let mut stdout = child.stdout.take().context("parec stdout was not piped")?;
    let mut bytes = Vec::with_capacity(target_bytes);
    let mut buffer = vec![0_u8; 4096];

    while bytes.len() < target_bytes {
        let read = stdout
            .read(&mut buffer)
            .with_context(|| format!("Could not read audio from parec source {source_name}"))?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    let _ = child.kill();
    let _ = child.wait();
    bytes.truncate(target_bytes);
    if bytes.len() % 2 != 0 {
        bytes.pop();
    }
    Ok(bytemuck::cast_slice::<u8, i16>(&bytes).to_vec())
}

fn probe_pulse_source(source_name: &str) -> Result<f32> {
    let mut child = Command::new("parec")
        .args([
            "--raw",
            "--format=s16le",
            "--rate=16000",
            "--channels=1",
            "--device",
            source_name,
        ])
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("Could not start parec for source {source_name}"))?;
    let mut stdout = child.stdout.take().context("parec stdout was not piped")?;
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut buffer = vec![0_u8; 4096];
    let mut peak = 0.0_f32;

    while Instant::now() < deadline {
        let read = stdout
            .read(&mut buffer)
            .with_context(|| format!("Could not read probe audio from {source_name}"))?;
        if read == 0 {
            break;
        }
        if read % 2 != 0 {
            continue;
        }
        peak = peak.max(pcm_rms(bytemuck::cast_slice::<u8, i16>(&buffer[..read])));
    }

    let _ = child.kill();
    let _ = child.wait();
    Ok(peak)
}

fn record_cpal_samples(config: &AudioCaptureConfig, duration: Duration) -> Result<Vec<i16>> {
    let host = cpal::default_host();
    let device =
        select_input_device(&host, config).context("Could not select an audio capture device")?;
    let device_name = device
        .name()
        .unwrap_or_else(|_| "unknown input".to_string());
    let supported_config = select_supported_config(&device)
        .with_context(|| format!("Could not find a supported input config for {device_name}"))?;

    println!(
        "Recording source: {} [{}] {} Hz, {} channel(s), {:?}",
        device_name,
        config.source_kind.as_str(),
        supported_config.sample_rate().0,
        supported_config.channels(),
        supported_config.sample_format()
    );

    let target_samples = duration_to_target_samples(duration);
    let samples = Arc::new(Mutex::new(Vec::<i16>::with_capacity(target_samples)));
    let stream = build_record_stream(&device, &supported_config, samples.clone(), target_samples)?;
    stream.play().context("Failed to start audio stream")?;

    let deadline = Instant::now() + duration + Duration::from_secs(2);
    while Instant::now() < deadline {
        if samples
            .lock()
            .expect("Could not lock recorded sample buffer")
            .len()
            >= target_samples
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    drop(stream);
    let mut samples = samples
        .lock()
        .expect("Could not lock recorded sample buffer")
        .clone();
    samples.truncate(target_samples);
    Ok(samples)
}

fn select_input_device(host: &cpal::Host, config: &AudioCaptureConfig) -> Result<Device> {
    let source_id = &config.source_id;

    if config.source_id == "default" && config.source_kind == AudioSourceKind::Microphone {
        return host
            .default_input_device()
            .context("No default input device available");
    }

    for device in host
        .input_devices()
        .context("Could not enumerate input devices")?
    {
        let name = device.name().unwrap_or_default();

        if source_id != "default" && name == *source_id {
            return Ok(device);
        }

        if source_id != "default" && name.to_lowercase().contains(&source_id.to_lowercase()) {
            return Ok(device);
        }
    }

    if config.source_id == "default" {
        return host
            .default_input_device()
            .context("No default input device available");
    }

    bail!("Audio source '{}' was not found", config.source_id)
}

fn select_supported_config(device: &Device) -> Result<SupportedStreamConfig> {
    let mut best = None;
    let mut best_score = u32::MAX;

    for range in device
        .supported_input_configs()
        .context("Could not read supported input configs")?
    {
        let min = range.min_sample_rate().0;
        let max = range.max_sample_rate().0;
        let sample_rate = if min <= TARGET_SAMPLE_RATE && TARGET_SAMPLE_RATE <= max {
            TARGET_SAMPLE_RATE
        } else if TARGET_SAMPLE_RATE < min {
            min
        } else {
            max
        };
        let channel_penalty = range.channels().saturating_sub(1) as u32 * 1_000;
        let rate_penalty = sample_rate.abs_diff(TARGET_SAMPLE_RATE);
        let format_penalty = match range.sample_format() {
            SampleFormat::I16 => 0,
            SampleFormat::F32 => 100,
            SampleFormat::U16 => 200,
            _ => 500,
        };
        let score = rate_penalty + channel_penalty + format_penalty;

        if score < best_score {
            best_score = score;
            best = Some(range.with_sample_rate(SampleRate(sample_rate)));
        }
    }

    best.context("Device does not report any supported input config")
}

fn build_record_stream(
    device: &Device,
    supported_config: &SupportedStreamConfig,
    samples: Arc<Mutex<Vec<i16>>>,
    target_samples: usize,
) -> Result<cpal::Stream> {
    let stream_config: StreamConfig = supported_config.clone().into();
    let channels = stream_config.channels as usize;
    let input_rate = stream_config.sample_rate.0;
    let err_fn = move |err| eprintln!("an error occurred on the audio stream: {err}");

    match supported_config.sample_format() {
        SampleFormat::I16 => build_typed_record_stream::<i16>(
            device,
            &stream_config,
            channels,
            input_rate,
            samples,
            target_samples,
            err_fn,
        ),
        SampleFormat::U16 => build_typed_record_stream::<u16>(
            device,
            &stream_config,
            channels,
            input_rate,
            samples,
            target_samples,
            err_fn,
        ),
        SampleFormat::F32 => build_typed_record_stream::<f32>(
            device,
            &stream_config,
            channels,
            input_rate,
            samples,
            target_samples,
            err_fn,
        ),
        other => bail!("Unsupported input sample format: {other:?}"),
    }
}

fn build_typed_record_stream<T>(
    device: &Device,
    stream_config: &StreamConfig,
    channels: usize,
    input_rate: u32,
    samples: Arc<Mutex<Vec<i16>>>,
    target_samples: usize,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: SizedSample + AudioSample + Send + 'static,
{
    let mut normalizer = AudioNormalizer::new(channels, input_rate, TARGET_SAMPLE_RATE);
    device
        .build_input_stream(
            stream_config,
            move |data: &[T], _: &_| {
                let mut pcm = Vec::new();
                normalizer.push(data, &mut pcm);
                if pcm.is_empty() {
                    return;
                }

                let mut samples = samples.lock().expect("Could not lock sample buffer");
                if samples.len() >= target_samples {
                    return;
                }
                let remaining = target_samples - samples.len();
                samples.extend(pcm.into_iter().take(remaining));
            },
            err_fn,
            None,
        )
        .context("Failed to build audio input stream")
}

fn build_input_stream(
    device: &Device,
    supported_config: &SupportedStreamConfig,
    bytes: Arc<Mutex<Vec<u8>>>,
    audio_tx: watch::Sender<()>,
    audio_active: Arc<Mutex<bool>>,
) -> Result<cpal::Stream> {
    let stream_config: StreamConfig = supported_config.clone().into();
    let channels = stream_config.channels as usize;
    let input_rate = stream_config.sample_rate.0;
    let err_fn = move |err| eprintln!("an error occurred on the audio stream: {err}");

    match supported_config.sample_format() {
        SampleFormat::I16 => build_typed_input_stream::<i16>(
            device,
            &stream_config,
            channels,
            input_rate,
            bytes,
            audio_tx,
            audio_active,
            err_fn,
        ),
        SampleFormat::U16 => build_typed_input_stream::<u16>(
            device,
            &stream_config,
            channels,
            input_rate,
            bytes,
            audio_tx,
            audio_active,
            err_fn,
        ),
        SampleFormat::F32 => build_typed_input_stream::<f32>(
            device,
            &stream_config,
            channels,
            input_rate,
            bytes,
            audio_tx,
            audio_active,
            err_fn,
        ),
        other => bail!("Unsupported input sample format: {other:?}"),
    }
}

fn build_typed_input_stream<T>(
    device: &Device,
    stream_config: &StreamConfig,
    channels: usize,
    input_rate: u32,
    bytes: Arc<Mutex<Vec<u8>>>,
    audio_tx: watch::Sender<()>,
    audio_active: Arc<Mutex<bool>>,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: SizedSample + AudioSample + Send + 'static,
{
    let mut normalizer = AudioNormalizer::new(channels, input_rate, TARGET_SAMPLE_RATE);
    device
        .build_input_stream(
            stream_config,
            move |data: &[T], _: &_| {
                if !*audio_active.lock().expect("Could not lock audio stop") {
                    return;
                }

                let mut pcm = Vec::new();
                normalizer.push(data, &mut pcm);
                if pcm.is_empty() {
                    return;
                }
                let rms = pcm_rms(&pcm);
                if rms < SILENCE_RMS_THRESHOLD {
                    normalizer.level_stats.observe(rms, false);
                    return;
                }
                normalizer.level_stats.observe(rms, true);

                bytes
                    .lock()
                    .expect("Could not lock mutex to write audio data")
                    .extend_from_slice(bytemuck::cast_slice(&pcm));
                let _ = audio_tx.send(());
            },
            err_fn,
            None,
        )
        .context("Failed to build audio input stream")
}

struct AudioNormalizer {
    channels: usize,
    input_rate: u32,
    target_rate: u32,
    phase: f64,
    level_stats: AudioLevelStats,
}

impl AudioNormalizer {
    fn new(channels: usize, input_rate: u32, target_rate: u32) -> Self {
        Self {
            channels: channels.max(1),
            input_rate,
            target_rate,
            phase: 0.0,
            level_stats: AudioLevelStats::default(),
        }
    }

    fn push<T: AudioSample>(&mut self, input: &[T], output: &mut Vec<i16>) {
        let frames = input.len() / self.channels;
        if frames == 0 {
            return;
        }

        let mut mono = Vec::with_capacity(frames);
        for frame in input.chunks_exact(self.channels) {
            let sum = frame.iter().map(|sample| sample.to_f32()).sum::<f32>();
            mono.push(sum / self.channels as f32);
        }

        let step = self.input_rate as f64 / self.target_rate as f64;
        while self.phase < mono.len() as f64 {
            let idx = self.phase.floor() as usize;
            output.push(f32_to_i16(mono[idx]));
            self.phase += step;
        }
        self.phase -= mono.len() as f64;
    }
}

trait AudioSample {
    fn to_f32(&self) -> f32;
}

impl AudioSample for i16 {
    fn to_f32(&self) -> f32 {
        *self as f32 / i16::MAX as f32
    }
}

impl AudioSample for u16 {
    fn to_f32(&self) -> f32 {
        (*self as f32 - 32768.0) / 32768.0
    }
}

impl AudioSample for f32 {
    fn to_f32(&self) -> f32 {
        *self
    }
}

fn f32_to_i16(value: f32) -> i16 {
    (value.clamp(-1.0, 1.0) * i16::MAX as f32) as i16
}

fn duration_to_target_samples(duration: Duration) -> usize {
    (duration.as_secs_f64() * TARGET_SAMPLE_RATE as f64).ceil() as usize
}

fn duration_to_target_bytes(duration: Duration) -> usize {
    duration_to_target_samples(duration) * std::mem::size_of::<i16>()
}

fn write_wav_i16_mono(path: &Path, sample_rate: u32, samples: &[i16]) -> Result<()> {
    let data_bytes = samples.len() as u32 * 2;
    let riff_size = 36_u32
        .checked_add(data_bytes)
        .context("WAV file is too large")?;
    let mut file = File::create(path)?;

    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&sample_rate.to_le_bytes())?;
    file.write_all(&(sample_rate * 2).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_bytes.to_le_bytes())?;
    file.write_all(bytemuck::cast_slice(samples))?;
    Ok(())
}

fn pcm_rms(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return 0.0;
    }

    let sum_squares = pcm
        .iter()
        .map(|sample| {
            let normalized = *sample as f32 / i16::MAX as f32;
            normalized * normalized
        })
        .sum::<f32>();
    (sum_squares / pcm.len() as f32).sqrt()
}
