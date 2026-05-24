use color_eyre::eyre::{bail, Result};
use gdk::glib::ExitCode;
use gtk::prelude::*;
use gtk::{
    Adjustment, Application, ApplicationWindow, Button, CheckButton, ComboBoxText, Entry, Grid,
    Label, Orientation, ScrolledWindow, SpinButton, Stack, StackSidebar,
};

use crate::audio::list_audio_sources;
use crate::audio::AudioSourceKind;
use crate::config::{config_path, load_config, save_config, AppConfig};

const APP_ID: &str = "org.oddlama.whisper-overlay.settings";

pub fn launch_settings_app() -> Result<()> {
    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);

    let exit_code = app.run_with_args::<&str>(&[]);
    if exit_code != ExitCode::SUCCESS {
        bail!("Could not launch settings application: {:?}", exit_code);
    }

    Ok(())
}

fn build_ui(app: &Application) {
    let config = load_config().unwrap_or_else(|e| {
        eprintln!("Could not load config, using defaults: {e}");
        AppConfig::default()
    });

    let widgets = SettingsWidgets::new(&config);
    let root = gtk::Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();

    let content = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(12)
        .vexpand(true)
        .hexpand(true)
        .build();

    let stack = Stack::builder()
        .hexpand(true)
        .vexpand(true)
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    stack.add_titled(&general_page(&widgets), Some("general"), "General");
    stack.add_titled(&backend_page(&widgets), Some("backend"), "Backend");
    stack.add_titled(&models_page(&widgets), Some("models"), "Models");
    stack.add_titled(&language_page(&widgets), Some("language"), "Language");
    stack.add_titled(&overlay_page(&widgets), Some("overlay"), "Overlay");
    stack.add_titled(
        &diagnostics_page(&widgets),
        Some("diagnostics"),
        "Diagnostics",
    );

    let sidebar = StackSidebar::builder()
        .stack(&stack)
        .width_request(150)
        .vexpand(true)
        .build();

    content.append(&sidebar);
    content.append(&stack);

    let status = Label::builder()
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    status.set_text(
        &config_path()
            .map(|path| format!("Config: {}", path.display()))
            .unwrap_or_else(|_| "Config path unavailable".to_string()),
    );

    let revert_button = Button::with_label("Revert");
    let save_button = Button::with_label("Save");
    let actions = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    actions.append(&revert_button);
    actions.append(&save_button);

    let footer = gtk::Box::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .build();
    footer.append(&status);
    footer.append(&actions);

    root.append(&content);
    root.append(&footer);

    let widgets_for_save = widgets.clone();
    let status_for_save = status.clone();
    save_button.connect_clicked(move |_| {
        let config = widgets_for_save.to_config();
        match save_config(&config) {
            Ok(path) => status_for_save.set_text(&format!("Saved: {}", path.display())),
            Err(e) => status_for_save.set_text(&format!("Save failed: {e:#}")),
        }
    });

    let widgets_for_revert = widgets.clone();
    let status_for_revert = status.clone();
    revert_button.connect_clicked(move |_| match load_config() {
        Ok(config) => {
            widgets_for_revert.apply_config(&config);
            status_for_revert.set_text("Reverted to saved config");
        }
        Err(e) => status_for_revert.set_text(&format!("Revert failed: {e:#}")),
    });

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Whisper Overlay Settings")
        .default_width(880)
        .default_height(620)
        .child(&root)
        .build();

    window.present();
}

#[derive(Clone)]
struct SettingsWidgets {
    address: Entry,
    hotkey: Entry,
    style: Entry,
    type_field: ComboBoxText,
    audio_source_kind: ComboBoxText,
    audio_source: ComboBoxText,
    capture_mode: ComboBoxText,
    caption_finalize_interval: SpinButton,
    type_into_focused_app: CheckButton,
    backend: ComboBoxText,
    task: ComboBoxText,
    realtime_device: ComboBoxText,
    realtime_model: Entry,
    realtime_model_realtime: Entry,
    onnx_provider: ComboBoxText,
    onnx_compute_type: ComboBoxText,
    onnx_model: Entry,
    model_source: ComboBoxText,
    custom_model: Entry,
    cache_dir: Entry,
    catalog_url: Entry,
    source_language: Entry,
    target_language: Entry,
    overlay_anchor: ComboBoxText,
    overlay_bottom_margin: SpinButton,
    overlay_width: SpinButton,
    overlay_history: SpinButton,
    confidence_colors: CheckButton,
    plain_text_fallback: CheckButton,
}

impl SettingsWidgets {
    fn new(config: &AppConfig) -> Self {
        let source_kind = AudioSourceKind::parse(&config.audio.source_kind)
            .unwrap_or(AudioSourceKind::Microphone);
        let widgets = Self {
            address: Entry::new(),
            hotkey: Entry::new(),
            style: Entry::new(),
            type_field: combo(&["text", "source_text", "translated_text"]),
            audio_source_kind: labeled_combo(&[
                ("microphone", "Microphone"),
                ("desktop-output", "Desktop audio"),
                ("output-device", "Speaker/output"),
                ("application", "Application audio"),
            ]),
            audio_source: audio_source_combo(&source_kind, &config.audio.source_id),
            capture_mode: combo(&[
                "push-to-talk",
                "toggle-live-caption",
                "always-on-live-caption",
            ]),
            caption_finalize_interval: spin(0.0, 30.0, 1.0),
            type_into_focused_app: CheckButton::new(),
            backend: combo(&["realtime-stt", "onnx"]),
            task: combo(&["transcribe", "translate"]),
            realtime_device: combo(&["auto", "cpu", "cuda"]),
            realtime_model: Entry::new(),
            realtime_model_realtime: Entry::new(),
            onnx_provider: combo(&["auto", "cpu", "cuda", "tensorrt", "rocm", "openvino"]),
            onnx_compute_type: combo(&["auto", "fp32", "fp16", "int8"]),
            onnx_model: Entry::new(),
            model_source: combo(&["builtin", "catalog", "local_path", "manual_id"]),
            custom_model: Entry::new(),
            cache_dir: Entry::new(),
            catalog_url: Entry::new(),
            source_language: Entry::new(),
            target_language: Entry::new(),
            overlay_anchor: combo(&["bottom", "top"]),
            overlay_bottom_margin: spin(0.0, 1000.0, 10.0),
            overlay_width: spin(400.0, 4000.0, 100.0),
            overlay_history: spin(0.5, 60.0, 0.5),
            confidence_colors: CheckButton::new(),
            plain_text_fallback: CheckButton::new(),
        };
        let source_combo = widgets.audio_source.clone();
        widgets
            .audio_source_kind
            .connect_changed(move |kind_combo| {
                let source_kind = AudioSourceKind::parse(&combo_text(kind_combo))
                    .unwrap_or(AudioSourceKind::Microphone);
                refresh_audio_source_combo(&source_combo, &source_kind, "default");
            });
        widgets.apply_config(config);
        widgets
    }

    fn apply_config(&self, config: &AppConfig) {
        self.address.set_text(&config.client.address);
        self.hotkey.set_text(&config.client.hotkey);
        self.style.set_text(&config.client.style);
        set_combo(&self.type_field, &config.client.type_field);
        set_combo(&self.audio_source_kind, &config.audio.source_kind);
        let source_kind = AudioSourceKind::parse(&config.audio.source_kind)
            .unwrap_or(AudioSourceKind::Microphone);
        refresh_audio_source_combo(&self.audio_source, &source_kind, &config.audio.source_id);
        set_combo(&self.audio_source, &config.audio.source_id);
        set_combo(&self.capture_mode, &config.audio.capture_mode);
        self.caption_finalize_interval
            .set_value(config.caption.finalize_interval_seconds);
        self.type_into_focused_app
            .set_active(config.caption.type_into_focused_app);
        set_combo(&self.backend, &config.server.backend);
        set_combo(&self.task, &config.server.task);
        set_combo(&self.realtime_device, &config.realtime_stt.device);
        self.realtime_model.set_text(&config.realtime_stt.model);
        self.realtime_model_realtime
            .set_text(&config.realtime_stt.model_realtime);
        set_combo(&self.onnx_provider, &config.onnx.provider);
        set_combo(&self.onnx_compute_type, &config.onnx.compute_type);
        self.onnx_model.set_text(&config.onnx.model);
        set_combo(&self.model_source, &config.onnx.model_source);
        self.custom_model.set_text(&config.onnx.custom_model);
        self.cache_dir.set_text(&config.models.cache_dir);
        self.catalog_url.set_text(&config.models.catalog_url);
        self.source_language.set_text(&config.server.language);
        self.target_language
            .set_text(&config.server.target_language);
        set_combo(&self.overlay_anchor, &config.overlay.anchor);
        self.overlay_bottom_margin
            .set_value(config.overlay.bottom_margin.into());
        self.overlay_width.set_value(config.overlay.width.into());
        self.overlay_history
            .set_value(config.overlay.keep_history_seconds);
        self.confidence_colors
            .set_active(config.overlay.confidence_colors);
        self.plain_text_fallback
            .set_active(config.overlay.plain_text_fallback);
    }

    fn to_config(&self) -> AppConfig {
        AppConfig {
            client: crate::config::ClientConfig {
                address: self.address.text().to_string(),
                hotkey: self.hotkey.text().to_string(),
                style: self.style.text().to_string(),
                type_field: combo_text(&self.type_field),
            },
            audio: crate::config::AudioConfig {
                source_kind: combo_text(&self.audio_source_kind),
                source_id: combo_text(&self.audio_source),
                capture_mode: combo_text(&self.capture_mode),
                ..crate::config::AudioConfig::default()
            },
            caption: crate::config::CaptionConfig {
                type_into_focused_app: self.type_into_focused_app.is_active(),
                finalize_interval_seconds: self.caption_finalize_interval.value(),
                ..crate::config::CaptionConfig::default()
            },
            server: crate::config::ServerConfig {
                backend: combo_text(&self.backend),
                host: self
                    .address
                    .text()
                    .split(':')
                    .next()
                    .unwrap_or("localhost")
                    .to_string(),
                port: self
                    .address
                    .text()
                    .split(':')
                    .nth(1)
                    .and_then(|port| port.parse().ok())
                    .unwrap_or(7007),
                language: self.source_language.text().to_string(),
                task: combo_text(&self.task),
                target_language: self.target_language.text().to_string(),
            },
            realtime_stt: crate::config::RealtimeSttConfig {
                device: combo_text(&self.realtime_device),
                model: self.realtime_model.text().to_string(),
                model_realtime: self.realtime_model_realtime.text().to_string(),
                model_source: "builtin".to_string(),
                custom_model: String::new(),
            },
            onnx: crate::config::OnnxConfig {
                model: self.onnx_model.text().to_string(),
                provider: combo_text(&self.onnx_provider),
                device: "auto".to_string(),
                compute_type: combo_text(&self.onnx_compute_type),
                model_source: combo_text(&self.model_source),
                custom_model: self.custom_model.text().to_string(),
            },
            models: crate::config::ModelsConfig {
                cache_dir: self.cache_dir.text().to_string(),
                catalog_url: self.catalog_url.text().to_string(),
            },
            overlay: crate::config::OverlayConfig {
                anchor: combo_text(&self.overlay_anchor),
                bottom_margin: self.overlay_bottom_margin.value_as_int(),
                width: self.overlay_width.value_as_int(),
                keep_history_seconds: self.overlay_history.value(),
                confidence_colors: self.confidence_colors.is_active(),
                plain_text_fallback: self.plain_text_fallback.is_active(),
            },
        }
    }
}

fn general_page(widgets: &SettingsWidgets) -> ScrolledWindow {
    let grid = form_grid();
    add_entry_row(&grid, 0, "Server address", &widgets.address);
    add_entry_row(&grid, 1, "Hotkey", &widgets.hotkey);
    add_combo_row(&grid, 2, "Text to insert", &widgets.type_field);
    add_combo_row(&grid, 3, "Audio input", &widgets.audio_source_kind);
    add_combo_row(&grid, 4, "Device or app", &widgets.audio_source);
    add_combo_row(&grid, 5, "Capture mode", &widgets.capture_mode);
    add_spin_row(
        &grid,
        6,
        "Finalize captions every",
        &widgets.caption_finalize_interval,
    );
    add_check_row(
        &grid,
        7,
        "Type live captions into focused app",
        &widgets.type_into_focused_app,
    );
    add_entry_row(&grid, 8, "Style file", &widgets.style);
    add_note(
        &grid,
        9,
        "Device choices are filtered by the selected audio input. Application audio needs a PipeWire/PulseAudio stream backend and may not appear yet.",
    );
    scroll(grid)
}

fn backend_page(widgets: &SettingsWidgets) -> ScrolledWindow {
    let grid = form_grid();
    add_combo_row(&grid, 0, "Backend", &widgets.backend);
    add_combo_row(&grid, 1, "Task", &widgets.task);
    add_combo_row(&grid, 2, "RealtimeSTT device", &widgets.realtime_device);
    add_entry_row(&grid, 3, "RealtimeSTT model", &widgets.realtime_model);
    add_entry_row(
        &grid,
        4,
        "RealtimeSTT live model",
        &widgets.realtime_model_realtime,
    );
    add_combo_row(&grid, 5, "ONNX provider", &widgets.onnx_provider);
    add_combo_row(&grid, 6, "ONNX compute type", &widgets.onnx_compute_type);
    add_note(
        &grid,
        7,
        "Provider availability is reported by the server diagnostics when connected.",
    );
    scroll(grid)
}

fn models_page(widgets: &SettingsWidgets) -> ScrolledWindow {
    let grid = form_grid();
    add_combo_row(&grid, 0, "Model source", &widgets.model_source);
    add_entry_row(&grid, 1, "ONNX model id/path", &widgets.onnx_model);
    add_entry_row(&grid, 2, "Custom model id/path", &widgets.custom_model);
    add_entry_row(&grid, 3, "Model cache directory", &widgets.cache_dir);
    add_entry_row(&grid, 4, "Catalog URL", &widgets.catalog_url);
    add_note(
        &grid,
        5,
        "Catalog download/install UI will be added after the persisted settings path is stable.",
    );
    scroll(grid)
}

fn language_page(widgets: &SettingsWidgets) -> ScrolledWindow {
    let grid = form_grid();
    add_entry_row(&grid, 0, "Source language", &widgets.source_language);
    add_entry_row(&grid, 1, "Target language", &widgets.target_language);
    add_readonly_row(&grid, 2, "Task", "Configured on the Backend page");
    add_readonly_row(&grid, 3, "Type into apps", "Configured on the General page");
    add_note(
        &grid,
        4,
        "Whisper translation currently targets English unless another translation backend is added.",
    );
    scroll(grid)
}

fn overlay_page(widgets: &SettingsWidgets) -> ScrolledWindow {
    let grid = form_grid();
    add_combo_row(&grid, 0, "Anchor", &widgets.overlay_anchor);
    add_spin_row(&grid, 1, "Bottom margin", &widgets.overlay_bottom_margin);
    add_spin_row(&grid, 2, "Width", &widgets.overlay_width);
    add_spin_row(&grid, 3, "Keep history seconds", &widgets.overlay_history);
    add_check_row(&grid, 4, "Confidence colors", &widgets.confidence_colors);
    add_check_row(
        &grid,
        5,
        "Plain-text fallback",
        &widgets.plain_text_fallback,
    );
    scroll(grid)
}

fn diagnostics_page(widgets: &SettingsWidgets) -> ScrolledWindow {
    let grid = form_grid();
    add_readonly_row(&grid, 0, "Server status", "Not connected from settings UI");
    add_readonly_row(&grid, 1, "Backend", &combo_text(&widgets.backend));
    add_readonly_row(
        &grid,
        2,
        "ONNX provider",
        &combo_text(&widgets.onnx_provider),
    );
    add_readonly_row(&grid, 3, "Model", &widgets.onnx_model.text());
    add_note(
        &grid,
        4,
        "Live server capability probing will be added through the status protocol.",
    );
    scroll(grid)
}

fn form_grid() -> Grid {
    Grid::builder()
        .column_spacing(16)
        .row_spacing(10)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build()
}

fn scroll(grid: Grid) -> ScrolledWindow {
    ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .child(&grid)
        .build()
}

fn label(text: &str) -> Label {
    Label::builder()
        .label(text)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .build()
}

fn add_entry_row(grid: &Grid, row: i32, name: &str, entry: &Entry) {
    entry.set_hexpand(true);
    grid.attach(&label(name), 0, row, 1, 1);
    grid.attach(entry, 1, row, 1, 1);
}

fn add_combo_row(grid: &Grid, row: i32, name: &str, combo: &ComboBoxText) {
    combo.set_hexpand(true);
    grid.attach(&label(name), 0, row, 1, 1);
    grid.attach(combo, 1, row, 1, 1);
}

fn add_spin_row(grid: &Grid, row: i32, name: &str, spin: &SpinButton) {
    spin.set_hexpand(true);
    grid.attach(&label(name), 0, row, 1, 1);
    grid.attach(spin, 1, row, 1, 1);
}

fn add_check_row(grid: &Grid, row: i32, name: &str, check: &CheckButton) {
    grid.attach(&label(name), 0, row, 1, 1);
    grid.attach(check, 1, row, 1, 1);
}

fn add_readonly_row(grid: &Grid, row: i32, name: &str, value: &str) {
    let value = Label::builder()
        .label(value)
        .selectable(true)
        .halign(gtk::Align::Start)
        .build();
    grid.attach(&label(name), 0, row, 1, 1);
    grid.attach(&value, 1, row, 1, 1);
}

fn add_note(grid: &Grid, row: i32, text: &str) {
    let note = Label::builder()
        .label(text)
        .wrap(true)
        .halign(gtk::Align::Start)
        .build();
    grid.attach(&note, 0, row, 2, 1);
}

fn combo(values: &[&str]) -> ComboBoxText {
    let combo = ComboBoxText::new();
    for value in values {
        combo.append(Some(value), value);
    }
    combo.set_active(Some(0));
    combo
}

fn labeled_combo(values: &[(&str, &str)]) -> ComboBoxText {
    let combo = ComboBoxText::new();
    for (id, label) in values {
        combo.append(Some(id), label);
    }
    combo.set_active(Some(0));
    combo
}

fn audio_source_combo(source_kind: &AudioSourceKind, current_source_id: &str) -> ComboBoxText {
    let combo = ComboBoxText::new();
    refresh_audio_source_combo(&combo, source_kind, current_source_id);
    combo
}

fn refresh_audio_source_combo(
    combo: &ComboBoxText,
    source_kind: &AudioSourceKind,
    current_source_id: &str,
) {
    combo.remove_all();
    combo.append(Some("default"), default_audio_source_label(source_kind));

    let mut matched_count = 0;
    match list_audio_sources() {
        Ok(sources) => {
            for source in sources {
                if !audio_source_matches(source_kind, &source.kind) {
                    continue;
                }

                let label = format!(
                    "{}: {}{}",
                    audio_source_kind_label(&source.kind),
                    source.name,
                    if source.is_default { " (default)" } else { "" }
                );
                combo.append(Some(&source.id), &label);
                matched_count += 1;
            }
        }
        Err(err) => {
            eprintln!("warning: could not load audio source list for settings: {err:#}");
        }
    }

    if !current_source_id.is_empty() && combo.set_active_id(Some(current_source_id)) {
        return;
    }

    if !current_source_id.is_empty() && current_source_id != "default" {
        combo.append(
            Some(current_source_id),
            &format!("Manual: {current_source_id}"),
        );
        combo.set_active_id(Some(current_source_id));
    } else {
        combo.set_active(Some(0));
    }

    if matched_count == 0
        && matches!(
            source_kind,
            AudioSourceKind::DesktopOutput
                | AudioSourceKind::OutputDevice
                | AudioSourceKind::Application
        )
    {
        combo.append(Some("unsupported"), "No matching source found");
    }
}

fn audio_source_matches(selected: &AudioSourceKind, actual: &AudioSourceKind) -> bool {
    match selected {
        AudioSourceKind::Microphone => actual == &AudioSourceKind::Microphone,
        AudioSourceKind::DesktopOutput => actual == &AudioSourceKind::DesktopOutput,
        AudioSourceKind::OutputDevice => actual == &AudioSourceKind::OutputDevice,
        AudioSourceKind::Application => actual == &AudioSourceKind::Application,
    }
}

fn default_audio_source_label(source_kind: &AudioSourceKind) -> &'static str {
    match source_kind {
        AudioSourceKind::Microphone => "Default microphone",
        AudioSourceKind::DesktopOutput => "Default desktop audio",
        AudioSourceKind::OutputDevice => "Default output device",
        AudioSourceKind::Application => "Default application",
    }
}

fn audio_source_kind_label(source_kind: &AudioSourceKind) -> &'static str {
    match source_kind {
        AudioSourceKind::Microphone => "Microphone",
        AudioSourceKind::DesktopOutput => "Desktop audio",
        AudioSourceKind::OutputDevice => "Output",
        AudioSourceKind::Application => "Application",
    }
}

fn set_combo(combo: &ComboBoxText, value: &str) {
    if !value.is_empty() && combo.set_active_id(Some(value)) {
        return;
    }
    combo.set_active(Some(0));
}

fn combo_text(combo: &ComboBoxText) -> String {
    combo
        .active_id()
        .map(|text| text.to_string())
        .unwrap_or_default()
}

fn spin(min: f64, max: f64, step: f64) -> SpinButton {
    let adjustment = Adjustment::new(min, min, max, step, step * 10.0, 0.0);
    SpinButton::new(Some(&adjustment), step, 1)
}
