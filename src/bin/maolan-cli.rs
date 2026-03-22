#[path = "../audio_defaults.rs"]
mod audio_defaults;
#[path = "../cli/mod.rs"]
mod cli;

use cli::export::{
    EXPORT_MP3_BITRATES_KBPS, EXPORT_MP3_MODE_ALL, EXPORT_NORMALIZE_MODE_ALL,
    EXPORT_RENDER_MODE_ALL, ExportBitDepth, ExportFormat, ExportNormalizeMode, ExportRenderMode,
    ExportSettings, STANDARD_EXPORT_SAMPLE_RATES, default_export_base_path,
    export_bit_depth_options, export_session, validate_export_settings,
};
use cli::support::{
    CliConfig, ExportSessionData, load_export_session_data, load_session_end_sample,
    load_session_restore_actions,
};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use maolan_engine::{
    client::Client,
    message::{Action, Message as EngineMessage},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Flex, Layout, Rect},
    prelude::Stylize,
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::{
    collections::BTreeSet,
    io,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::{Receiver, UnboundedReceiver, UnboundedSender, unbounded_channel};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    session_dir: Option<PathBuf>,
    device: Option<String>,
    input_device: Option<String>,
    sample_rate_hz: i32,
    bits: usize,
    period_frames: usize,
    nperiods: usize,
    exclusive: bool,
    sync_mode: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            session_dir: None,
            device: None,
            input_device: None,
            sample_rate_hz: audio_defaults::SAMPLE_RATE_HZ,
            bits: audio_defaults::BIT_DEPTH,
            period_frames: audio_defaults::PERIOD_FRAMES,
            nperiods: audio_defaults::NPERIODS,
            exclusive: false,
            sync_mode: audio_defaults::SYNC_MODE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppCommand {
    TogglePlayStop,
    Pause,
    JumpToStart,
    JumpToEnd,
    Panic,
    ToggleExport,
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    Activate,
    Back,
    Quit,
    None,
}

#[derive(Debug)]
enum ExportEvent {
    Progress {
        progress: f32,
        operation: Option<String>,
    },
    Finished(Result<Vec<PathBuf>, String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExportField {
    FormatWav,
    FormatMp3,
    FormatOgg,
    FormatFlac,
    SampleRate,
    BitDepth,
    Mp3Mode,
    Mp3Bitrate,
    OggQuality,
    RenderMode,
    HwOutPort(usize),
    RealtimeFallback,
    MasterLimiter,
    MasterLimiterCeiling,
    Normalize,
    NormalizeMode,
    NormalizeDbfs,
    NormalizeLufs,
    NormalizeDbtp,
    NormalizeLimiter,
    ExportNow,
    Cancel,
}

#[derive(Debug, Clone)]
struct ExportUiState {
    session: ExportSessionData,
    settings: ExportSettings,
    selected_index: usize,
}

#[derive(Debug)]
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(stdout, cursor::Show, LeaveAlternateScreen);
    }
}

#[derive(Debug)]
struct SessionDiagnostics {
    track_count: usize,
    audio_clip_count: usize,
    midi_clip_count: usize,
    pending_requests: usize,
    workers_ready: usize,
    transport_playing: bool,
}

#[derive(Debug)]
struct App {
    session_dir: Option<PathBuf>,
    playing: bool,
    paused: bool,
    transport_sample: usize,
    hw_ready: bool,
    input_channels: usize,
    output_channels: usize,
    sample_rate_hz: usize,
    status: String,
    open_audio_action: Option<Action>,
    sticky_status: bool,
    diagnostics: Option<SessionDiagnostics>,
    hw_out_db: Vec<f32>,
    track_meters: Vec<(String, Vec<f32>)>,
    last_meter_request: Option<Instant>,
    export_ui: Option<ExportUiState>,
    export_in_progress: bool,
    export_progress: f32,
    export_operation: Option<String>,
    default_export_sample_rate_hz: u32,
}

impl App {
    fn new(
        session_dir: Option<PathBuf>,
        open_audio_action: Option<Action>,
        status: String,
        default_export_sample_rate_hz: u32,
    ) -> Self {
        Self {
            session_dir,
            playing: false,
            paused: false,
            transport_sample: 0,
            hw_ready: false,
            input_channels: 0,
            output_channels: 0,
            sample_rate_hz: 0,
            status,
            open_audio_action,
            sticky_status: false,
            diagnostics: None,
            hw_out_db: Vec::new(),
            track_meters: Vec::new(),
            last_meter_request: None,
            export_ui: None,
            export_in_progress: false,
            export_progress: 0.0,
            export_operation: None,
            default_export_sample_rate_hz,
        }
    }

    async fn open_audio(&mut self, client: &Client) {
        if let Some(action) = self.open_audio_action.clone() {
            self.status = "Opening audio device...".to_string();
            if let Err(err) = client.send(EngineMessage::Request(action)).await {
                self.status = format!("Failed to send open-audio request: {err}");
                self.sticky_status = true;
            }
        }
    }

    async fn restore_session(&mut self, client: &Client, session_dir: Option<&PathBuf>) {
        let Some(session_dir) = session_dir else {
            return;
        };
        match load_session_restore_actions(session_dir) {
            Ok(actions) => {
                self.status = format!("Restoring session '{}'", session_dir.display());
                for action in actions {
                    if let Err(err) = client.send(EngineMessage::Request(action)).await {
                        self.status = format!("Failed to send session restore action: {err}");
                        self.playing = false;
                        self.paused = false;
                        self.sticky_status = true;
                        break;
                    }
                }
                let _ = client
                    .send(EngineMessage::Request(Action::RequestSessionDiagnostics))
                    .await;
            }
            Err(err) => {
                self.status = err;
                self.sticky_status = true;
            }
        }
    }

    async fn handle_command(
        &mut self,
        client: &Client,
        command: AppCommand,
        export_tx: &UnboundedSender<ExportEvent>,
    ) -> bool {
        if self.export_ui.is_some() {
            return self.handle_export_command(command, export_tx);
        }

        match command {
            AppCommand::TogglePlayStop => {
                if !self.hw_ready {
                    self.status =
                        "Audio is not ready. Set a default device in config or pass --device."
                            .to_string();
                    return true;
                }
                if self.playing && !self.paused {
                    self.playing = false;
                    self.paused = false;
                    self.status = "Stopped".to_string();
                    if let Err(err) = send_transport_stop(client).await {
                        self.status = err;
                        self.sticky_status = true;
                    }
                } else {
                    let was_playing = self.playing;
                    self.playing = true;
                    self.paused = false;
                    self.status = "Playing".to_string();
                    if let Err(err) = send_transport_play(client, was_playing).await {
                        self.status = err;
                        self.sticky_status = true;
                    }
                }
                let _ = client
                    .send(EngineMessage::Request(Action::RequestSessionDiagnostics))
                    .await;
                true
            }
            AppCommand::Pause => {
                if !self.hw_ready {
                    self.status =
                        "Audio is not ready. Set a default device in config or pass --device."
                            .to_string();
                    return true;
                }
                let was_playing = self.playing;
                self.playing = true;
                self.paused = true;
                self.status = "Paused".to_string();
                if let Err(err) = send_transport_pause(client, was_playing).await {
                    self.status = err;
                    self.sticky_status = true;
                }
                let _ = client
                    .send(EngineMessage::Request(Action::RequestSessionDiagnostics))
                    .await;
                true
            }
            AppCommand::JumpToStart => {
                self.transport_sample = 0;
                self.status = "Rewound to start".to_string();
                if let Err(err) = send_transport_position(client, 0).await {
                    self.status = err;
                    self.sticky_status = true;
                }
                let _ = client
                    .send(EngineMessage::Request(Action::RequestSessionDiagnostics))
                    .await;
                true
            }
            AppCommand::JumpToEnd => {
                let Some(session_dir) = self.session_dir.as_ref() else {
                    self.status = "Jump to end requires an opened/saved session".to_string();
                    return true;
                };
                match load_session_end_sample(session_dir) {
                    Ok(sample) => {
                        self.transport_sample = sample;
                        self.status = "Jumped to end".to_string();
                        if let Err(err) = send_transport_position(client, sample).await {
                            self.status = err;
                            self.sticky_status = true;
                        }
                    }
                    Err(err) => {
                        self.status = err;
                        self.sticky_status = true;
                    }
                }
                let _ = client
                    .send(EngineMessage::Request(Action::RequestSessionDiagnostics))
                    .await;
                true
            }
            AppCommand::Panic => {
                self.status = "Sent panic".to_string();
                if let Err(err) = send_transport_panic(client).await {
                    self.status = err;
                    self.sticky_status = true;
                }
                let _ = client
                    .send(EngineMessage::Request(Action::RequestSessionDiagnostics))
                    .await;
                true
            }
            AppCommand::ToggleExport => {
                self.open_export_ui();
                true
            }
            AppCommand::Quit => false,
            _ => true,
        }
    }

    fn open_export_ui(&mut self) {
        if self.export_in_progress {
            self.status = "Export already in progress".to_string();
            return;
        }
        let Some(session_dir) = self.session_dir.as_ref() else {
            self.status = "Export requires an opened/saved session".to_string();
            return;
        };
        match load_export_session_data(session_dir) {
            Ok(session) => {
                let mut settings =
                    ExportSettings::new(self.default_export_sample_rate_hz, self.output_channels);
                settings.normalize_hw_out_ports(self.output_channels);
                self.export_ui = Some(ExportUiState {
                    session,
                    settings,
                    selected_index: 0,
                });
                self.status = format!(
                    "Export panel open: {}",
                    default_export_base_path(session_dir).display()
                );
            }
            Err(err) => {
                self.status = err;
                self.sticky_status = true;
            }
        }
    }

    fn handle_export_command(
        &mut self,
        command: AppCommand,
        export_tx: &UnboundedSender<ExportEvent>,
    ) -> bool {
        let visible_fields = self.visible_export_fields();
        let Some(export_ui) = self.export_ui.as_mut() else {
            return true;
        };

        match command {
            AppCommand::Quit => false,
            AppCommand::Back | AppCommand::ToggleExport => {
                self.export_ui = None;
                self.status = "Export panel closed".to_string();
                true
            }
            AppCommand::MoveUp => {
                export_ui.selected_index = export_ui.selected_index.saturating_sub(1);
                true
            }
            AppCommand::MoveDown => {
                export_ui.selected_index =
                    (export_ui.selected_index + 1).min(visible_fields.len().saturating_sub(1));
                true
            }
            AppCommand::MoveLeft => {
                self.adjust_export_field(-1);
                true
            }
            AppCommand::MoveRight => {
                self.adjust_export_field(1);
                true
            }
            AppCommand::Activate
            | AppCommand::TogglePlayStop
            | AppCommand::Pause
            | AppCommand::JumpToStart
            | AppCommand::JumpToEnd
            | AppCommand::Panic => {
                self.activate_export_field(export_tx);
                true
            }
            AppCommand::None => true,
        }
    }

    fn handle_export_event(&mut self, event: ExportEvent) {
        match event {
            ExportEvent::Progress {
                progress,
                operation,
            } => {
                self.export_in_progress = true;
                self.export_progress = progress;
                self.export_operation = operation.clone();
                self.status = format!(
                    "Exporting ({}%): {}",
                    (progress * 100.0).round() as u16,
                    operation.unwrap_or_else(|| "Working".to_string())
                );
                self.sticky_status = true;
            }
            ExportEvent::Finished(result) => {
                self.export_in_progress = false;
                self.export_progress = 0.0;
                self.export_operation = None;
                match result {
                    Ok(paths) => {
                        let target = paths
                            .first()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "export".to_string());
                        self.status = format!("Export complete: {target}");
                        self.export_ui = None;
                    }
                    Err(err) => {
                        self.status = err;
                    }
                }
                self.sticky_status = true;
            }
        }
    }

    fn handle_engine_message(&mut self, message: EngineMessage) {
        match message {
            EngineMessage::Response(Ok(Action::HWInfo {
                channels,
                rate,
                input,
            })) => {
                self.hw_ready = true;
                self.sample_rate_hz = rate;
                if input {
                    self.input_channels = channels;
                } else {
                    self.output_channels = channels;
                    if let Some(export_ui) = self.export_ui.as_mut() {
                        export_ui
                            .settings
                            .normalize_hw_out_ports(self.output_channels);
                    }
                }
                if !self.sticky_status {
                    self.status = format!(
                        "Audio ready: {} in / {} out @ {} Hz",
                        self.input_channels, self.output_channels, self.sample_rate_hz
                    );
                }
            }
            EngineMessage::Response(Ok(Action::TransportPosition(sample))) => {
                self.transport_sample = sample;
            }
            EngineMessage::Response(Ok(Action::Play)) => {
                self.playing = true;
                self.paused = false;
            }
            EngineMessage::Response(Ok(Action::Pause)) => {
                self.playing = true;
                self.paused = true;
            }
            EngineMessage::Response(Ok(Action::Stop)) => {
                self.playing = false;
                self.paused = false;
            }
            EngineMessage::Response(Ok(Action::MeterSnapshot {
                hw_out_db,
                track_meters,
            })) => {
                self.hw_out_db = hw_out_db.as_ref().clone();
                self.track_meters = track_meters.as_ref().clone();
            }
            EngineMessage::Response(Ok(Action::SessionDiagnosticsReport {
                track_count,
                audio_clip_count,
                midi_clip_count,
                pending_requests,
                workers_ready,
                playing,
                transport_sample,
                ..
            })) => {
                self.diagnostics = Some(SessionDiagnostics {
                    track_count,
                    audio_clip_count,
                    midi_clip_count,
                    pending_requests,
                    workers_ready,
                    transport_playing: playing,
                });
                self.status = format!(
                    "Session loaded: {track_count} tracks, {audio_clip_count} audio clips, {midi_clip_count} MIDI clips, workers ready {workers_ready}, pending {pending_requests}, playing {playing}, transport {transport_sample}"
                );
                self.sticky_status = true;
            }
            EngineMessage::Response(Err(err)) => {
                self.status = err;
                self.playing = false;
                self.paused = false;
                self.sticky_status = true;
            }
            EngineMessage::OfflineBounceFinished { result: Err(err) } => {
                self.status = err;
                self.sticky_status = true;
            }
            _ => {}
        }
    }

    fn draw(&self, terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
        let state = transport_state_label(self.playing, self.paused);
        let state_color = transport_state_color(self.playing, self.paused);
        let device_hint = match &self.open_audio_action {
            Some(Action::OpenAudioDevice { device, .. }) => device.as_str(),
            _ => "not configured",
        };
        let audio_status = if self.hw_ready {
            format!(
                "{} in / {} out @ {} Hz",
                self.input_channels, self.output_channels, self.sample_rate_hz
            )
        } else {
            format!("not ready ({device_hint})")
        };

        terminal.draw(|frame| {
            let area = frame.area();
            let sections = Layout::vertical([
                Constraint::Length(4),
                Constraint::Length(8),
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Min(10),
            ])
            .margin(1)
            .split(area);

            let top = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .split(sections[1]);
            let middle =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(sections[2]);
            let track_meter_columns = self.track_meter_columns();
            let hw_meter_columns = self.hw_meter_columns();
            let meter_row = Layout::horizontal([
                Constraint::Length(meter_panel_width(track_meter_columns.len())),
                Constraint::Min(0),
                Constraint::Length(meter_panel_width(hw_meter_columns.len())),
            ])
            .split(sections[4]);

            let header = Paragraph::new(Text::from(vec![
                Line::from("maolan-cli").style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Line::from(self.session_dir_line()),
                Line::from(format!("Last: {}", self.status)),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Overview"));

            let transport = Paragraph::new(Text::from(vec![
                Line::from(vec![
                    "State: ".into(),
                    state
                        .to_string()
                        .fg(state_color)
                        .add_modifier(Modifier::BOLD),
                ]),
                Line::from(format!(
                    "Clock: {}",
                    format_transport_clock(self.transport_sample, self.sample_rate_hz)
                )),
                Line::from(format!("Transport sample: {}", self.transport_sample)),
                Line::from(format!("Engine playing: {}", self.engine_playing_label())),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Transport"));

            let audio = Paragraph::new(Text::from(vec![
                Line::from(format!("Audio: {audio_status}")),
                Line::from(format!("Output device: {}", self.output_device_label())),
                Line::from(format!("Input device: {}", self.input_device_label())),
                Line::from(format!(
                    "I/O: {} in / {} out",
                    self.input_channels, self.output_channels
                )),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Audio"));

            let diagnostics = Paragraph::new(Text::from(vec![
                Line::from(format!("Tracks: {}", self.track_count())),
                Line::from(format!("Audio clips: {}", self.audio_clip_count())),
                Line::from(format!("MIDI clips: {}", self.midi_clip_count())),
                Line::from(format!("Pending requests: {}", self.pending_requests())),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Session"));

            let workers = Paragraph::new(Text::from(vec![
                Line::from(format!("Workers ready: {}", self.workers_ready())),
                Line::from(format!("CLI transport: {state}")),
                Line::from("Refreshes from engine diagnostics"),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Engine"));

            let track_meters = Paragraph::new(vertical_meter_lines(&track_meter_columns))
                .block(Block::default().borders(Borders::ALL).title("Tracks"))
                .wrap(ratatui::widgets::Wrap { trim: true });

            let hw_meters = Paragraph::new(vertical_meter_lines(&hw_meter_columns))
                .block(Block::default().borders(Borders::ALL).title("HW"))
                .wrap(ratatui::widgets::Wrap { trim: true });

            let keys = Paragraph::new(Text::from(vec![
                Line::from("Space      play / stop"),
                Line::from("Shift+Space pause"),
                Line::from("Home       rewind to start"),
                Line::from("End        rewind to end"),
                Line::from("Ctrl+L     panic"),
                Line::from("Ctrl+E     export"),
                Line::from("q or Esc   quit"),
            ]))
            .block(Block::default().borders(Borders::ALL).title("Keys"));

            frame.render_widget(header, sections[0]);
            frame.render_widget(transport, top[0]);
            frame.render_widget(audio, top[1]);
            frame.render_widget(diagnostics, middle[0]);
            frame.render_widget(workers, middle[1]);
            frame.render_widget(keys, sections[3]);
            frame.render_widget(track_meters, meter_row[0]);
            frame.render_widget(hw_meters, meter_row[2]);
            if let Some(export_ui) = &self.export_ui {
                let popup = centered_rect(area, 80, 80);
                let export_scroll = export_panel_scroll(
                    popup.height.saturating_sub(2) as usize,
                    export_ui.selected_index,
                    self.visible_export_fields().len(),
                    self.export_in_progress,
                );
                frame.render_widget(Clear, popup);
                frame.render_widget(
                    Paragraph::new(self.export_panel_lines(export_ui))
                        .block(Block::default().borders(Borders::ALL).title("Export"))
                        .scroll((export_scroll as u16, 0))
                        .wrap(ratatui::widgets::Wrap { trim: false }),
                    popup,
                );
            }
        })?;

        Ok(())
    }

    fn session_dir_line(&self) -> String {
        self.session_dir
            .as_ref()
            .map(|path| format!("Session: {}", path.display()))
            .unwrap_or_else(|| "Session: not set".to_string())
    }

    fn output_device_label(&self) -> &str {
        match &self.open_audio_action {
            Some(Action::OpenAudioDevice { device, .. }) => device.as_str(),
            _ => "not configured",
        }
    }

    fn input_device_label(&self) -> &str {
        match &self.open_audio_action {
            Some(Action::OpenAudioDevice {
                input_device: Some(device),
                ..
            }) => device.as_str(),
            _ => "default / none",
        }
    }

    fn track_count(&self) -> usize {
        self.diagnostics
            .as_ref()
            .map(|d| d.track_count)
            .unwrap_or(0)
    }

    fn audio_clip_count(&self) -> usize {
        self.diagnostics
            .as_ref()
            .map(|d| d.audio_clip_count)
            .unwrap_or(0)
    }

    fn midi_clip_count(&self) -> usize {
        self.diagnostics
            .as_ref()
            .map(|d| d.midi_clip_count)
            .unwrap_or(0)
    }

    fn pending_requests(&self) -> usize {
        self.diagnostics
            .as_ref()
            .map(|d| d.pending_requests)
            .unwrap_or(0)
    }

    fn workers_ready(&self) -> usize {
        self.diagnostics
            .as_ref()
            .map(|d| d.workers_ready)
            .unwrap_or(0)
    }

    fn engine_playing_label(&self) -> &'static str {
        self.diagnostics
            .as_ref()
            .map(|d| if d.transport_playing { "yes" } else { "no" })
            .unwrap_or("unknown")
    }

    fn track_meter_columns(&self) -> Vec<f32> {
        let mut columns = Vec::new();
        for (_, values) in self.track_meters.iter().take(6) {
            let peak = values
                .iter()
                .copied()
                .fold(-90.0_f32, |acc, value| acc.max(value));
            columns.push(peak);
        }
        columns
    }

    fn hw_meter_columns(&self) -> Vec<f32> {
        let mut columns = Vec::new();
        for level in self.hw_out_db.iter().take(2) {
            columns.push(*level);
        }
        columns
    }

    fn visible_export_fields(&self) -> Vec<ExportField> {
        let Some(export_ui) = self.export_ui.as_ref() else {
            return Vec::new();
        };
        let mut fields = vec![
            ExportField::FormatWav,
            ExportField::FormatMp3,
            ExportField::FormatOgg,
            ExportField::FormatFlac,
            ExportField::SampleRate,
        ];
        let selected_formats = export_ui.settings.selected_formats();
        if selected_formats
            .iter()
            .any(|format| matches!(format, ExportFormat::Wav | ExportFormat::Flac))
        {
            fields.push(ExportField::BitDepth);
        }
        if export_ui.settings.format_mp3 {
            fields.push(ExportField::Mp3Mode);
            fields.push(ExportField::Mp3Bitrate);
        }
        if export_ui.settings.format_ogg {
            fields.push(ExportField::OggQuality);
        }
        fields.push(ExportField::RenderMode);
        if matches!(export_ui.settings.render_mode, ExportRenderMode::Mixdown) {
            fields.extend((0..self.output_channels.max(1)).map(ExportField::HwOutPort));
        }
        fields.push(ExportField::RealtimeFallback);
        fields.push(ExportField::MasterLimiter);
        fields.push(ExportField::MasterLimiterCeiling);
        fields.push(ExportField::Normalize);
        if export_ui.settings.normalize {
            fields.push(ExportField::NormalizeMode);
            match export_ui.settings.normalize_mode {
                ExportNormalizeMode::Peak => fields.push(ExportField::NormalizeDbfs),
                ExportNormalizeMode::Loudness => {
                    fields.push(ExportField::NormalizeLufs);
                    fields.push(ExportField::NormalizeDbtp);
                    fields.push(ExportField::NormalizeLimiter);
                }
            }
        }
        fields.push(ExportField::ExportNow);
        fields.push(ExportField::Cancel);
        fields
    }

    fn selected_export_field(&self) -> Option<ExportField> {
        let export_ui = self.export_ui.as_ref()?;
        self.visible_export_fields()
            .get(export_ui.selected_index)
            .copied()
    }

    fn adjust_export_field(&mut self, delta: i32) {
        let field = match self.selected_export_field() {
            Some(field) => field,
            None => return,
        };
        let Some(export_ui) = self.export_ui.as_mut() else {
            return;
        };
        match field {
            ExportField::FormatWav => {
                export_ui.settings.format_wav = !export_ui.settings.format_wav
            }
            ExportField::FormatMp3 => {
                export_ui.settings.format_mp3 = !export_ui.settings.format_mp3
            }
            ExportField::FormatOgg => {
                export_ui.settings.format_ogg = !export_ui.settings.format_ogg
            }
            ExportField::FormatFlac => {
                export_ui.settings.format_flac = !export_ui.settings.format_flac
            }
            ExportField::SampleRate => {
                cycle_u32(
                    &mut export_ui.settings.sample_rate_hz,
                    &STANDARD_EXPORT_SAMPLE_RATES,
                    delta,
                );
            }
            ExportField::BitDepth => {
                let options = export_bit_depth_options(&export_ui.settings.selected_formats());
                cycle_copy(&mut export_ui.settings.bit_depth, &options, delta);
            }
            ExportField::Mp3Mode => cycle_copy(
                &mut export_ui.settings.mp3_mode,
                &EXPORT_MP3_MODE_ALL,
                delta,
            ),
            ExportField::Mp3Bitrate => {
                cycle_u16(
                    &mut export_ui.settings.mp3_bitrate_kbps,
                    &EXPORT_MP3_BITRATES_KBPS,
                    delta,
                );
            }
            ExportField::OggQuality => {
                export_ui.settings.ogg_quality =
                    (export_ui.settings.ogg_quality + (delta as f32 * 0.1)).clamp(-0.1, 1.0);
            }
            ExportField::RenderMode => {
                cycle_copy(
                    &mut export_ui.settings.render_mode,
                    &EXPORT_RENDER_MODE_ALL,
                    delta,
                );
            }
            ExportField::HwOutPort(port) => {
                toggle_hw_out_port(&mut export_ui.settings.hw_out_ports, port);
            }
            ExportField::RealtimeFallback => {
                export_ui.settings.realtime_fallback = !export_ui.settings.realtime_fallback;
            }
            ExportField::MasterLimiter => {
                export_ui.settings.master_limiter = !export_ui.settings.master_limiter;
            }
            ExportField::MasterLimiterCeiling => {
                export_ui.settings.master_limiter_ceiling_dbtp =
                    (export_ui.settings.master_limiter_ceiling_dbtp + delta as f32 * 0.5)
                        .clamp(-20.0, 0.0);
            }
            ExportField::Normalize => export_ui.settings.normalize = !export_ui.settings.normalize,
            ExportField::NormalizeMode => {
                cycle_copy(
                    &mut export_ui.settings.normalize_mode,
                    &EXPORT_NORMALIZE_MODE_ALL,
                    delta,
                );
            }
            ExportField::NormalizeDbfs => {
                export_ui.settings.normalize_dbfs =
                    (export_ui.settings.normalize_dbfs + delta as f32 * 0.5).clamp(-60.0, 0.0);
            }
            ExportField::NormalizeLufs => {
                export_ui.settings.normalize_lufs =
                    (export_ui.settings.normalize_lufs + delta as f32).clamp(-70.0, -5.0);
            }
            ExportField::NormalizeDbtp => {
                export_ui.settings.normalize_dbtp =
                    (export_ui.settings.normalize_dbtp + delta as f32 * 0.5).clamp(-20.0, 0.0);
            }
            ExportField::NormalizeLimiter => {
                export_ui.settings.normalize_tp_limiter = !export_ui.settings.normalize_tp_limiter;
            }
            ExportField::ExportNow | ExportField::Cancel => {}
        }
        export_ui
            .settings
            .normalize_hw_out_ports(self.output_channels);
        let bit_depth_options = export_bit_depth_options(&export_ui.settings.selected_formats());
        if !bit_depth_options.contains(&export_ui.settings.bit_depth) {
            export_ui.settings.bit_depth = bit_depth_options
                .first()
                .copied()
                .unwrap_or(ExportBitDepth::Int24);
        }
        let selected_index = export_ui.selected_index;
        let _ = export_ui;
        let visible_count = self.visible_export_fields().len();
        if let Some(export_ui) = self.export_ui.as_mut() {
            export_ui.selected_index = selected_index.min(visible_count.saturating_sub(1));
        }
    }

    fn activate_export_field(&mut self, export_tx: &UnboundedSender<ExportEvent>) {
        let field = match self.selected_export_field() {
            Some(field) => field,
            None => return,
        };
        match field {
            ExportField::ExportNow => self.start_export(export_tx),
            ExportField::Cancel => {
                self.export_ui = None;
                self.status = "Export panel closed".to_string();
            }
            _ => self.adjust_export_field(1),
        }
    }

    fn start_export(&mut self, export_tx: &UnboundedSender<ExportEvent>) {
        if self.export_in_progress {
            self.status = "Export already in progress".to_string();
            return;
        }
        let Some(session_dir) = self.session_dir.clone() else {
            self.status = "Export requires an opened/saved session".to_string();
            return;
        };
        let Some(export_ui) = self.export_ui.clone() else {
            return;
        };
        if let Err(err) = validate_export_settings(&export_ui.settings, &export_ui.session) {
            self.status = err;
            self.sticky_status = true;
            return;
        }
        let export_base_path = default_export_base_path(&session_dir);
        let settings = export_ui.settings.clone();
        let session = export_ui.session.clone();
        let tx = export_tx.clone();
        self.export_in_progress = true;
        self.export_progress = 0.0;
        self.export_operation = Some("Preparing".to_string());
        self.status = format!("Exporting to {}", export_base_path.display());
        self.sticky_status = true;
        tokio::spawn(async move {
            let result = export_session(
                &session,
                &session_dir,
                &export_base_path,
                &settings,
                |progress, operation| {
                    let _ = tx.send(ExportEvent::Progress {
                        progress,
                        operation,
                    });
                },
            )
            .await
            .map_err(|err| err.to_string());
            let _ = tx.send(ExportEvent::Finished(result));
        });
    }

    fn export_panel_lines(&self, export_ui: &ExportUiState) -> Text<'static> {
        let base_path = self
            .session_dir
            .as_ref()
            .map(|path| default_export_base_path(path).display().to_string())
            .unwrap_or_else(|| "session/export".to_string());
        let mut lines = vec![
            Line::from(format!("Path: {base_path}")),
            Line::from("Keys: Up/Down select, Left/Right change, Enter/Space apply, Esc close"),
            Line::from("Stem export in CLI renders all eligible tracks."),
            Line::from(""),
        ];
        let selected = export_ui.selected_index;
        for (index, field) in self.visible_export_fields().into_iter().enumerate() {
            let prefix = if index == selected { ">" } else { " " };
            lines.push(Line::from(format!(
                "{prefix} {}",
                self.export_field_label(field, &export_ui.settings)
            )));
        }
        if self.export_in_progress {
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "Progress: {}% {}",
                (self.export_progress * 100.0).round() as u16,
                self.export_operation.clone().unwrap_or_default()
            )));
        }
        Text::from(lines)
    }

    fn export_field_label(&self, field: ExportField, settings: &ExportSettings) -> String {
        match field {
            ExportField::FormatWav => format!("[{}] WAV", mark(settings.format_wav)),
            ExportField::FormatMp3 => format!("[{}] MP3", mark(settings.format_mp3)),
            ExportField::FormatOgg => format!("[{}] OGG", mark(settings.format_ogg)),
            ExportField::FormatFlac => format!("[{}] FLAC", mark(settings.format_flac)),
            ExportField::SampleRate => format!("Sample rate: {}", settings.sample_rate_hz),
            ExportField::BitDepth => format!("Bit depth: {}", settings.bit_depth),
            ExportField::Mp3Mode => format!("MP3 mode: {}", settings.mp3_mode),
            ExportField::Mp3Bitrate => format!("MP3 bitrate: {} kbps", settings.mp3_bitrate_kbps),
            ExportField::OggQuality => format!("OGG quality: {:.1}", settings.ogg_quality),
            ExportField::RenderMode => format!("Render mode: {}", settings.render_mode),
            ExportField::HwOutPort(port) => format!(
                "[{}] hw:out {}",
                mark(settings.hw_out_ports.contains(&port)),
                port + 1
            ),
            ExportField::RealtimeFallback => {
                format!(
                    "[{}] Real-time fallback render",
                    mark(settings.realtime_fallback)
                )
            }
            ExportField::MasterLimiter => {
                format!("[{}] Master limiter", mark(settings.master_limiter))
            }
            ExportField::MasterLimiterCeiling => {
                format!(
                    "Master limiter ceiling: {:.1} dBTP",
                    settings.master_limiter_ceiling_dbtp
                )
            }
            ExportField::Normalize => format!("[{}] Normalize", mark(settings.normalize)),
            ExportField::NormalizeMode => format!("Normalize mode: {}", settings.normalize_mode),
            ExportField::NormalizeDbfs => {
                format!("Normalize target: {:.1} dBFS", settings.normalize_dbfs)
            }
            ExportField::NormalizeLufs => {
                format!("Loudness target: {:.1} LUFS", settings.normalize_lufs)
            }
            ExportField::NormalizeDbtp => {
                format!("True peak ceiling: {:.1} dBTP", settings.normalize_dbtp)
            }
            ExportField::NormalizeLimiter => format!(
                "[{}] Use true-peak limiter",
                mark(settings.normalize_tp_limiter)
            ),
            ExportField::ExportNow => "Export".to_string(),
            ExportField::Cancel => "Cancel".to_string(),
        }
    }
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .flex(Flex::Center)
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .flex(Flex::Center)
    .split(vertical[1])[1]
}

fn mark(enabled: bool) -> &'static str {
    if enabled { "x" } else { " " }
}

fn cycle_copy<T: Copy + PartialEq>(current: &mut T, values: &[T], delta: i32) {
    if values.is_empty() {
        return;
    }
    let index = values
        .iter()
        .position(|value| value == current)
        .unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(values.len() as i32) as usize;
    *current = values[next];
}

fn cycle_u32(current: &mut u32, values: &[u32], delta: i32) {
    if values.is_empty() {
        return;
    }
    let index = values
        .iter()
        .position(|value| value == current)
        .unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(values.len() as i32) as usize;
    *current = values[next];
}

fn cycle_u16(current: &mut u16, values: &[u16], delta: i32) {
    if values.is_empty() {
        return;
    }
    let index = values
        .iter()
        .position(|value| value == current)
        .unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(values.len() as i32) as usize;
    *current = values[next];
}

fn toggle_hw_out_port(ports: &mut BTreeSet<usize>, port: usize) {
    if !ports.insert(port) {
        ports.remove(&port);
    }
}

fn transport_state_label(playing: bool, paused: bool) -> &'static str {
    if playing && paused {
        "paused"
    } else if playing {
        "playing"
    } else {
        "stopped"
    }
}

fn transport_state_color(playing: bool, paused: bool) -> Color {
    if playing && paused {
        Color::Yellow
    } else if playing {
        Color::Green
    } else {
        Color::Red
    }
}

fn format_transport_clock(sample: usize, sample_rate_hz: usize) -> String {
    if sample_rate_hz == 0 {
        return "--:--:--.---".to_string();
    }
    let total_ms = (sample as u128).saturating_mul(1000) / sample_rate_hz as u128;
    let hours = total_ms / 3_600_000;
    let minutes = (total_ms / 60_000) % 60;
    let seconds = (total_ms / 1000) % 60;
    let millis = total_ms % 1000;
    format!("{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
}

fn vertical_meter_lines(meters: &[f32]) -> Text<'static> {
    if meters.is_empty() {
        return Text::from(vec![Line::from("No meters yet")]);
    }
    build_vertical_meter_text(meters, 6)
}

fn build_vertical_meter_text(meters: &[f32], height: usize) -> Text<'static> {
    let mut lines = Vec::new();
    for row in (0..height).rev() {
        let mut line = String::new();
        for level in meters {
            let filled = meter_height(*level, height);
            let ch = if row < filled { '#' } else { '|' };
            line.push(' ');
            line.push(ch);
            line.push(' ');
        }
        lines.push(Line::from(line));
    }

    let mut baseline = String::new();
    for _ in meters {
        baseline.push_str("---");
    }
    lines.push(Line::from(baseline));

    let mut values = String::new();
    for level in meters {
        let rounded = level.round() as i32;
        values.push_str(&format!("{:^3}", rounded));
    }
    lines.push(Line::from(values));

    Text::from(lines)
}

fn meter_panel_width(column_count: usize) -> u16 {
    let content_width = (column_count.max(1) * 3) as u16;
    content_width.max(8) + 2
}

fn export_panel_scroll(
    viewport_lines: usize,
    selected_index: usize,
    field_count: usize,
    export_in_progress: bool,
) -> usize {
    let header_lines = 4usize;
    let progress_lines = if export_in_progress { 2 } else { 0 };
    let total_lines = header_lines + field_count + progress_lines;
    if viewport_lines == 0 || total_lines <= viewport_lines {
        return 0;
    }
    let selected_line = header_lines + selected_index.min(field_count.saturating_sub(1));
    let min_scroll = selected_line.saturating_sub(viewport_lines.saturating_sub(1));
    let max_scroll = total_lines.saturating_sub(viewport_lines);
    min_scroll.min(max_scroll)
}

fn meter_height(level_db: f32, height: usize) -> usize {
    let clamped = level_db.clamp(-90.0, 0.0);
    ((((clamped + 90.0) / 90.0) * height as f32).round() as usize).min(height)
}

async fn maybe_request_meters(app: &mut App, client: &Client) {
    if !app.hw_ready {
        return;
    }
    let now = Instant::now();
    if app
        .last_meter_request
        .is_some_and(|last| now.duration_since(last) < Duration::from_millis(100))
    {
        return;
    }
    let _ = client
        .send(EngineMessage::Request(Action::RequestMeterSnapshot))
        .await;
    app.last_meter_request = Some(now);
}

async fn send_transport_play(client: &Client, was_playing: bool) -> Result<(), String> {
    client
        .send(EngineMessage::Request(Action::SetClipPlaybackEnabled(true)))
        .await?;
    if !was_playing {
        client.send(EngineMessage::Request(Action::Play)).await?;
    }
    Ok(())
}

async fn send_transport_pause(client: &Client, was_playing: bool) -> Result<(), String> {
    client
        .send(EngineMessage::Request(Action::SetClipPlaybackEnabled(
            false,
        )))
        .await?;
    if !was_playing {
        client.send(EngineMessage::Request(Action::Play)).await?;
    }
    Ok(())
}

async fn send_transport_stop(client: &Client) -> Result<(), String> {
    client
        .send(EngineMessage::Request(Action::SetClipPlaybackEnabled(true)))
        .await?;
    client.send(EngineMessage::Request(Action::Stop)).await
}

async fn send_transport_position(client: &Client, sample: usize) -> Result<(), String> {
    client
        .send(EngineMessage::Request(Action::TransportPosition(sample)))
        .await
}

async fn send_transport_panic(client: &Client) -> Result<(), String> {
    client.send(EngineMessage::Request(Action::Panic)).await
}

fn parse_cli_options(args: impl IntoIterator<Item = String>) -> Result<CliOptions, String> {
    let mut options = CliOptions::default();
    let mut args = args.into_iter();
    let _ = args.next();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--device" => {
                options.device = Some(
                    args.next()
                        .ok_or_else(|| "--device requires a value".to_string())?,
                );
            }
            "--input-device" => {
                options.input_device = Some(
                    args.next()
                        .ok_or_else(|| "--input-device requires a value".to_string())?,
                );
            }
            "--sample-rate" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--sample-rate requires a value".to_string())?;
                options.sample_rate_hz = value
                    .parse()
                    .map_err(|_| format!("Invalid --sample-rate value: {value}"))?;
            }
            "--bits" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--bits requires a value".to_string())?;
                options.bits = value
                    .parse()
                    .map_err(|_| format!("Invalid --bits value: {value}"))?;
            }
            "--period-frames" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--period-frames requires a value".to_string())?;
                options.period_frames = value
                    .parse()
                    .map_err(|_| format!("Invalid --period-frames value: {value}"))?;
            }
            "--nperiods" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--nperiods requires a value".to_string())?;
                options.nperiods = value
                    .parse()
                    .map_err(|_| format!("Invalid --nperiods value: {value}"))?;
            }
            "--exclusive" => {
                options.exclusive = true;
            }
            "--sync-mode" => {
                options.sync_mode = true;
            }
            "--help" | "-h" => {
                return Err(help_text());
            }
            other => {
                if other.starts_with('-') {
                    return Err(format!("Unknown argument: {other}\n\n{}", help_text()));
                }
                if options.session_dir.is_some() {
                    return Err(format!(
                        "Only one session directory may be provided.\n\n{}",
                        help_text()
                    ));
                }
                options.session_dir = Some(PathBuf::from(other));
            }
        }
    }
    Ok(options)
}

fn help_text() -> String {
    format!(
        "Usage: maolan-cli [session_dir] [options]\n\nOptions:\n  --device <id>           Output device id\n  --input-device <id>     Input device id\n  --sample-rate <hz>      Sample rate (default: {})\n  --bits <n>              Bit depth (default: {})\n  --period-frames <n>     Period size (default: {})\n  --nperiods <n>          Number of periods (default: {})\n  --exclusive             Open device in exclusive mode\n  --sync-mode             Enable sync mode",
        audio_defaults::SAMPLE_RATE_HZ,
        audio_defaults::BIT_DEPTH,
        audio_defaults::PERIOD_FRAMES,
        audio_defaults::NPERIODS
    )
}

fn resolve_open_audio_action(options: &CliOptions, config: &CliConfig) -> Result<Action, String> {
    let device = options
        .device
        .clone()
        .or_else(|| config.default_output_device_id.clone())
        .ok_or_else(|| {
            "No output device configured. Pass --device or set default_output_device_id in ~/.config/maolan/config.toml".to_string()
        })?;
    let input_device = options
        .input_device
        .clone()
        .or_else(|| config.default_input_device_id.clone());
    let bits = if options.bits == CliOptions::default().bits && config.default_audio_bit_depth > 0 {
        config.default_audio_bit_depth
    } else {
        options.bits
    };
    Ok(Action::OpenAudioDevice {
        device,
        input_device,
        sample_rate_hz: options.sample_rate_hz,
        bits: bits as i32,
        exclusive: options.exclusive,
        period_frames: options.period_frames,
        nperiods: options.nperiods,
        sync_mode: options.sync_mode,
    })
}

fn map_key_event(key: KeyEvent) -> AppCommand {
    if key.kind != KeyEventKind::Press {
        return AppCommand::None;
    }
    match (key.code, key.modifiers) {
        (KeyCode::Char('e'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            AppCommand::ToggleExport
        }
        (KeyCode::Char('l'), modifiers) if modifiers.contains(KeyModifiers::CONTROL) => {
            AppCommand::Panic
        }
        (KeyCode::Char('q'), _) => AppCommand::Quit,
        (KeyCode::Esc, _) => AppCommand::Back,
        (KeyCode::Home, _) => AppCommand::JumpToStart,
        (KeyCode::End, _) => AppCommand::JumpToEnd,
        (KeyCode::Up, _) => AppCommand::MoveUp,
        (KeyCode::Down, _) => AppCommand::MoveDown,
        (KeyCode::Left, _) => AppCommand::MoveLeft,
        (KeyCode::Right, _) => AppCommand::MoveRight,
        (KeyCode::Enter, _) => AppCommand::Activate,
        (KeyCode::Char(' '), modifiers) if modifiers.contains(KeyModifiers::SHIFT) => {
            AppCommand::Pause
        }
        (KeyCode::Char(' '), _) => AppCommand::TogglePlayStop,
        _ => AppCommand::None,
    }
}

fn drain_engine_messages(app: &mut App, rx: &mut Receiver<EngineMessage>) {
    while let Ok(message) = rx.try_recv() {
        app.handle_engine_message(message);
    }
}

fn should_restore_session(
    restored: bool,
    session_dir: Option<&PathBuf>,
    open_audio_action: Option<&Action>,
    hw_ready: bool,
) -> bool {
    if restored || session_dir.is_none() {
        return false;
    }
    open_audio_action.is_none() || hw_ready
}

fn spawn_input_thread() -> UnboundedReceiver<AppCommand> {
    let (tx, rx) = unbounded_channel();
    thread::spawn(move || {
        loop {
            match event::poll(Duration::from_millis(100)) {
                Ok(true) => match event::read() {
                    Ok(Event::Key(key)) => {
                        let command = map_key_event(key);
                        if tx.send(command).is_err() || matches!(command, AppCommand::Quit) {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                },
                Ok(false) => {}
                Err(_) => break,
            }
        }
    });
    rx
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    let stdout = io::stdout();
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = match parse_cli_options(std::env::args()) {
        Ok(options) => options,
        Err(message) if message.starts_with("Usage: ") => {
            println!("{message}");
            return Ok(());
        }
        Err(message) => return Err(message.into()),
    };
    let config = CliConfig::load().unwrap_or_default();
    let open_audio_action = resolve_open_audio_action(&options, &config).ok();
    let status = match open_audio_action.as_ref() {
        Some(Action::OpenAudioDevice { device, .. }) => {
            if let Some(session_dir) = options.session_dir.as_ref() {
                format!(
                    "Ready to open audio device '{device}' for session '{}'",
                    session_dir.display()
                )
            } else {
                format!("Ready to open audio device '{device}'")
            }
        }
        _ => {
            if let Some(session_dir) = options.session_dir.as_ref() {
                format!(
                    "Session '{}'; no output device configured. Pass --device or set default_output_device_id in config.",
                    session_dir.display()
                )
            } else {
                "No output device configured. Pass --device or set default_output_device_id in config."
                    .to_string()
            }
        }
    };

    let _terminal = TerminalGuard::enter()?;
    let mut terminal = init_terminal()?;
    let client = Client::default();
    let _ = client
        .send(EngineMessage::Request(Action::SetOscEnabled(
            config.osc_enabled,
        )))
        .await;
    let mut rx = client.subscribe().await;
    let mut input_rx = spawn_input_thread();
    let (export_tx, mut export_rx) = unbounded_channel();
    let mut app = App::new(
        options.session_dir.clone(),
        open_audio_action,
        status,
        if config.default_export_sample_rate_hz == 0 {
            audio_defaults::SAMPLE_RATE_HZ as u32
        } else {
            config.default_export_sample_rate_hz
        },
    );
    let mut session_restored = false;
    app.draw(&mut terminal)?;
    app.open_audio(&client).await;
    if should_restore_session(
        session_restored,
        options.session_dir.as_ref(),
        app.open_audio_action.as_ref(),
        app.hw_ready,
    ) {
        app.restore_session(&client, options.session_dir.as_ref())
            .await;
        session_restored = true;
    }

    loop {
        drain_engine_messages(&mut app, &mut rx);
        if should_restore_session(
            session_restored,
            options.session_dir.as_ref(),
            app.open_audio_action.as_ref(),
            app.hw_ready,
        ) {
            app.restore_session(&client, options.session_dir.as_ref())
                .await;
            session_restored = true;
        }
        while let Ok(event) = export_rx.try_recv() {
            app.handle_export_event(event);
        }
        maybe_request_meters(&mut app, &client).await;
        app.draw(&mut terminal)?;
        while let Ok(command) = input_rx.try_recv() {
            if !app.handle_command(&client, command, &export_tx).await {
                let _ = client.send(EngineMessage::Request(Action::Quit)).await;
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_space_to_toggle_play_stop() {
        let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(map_key_event(key), AppCommand::TogglePlayStop);
    }

    #[test]
    fn restore_session_waits_for_audio_when_opening_device() {
        assert!(!should_restore_session(
            false,
            Some(&PathBuf::from("/tmp/session")),
            Some(&Action::OpenAudioDevice {
                device: "dev".to_string(),
                input_device: None,
                sample_rate_hz: audio_defaults::SAMPLE_RATE_HZ,
                bits: audio_defaults::BIT_DEPTH as i32,
                exclusive: false,
                period_frames: audio_defaults::PERIOD_FRAMES,
                nperiods: audio_defaults::NPERIODS,
                sync_mode: audio_defaults::SYNC_MODE,
            }),
            false,
        ));
        assert!(should_restore_session(
            false,
            Some(&PathBuf::from("/tmp/session")),
            Some(&Action::OpenAudioDevice {
                device: "dev".to_string(),
                input_device: None,
                sample_rate_hz: audio_defaults::SAMPLE_RATE_HZ,
                bits: audio_defaults::BIT_DEPTH as i32,
                exclusive: false,
                period_frames: audio_defaults::PERIOD_FRAMES,
                nperiods: audio_defaults::NPERIODS,
                sync_mode: audio_defaults::SYNC_MODE,
            }),
            true,
        ));
    }

    #[test]
    fn restore_session_runs_immediately_without_audio_open() {
        assert!(should_restore_session(
            false,
            Some(&PathBuf::from("/tmp/session")),
            None,
            false,
        ));
    }

    #[test]
    fn maps_shift_space_to_pause() {
        let key = KeyEvent::new(KeyCode::Char(' '), KeyModifiers::SHIFT);
        assert_eq!(map_key_event(key), AppCommand::Pause);
    }

    #[test]
    fn maps_ctrl_e_to_toggle_export() {
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL);
        assert_eq!(map_key_event(key), AppCommand::ToggleExport);
    }

    #[test]
    fn maps_home_to_jump_to_start() {
        let key = KeyEvent::new(KeyCode::Home, KeyModifiers::NONE);
        assert_eq!(map_key_event(key), AppCommand::JumpToStart);
    }

    #[test]
    fn maps_end_to_jump_to_end() {
        let key = KeyEvent::new(KeyCode::End, KeyModifiers::NONE);
        assert_eq!(map_key_event(key), AppCommand::JumpToEnd);
    }

    #[test]
    fn maps_ctrl_l_to_panic() {
        let key = KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL);
        assert_eq!(map_key_event(key), AppCommand::Panic);
    }

    #[test]
    fn resolve_open_audio_action_uses_cli_device_over_config() {
        let options = CliOptions {
            device: Some("cli-device".to_string()),
            input_device: Some("cli-input".to_string()),
            ..CliOptions::default()
        };
        let config = CliConfig {
            default_audio_bit_depth: 24,
            default_export_sample_rate_hz: 48_000,
            osc_enabled: false,
            default_output_device_id: Some("config-device".to_string()),
            default_input_device_id: Some("config-input".to_string()),
        };

        let action = resolve_open_audio_action(&options, &config).expect("open audio action");

        match action {
            Action::OpenAudioDevice {
                device,
                input_device,
                sample_rate_hz,
                bits,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            } => {
                assert_eq!(device, "cli-device");
                assert_eq!(input_device.as_deref(), Some("cli-input"));
                assert_eq!(sample_rate_hz, audio_defaults::SAMPLE_RATE_HZ);
                assert_eq!(bits, 24);
                assert!(!exclusive);
                assert_eq!(period_frames, audio_defaults::PERIOD_FRAMES);
                assert_eq!(nperiods, audio_defaults::NPERIODS);
                assert_eq!(sync_mode, audio_defaults::SYNC_MODE);
            }
            _ => panic!("expected OpenAudioDevice action"),
        }
    }

    #[test]
    fn resolve_open_audio_action_uses_config_defaults_when_cli_omits_device() {
        let options = CliOptions::default();
        let config = CliConfig {
            default_audio_bit_depth: 24,
            default_export_sample_rate_hz: 48_000,
            osc_enabled: false,
            default_output_device_id: Some("config-device".to_string()),
            default_input_device_id: Some("config-input".to_string()),
        };

        let action = resolve_open_audio_action(&options, &config).expect("open audio action");

        match action {
            Action::OpenAudioDevice {
                device,
                input_device,
                sample_rate_hz,
                bits,
                exclusive,
                period_frames,
                nperiods,
                sync_mode,
            } => {
                assert_eq!(device, "config-device");
                assert_eq!(input_device.as_deref(), Some("config-input"));
                assert_eq!(sample_rate_hz, audio_defaults::SAMPLE_RATE_HZ);
                assert_eq!(bits, 24);
                assert!(!exclusive);
                assert_eq!(period_frames, audio_defaults::PERIOD_FRAMES);
                assert_eq!(nperiods, audio_defaults::NPERIODS);
                assert_eq!(sync_mode, audio_defaults::SYNC_MODE);
            }
            _ => panic!("expected OpenAudioDevice action"),
        }
    }

    #[test]
    fn parse_cli_options_accepts_positional_session_dir() {
        let options = parse_cli_options(vec![
            "maolan-cli".to_string(),
            "/tmp/session".to_string(),
            "--device".to_string(),
            "hw:0".to_string(),
        ])
        .expect("cli options");

        assert_eq!(options.session_dir, Some(PathBuf::from("/tmp/session")));
        assert_eq!(options.device.as_deref(), Some("hw:0"));
    }

    #[test]
    fn parse_cli_options_rejects_multiple_session_dirs() {
        let err = parse_cli_options(vec![
            "maolan-cli".to_string(),
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
        ])
        .expect_err("multiple session dirs should fail");

        assert!(err.contains("Only one session directory may be provided."));
    }

    #[test]
    fn format_transport_clock_formats_elapsed_time() {
        assert_eq!(format_transport_clock(72_000, 48_000), "00:00:01.500");
    }

    #[test]
    fn format_transport_clock_handles_missing_sample_rate() {
        assert_eq!(format_transport_clock(1_000, 0), "--:--:--.---");
    }

    #[test]
    fn meter_height_scales_db_to_rows() {
        assert_eq!(meter_height(-90.0, 6), 0);
        assert_eq!(meter_height(-45.0, 6), 3);
        assert_eq!(meter_height(0.0, 6), 6);
    }

    #[test]
    fn build_vertical_meter_text_renders_values_without_labels() {
        let text = build_vertical_meter_text(&[-45.0], 4);
        let lines: Vec<String> = text
            .lines
            .into_iter()
            .map(|line| line.to_string())
            .collect();
        assert_eq!(lines[0], " | ");
        assert_eq!(lines[1], " | ");
        assert_eq!(lines[2], " # ");
        assert_eq!(lines[3], " # ");
        assert_eq!(lines[4], "---");
        assert_eq!(lines[5], "-45");
    }

    #[test]
    fn track_meter_columns_only_include_track_peaks() {
        let mut app = App::new(None, None, String::new(), 48_000);
        app.track_meters = vec![
            ("Track 1".to_string(), vec![-18.0, -12.0]),
            ("Track 2".to_string(), vec![-9.0]),
        ];
        app.hw_out_db = vec![-6.0, -3.0];

        assert_eq!(app.track_meter_columns(), vec![-12.0, -9.0]);
    }

    #[test]
    fn hw_meter_columns_only_include_hw_outputs() {
        let mut app = App::new(None, None, String::new(), 48_000);
        app.track_meters = vec![("Track 1".to_string(), vec![-18.0, -12.0])];
        app.hw_out_db = vec![-6.0, -3.0];

        assert_eq!(app.hw_meter_columns(), vec![-6.0, -3.0]);
    }

    #[test]
    fn meter_panel_width_uses_compact_right_dock_width() {
        assert_eq!(meter_panel_width(0), 10);
        assert_eq!(meter_panel_width(4), 14);
    }

    #[test]
    fn export_panel_scroll_keeps_selected_row_visible() {
        assert_eq!(export_panel_scroll(8, 0, 12, false), 0);
        assert_eq!(export_panel_scroll(8, 7, 12, false), 4);
        assert_eq!(export_panel_scroll(8, 11, 12, true), 8);
    }
}
