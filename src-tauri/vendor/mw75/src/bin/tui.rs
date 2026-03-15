//! Real-time 12-channel EEG chart viewer for MW75 Neuro headphones.
//!
//! Usage:
//!   cargo run --bin mw75-tui --features tui,rfcomm   # hardware (signed)
//!   cargo run --bin mw75-tui -- --simulate            # synthetic data
//!
//! Keys
//! ────
//!   +  / =   zoom out  (increase µV scale)
//!   -        zoom in   (decrease µV scale)
//!   a        auto-scale: fit Y axis to current peak amplitude
//!   v        toggle smooth overlay (dim raw + bright moving-average)
//!   p        toggle pause / resume streaming
//!   c        clear waveform buffers
//!   1-4      show channel group (1=Ch1-4, 2=Ch5-8, 3=Ch9-12, 4=all)
//!   Tab      open device picker (scan / connect to another MW75)
//!   q / Esc  quit  (Esc also closes device picker)

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Axis, Block, Borders, Chart, Clear, Dataset, GraphType, List, ListItem, ListState,
        Paragraph,
    },
    Frame, Terminal,
};

use mw75::mw75_client::{Mw75Client, Mw75ClientConfig};
use mw75::protocol::{EEG_CHANNEL_NAMES, NUM_EEG_CHANNELS, SampleRate};
use mw75::simulate::spawn_simulator_with_rate;
use mw75::types::Mw75Event;

// ── Constants ─────────────────────────────────────────────────────────────────

const WINDOW_SECS: f64 = 2.0;

const Y_SCALES: &[f64] = &[10.0, 25.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0];
const DEFAULT_SCALE: usize = 4;

/// 12 distinct colours for channels.
const COLORS: [Color; 12] = [
    Color::Cyan,
    Color::Yellow,
    Color::Green,
    Color::Magenta,
    Color::LightRed,
    Color::LightBlue,
    Color::LightGreen,
    Color::LightCyan,
    Color::LightYellow,
    Color::Rgb(255, 150, 50),
    Color::Rgb(150, 100, 255),
    Color::Rgb(100, 255, 200),
];

const DIM_COLORS: [Color; 12] = [
    Color::Rgb(0, 90, 110),
    Color::Rgb(110, 90, 0),
    Color::Rgb(0, 110, 0),
    Color::Rgb(110, 0, 110),
    Color::Rgb(110, 40, 40),
    Color::Rgb(40, 40, 110),
    Color::Rgb(40, 110, 40),
    Color::Rgb(40, 110, 110),
    Color::Rgb(110, 110, 40),
    Color::Rgb(110, 70, 20),
    Color::Rgb(70, 40, 130),
    Color::Rgb(40, 130, 90),
];

const SMOOTH_WINDOW: usize = 9;
const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

// ── Channel view modes ────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum ChannelView {
    Group1, // Ch1-4
    Group2, // Ch5-8
    Group3, // Ch9-12
    All12,  // All channels
}

impl ChannelView {
    fn channels(self) -> Vec<usize> {
        match self {
            Self::Group1 => vec![0, 1, 2, 3],
            Self::Group2 => vec![4, 5, 6, 7],
            Self::Group3 => vec![8, 9, 10, 11],
            Self::All12 => (0..12).collect(),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Group1 => "Ch1-4",
            Self::Group2 => "Ch5-8",
            Self::Group3 => "Ch9-12",
            Self::All12 => "All 12",
        }
    }
}

// ── Commands from UI → data-source task ───────────────────────────────────────

enum DataCmd {
    /// Pause EEG streaming (shut down RFCOMM, or stop BLE).
    Pause,
    /// Resume EEG streaming (restart RFCOMM, or re-activate BLE).
    Resume,
    /// Scan for devices — results come back via App.scan_results.
    ScanDevices,
    /// Connect to the device at the given index in scan_results.
    ConnectDevice(usize),
}

// ── App mode ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
enum AppMode {
    Scanning,
    Connected { name: String },
    Simulated,
    Disconnected,
    /// RFCOMM is reconnecting after a resume.
    Reconnecting,
}

// ── Device picker state ───────────────────────────────────────────────────────

#[derive(Clone)]
struct DeviceEntry {
    name: String,
    id: String,
}

#[derive(Clone)]
enum PickerStatus {
    Idle,
    Scanning,
    Connecting(String),
}

struct DevicePicker {
    visible: bool,
    devices: Vec<DeviceEntry>,
    list_state: ListState,
    status: PickerStatus,
}

impl DevicePicker {
    fn new() -> Self {
        Self {
            visible: false,
            devices: Vec::new(),
            list_state: ListState::default(),
            status: PickerStatus::Idle,
        }
    }

    fn toggle(&mut self) {
        self.visible = !self.visible;
        if self.visible && self.devices.is_empty() {
            self.status = PickerStatus::Scanning;
        }
    }

    fn close(&mut self) {
        self.visible = false;
    }

    fn move_up(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        if i > 0 {
            self.list_state.select(Some(i - 1));
        }
    }

    fn move_down(&mut self) {
        let i = self.list_state.selected().unwrap_or(0);
        if !self.devices.is_empty() && i + 1 < self.devices.len() {
            self.list_state.select(Some(i + 1));
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.list_state.selected()
    }
}

// ── App state ─────────────────────────────────────────────────────────────────

struct App {
    bufs: Vec<VecDeque<f64>>,
    mode: AppMode,
    battery: Option<u8>,
    total_samples: u64,
    pkt_times: VecDeque<Instant>,
    scale_idx: usize,
    paused: bool,
    smooth: bool,
    dropped_packets: u64,
    last_counter: Option<u8>,
    view: ChannelView,
    eeg_hz: f64,
    buf_size: usize,
    /// True when EEG streaming is paused at the BT transport level.
    eeg_paused: bool,
    /// Device picker popup state.
    picker: DevicePicker,
    /// BLE id of the currently connected device (for picker highlight).
    connected_id: Option<String>,
}

impl App {
    fn new(sample_rate: SampleRate) -> Self {
        let eeg_hz = sample_rate.hz();
        let buf_size = (WINDOW_SECS * eeg_hz) as usize;
        Self {
            bufs: (0..NUM_EEG_CHANNELS)
                .map(|_| VecDeque::with_capacity(buf_size + 16))
                .collect(),
            mode: AppMode::Scanning,
            battery: None,
            total_samples: 0,
            pkt_times: VecDeque::with_capacity(512),
            scale_idx: DEFAULT_SCALE,
            paused: false,
            smooth: true,
            dropped_packets: 0,
            last_counter: None,
            view: ChannelView::All12,
            eeg_hz,
            buf_size,
            eeg_paused: false,
            picker: DevicePicker::new(),
            connected_id: None,
        }
    }

    fn push(&mut self, channels: &[f32]) {
        if self.paused {
            return;
        }
        for (ch, &v) in channels.iter().enumerate().take(NUM_EEG_CHANNELS) {
            let buf = &mut self.bufs[ch];
            buf.push_back(v as f64);
            while buf.len() > self.buf_size {
                buf.pop_front();
            }
        }
        self.total_samples += 1;
        let now = Instant::now();
        self.pkt_times.push_back(now);
        while self
            .pkt_times
            .front()
            .map(|t| now.duration_since(*t) > Duration::from_secs(2))
            .unwrap_or(false)
        {
            self.pkt_times.pop_front();
        }
    }

    fn clear(&mut self) {
        for b in &mut self.bufs {
            b.clear();
        }
        self.total_samples = 0;
        self.pkt_times.clear();
        self.dropped_packets = 0;
        self.last_counter = None;
    }

    fn pkt_rate(&self) -> f64 {
        let n = self.pkt_times.len();
        if n < 2 {
            return 0.0;
        }
        let span = self
            .pkt_times
            .back()
            .unwrap()
            .duration_since(self.pkt_times[0])
            .as_secs_f64();
        if span < 1e-9 {
            0.0
        } else {
            (n as f64 - 1.0) / span
        }
    }

    fn y_range(&self) -> f64 {
        Y_SCALES[self.scale_idx]
    }
    fn scale_up(&mut self) {
        if self.scale_idx + 1 < Y_SCALES.len() {
            self.scale_idx += 1;
        }
    }
    fn scale_down(&mut self) {
        if self.scale_idx > 0 {
            self.scale_idx -= 1;
        }
    }

    fn auto_scale(&mut self) {
        let visible = self.view.channels();
        let peak = visible
            .iter()
            .flat_map(|&ch| self.bufs[ch].iter())
            .fold(0.0_f64, |acc, &v| acc.max(v.abs()));
        let needed = peak * 1.1;
        self.scale_idx = Y_SCALES
            .iter()
            .position(|&s| s >= needed)
            .unwrap_or(Y_SCALES.len() - 1);
    }

    fn track_counter(&mut self, counter: u8) {
        if let Some(last) = self.last_counter {
            let expected = last.wrapping_add(1);
            if counter != expected {
                self.dropped_packets += counter.wrapping_sub(expected) as u64;
            }
        }
        self.last_counter = Some(counter);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn spinner_str() -> &'static str {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    SPINNER[(ms / 100) as usize % SPINNER.len()]
}

fn smooth_signal(data: &[(f64, f64)], window: usize) -> Vec<(f64, f64)> {
    if data.len() < 3 || window < 2 {
        return data.to_vec();
    }
    let half = window / 2;
    data.iter()
        .enumerate()
        .map(|(i, &(x, _))| {
            let start = i.saturating_sub(half);
            let end = (i + half + 1).min(data.len());
            let sum: f64 = data[start..end].iter().map(|&(_, y)| y).sum();
            (x, sum / (end - start) as f64)
        })
        .collect()
}

/// Centre a popup rectangle of the given size inside `area`.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let popup_w = area.width * percent_x / 100;
    let popup_h = area.height * percent_y / 100;
    let x = area.x + (area.width.saturating_sub(popup_w)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_h)) / 2;
    Rect::new(x, y, popup_w, popup_h)
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let root = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .split(area);

    draw_header(frame, root[0], app);
    draw_charts(frame, root[1], app);
    draw_footer(frame, root[2], app);

    // Device picker popup (drawn on top)
    if app.picker.visible {
        draw_device_picker(frame, area, app);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let (label, color) = match &app.mode {
        AppMode::Scanning => (format!("{} Scanning…", spinner_str()), Color::Yellow),
        AppMode::Connected { name } => (format!("● {name}"), Color::Green),
        AppMode::Simulated => ("◆ Simulated".to_owned(), Color::Cyan),
        AppMode::Disconnected => (format!("{} Disconnected", spinner_str()), Color::Red),
        AppMode::Reconnecting => (
            format!("{} Reconnecting…", spinner_str()),
            Color::Yellow,
        ),
    };

    let bat = app
        .battery
        .map(|b| format!("🔋{b}%"))
        .unwrap_or_default();
    let waiting_for_data = !app.eeg_paused
        && matches!(app.mode, AppMode::Connected { .. } | AppMode::Reconnecting)
        && app.pkt_rate() < 1.0;
    let rate = if waiting_for_data {
        format!("{} waiting…", spinner_str())
    } else {
        format!("{:.0}Hz", app.pkt_rate())
    };
    let rate_color = if waiting_for_data { Color::Yellow } else { Color::White };
    let scale = format!("±{:.0}µV", app.y_range());
    let total = format!("{}K", app.total_samples / 1_000);
    let dropped = if app.dropped_packets > 0 {
        format!(" {}drop", app.dropped_packets)
    } else {
        String::new()
    };
    let view_label = app.view.label();

    let line = Line::from(vec![
        Span::styled(
            " MW75 ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            label,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(bat, Style::default().fg(Color::White)),
        Span::raw(" "),
        Span::styled(rate, Style::default().fg(rate_color)),
        Span::raw(" "),
        Span::styled(scale, Style::default().fg(Color::LightBlue)),
        Span::raw(" "),
        Span::styled(total, Style::default().fg(Color::DarkGray)),
        Span::styled(dropped, Style::default().fg(Color::Red)),
        Span::raw("  "),
        Span::styled(
            format!("[{view_label}]"),
            Style::default().fg(Color::Yellow),
        ),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn draw_charts(frame: &mut Frame, area: Rect, app: &App) {
    let channels = app.view.channels();
    let n = channels.len();
    if n == 0 {
        return;
    }

    let constraints: Vec<Constraint> = (0..n).map(|_| Constraint::Ratio(1, n as u32)).collect();
    let rows = Layout::vertical(constraints).split(area);
    let y_range = app.y_range();

    for (row_idx, &ch) in channels.iter().enumerate() {
        let data: Vec<(f64, f64)> = app.bufs[ch]
            .iter()
            .enumerate()
            .map(|(i, &v)| (i as f64 / app.eeg_hz, v.clamp(-y_range, y_range)))
            .collect();

        draw_channel(frame, rows[row_idx], ch, &data, app);
    }
}

fn draw_channel(frame: &mut Frame, area: Rect, ch: usize, data: &[(f64, f64)], app: &App) {
    let color = COLORS[ch % COLORS.len()];
    let dim_color = DIM_COLORS[ch % DIM_COLORS.len()];
    let y_range = app.y_range();
    let name = EEG_CHANNEL_NAMES.get(ch).copied().unwrap_or("?");

    let (min_v, max_v, rms_v) = {
        let buf = &app.bufs[ch];
        if buf.is_empty() {
            (0.0, 0.0, 0.0)
        } else {
            let min = buf.iter().copied().fold(f64::INFINITY, f64::min);
            let max = buf.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let rms = (buf.iter().map(|&v| v * v).sum::<f64>() / buf.len() as f64).sqrt();
            (min, max, rms)
        }
    };

    let clipping = max_v > y_range || min_v < -y_range;
    let border_color = if clipping { Color::Red } else { Color::DarkGray };
    let clip_tag = if clipping { " CLIP" } else { "" };

    let title = format!(" {name} rms:{rms_v:.0}{clip_tag} ");

    let smoothed: Vec<(f64, f64)> = if app.smooth {
        smooth_signal(data, SMOOTH_WINDOW)
    } else {
        vec![]
    };

    let datasets: Vec<Dataset> = if app.smooth {
        vec![
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(dim_color))
                .data(data),
            Dataset::default()
                .marker(symbols::Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::default().fg(color))
                .data(&smoothed),
        ]
    } else {
        vec![Dataset::default()
            .marker(symbols::Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(color))
            .data(data)]
    };

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Span::styled(
                    title,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ))
                .borders(Borders::LEFT | Borders::RIGHT)
                .border_style(Style::default().fg(border_color)),
        )
        .x_axis(
            Axis::default()
                .bounds([0.0, WINDOW_SECS])
                .style(Style::default().fg(Color::DarkGray)),
        )
        .y_axis(
            Axis::default()
                .bounds([-y_range, y_range])
                .style(Style::default().fg(Color::DarkGray)),
        );

    frame.render_widget(chart, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let pause_label = if app.eeg_paused {
        Span::styled(
            " ⏸PAUSED ",
            Style::default()
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::raw("")
    };

    let line = Line::from(vec![
        Span::raw(" "),
        key("+/-"),
        Span::raw("Scale "),
        key("a"),
        Span::raw("Auto "),
        key("v"),
        Span::raw("Smooth "),
        key("p"),
        Span::raw("Pause "),
        key("c"),
        Span::raw("Clear "),
        key("1-4"),
        Span::raw("View "),
        key("Tab"),
        Span::raw("Devices "),
        key("q"),
        Span::raw("Quit"),
        pause_label,
    ]);

    frame.render_widget(Paragraph::new(line), area);
}

fn draw_device_picker(frame: &mut Frame, area: Rect, app: &mut App) {
    let popup = centered_rect(60, 50, area);

    // Clear the area behind the popup
    frame.render_widget(Clear, popup);

    let dynamic_title: String;
    let block_title = match &app.picker.status {
        PickerStatus::Connecting(name) => {
            dynamic_title = format!(" {} Connecting to {name}… ", spinner_str());
            &dynamic_title
        }
        PickerStatus::Scanning => {
            dynamic_title = format!(" {} Scanning for devices… ", spinner_str());
            &dynamic_title
        }
        PickerStatus::Idle if app.picker.devices.is_empty() => {
            " Devices — press 's' to scan, Esc to close "
        }
        _ => " Devices — ↑↓ select, Enter connect, s scan, Esc close ",
    };

    let connected_id = app.connected_id.as_deref();
    let items: Vec<ListItem> = app
        .picker
        .devices
        .iter()
        .enumerate()
        .map(|(i, dev)| {
            let is_connected = connected_id == Some(dev.id.as_str());
            let prefix = if is_connected { "● " } else { "  " };
            let suffix = if is_connected { "  [connected]" } else { "" };
            let content = format!("{prefix}{}  ({}){suffix}", dev.name, dev.id);
            let style = if app.picker.list_state.selected() == Some(i) {
                Style::default()
                    .fg(Color::Black)
                    .bg(if is_connected { Color::Green } else { Color::Cyan })
                    .add_modifier(Modifier::BOLD)
            } else if is_connected {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(content).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .title(Span::styled(
                block_title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );

    frame.render_stateful_widget(list, popup, &mut app.picker.list_state);
}

#[inline]
fn key(s: &str) -> Span<'_> {
    Span::styled(
        s,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    use std::io::IsTerminal as _;
    if !io::stdout().is_terminal() {
        eprintln!("Error: mw75-tui requires a real terminal (TTY).");
        std::process::exit(1);
    }

    // Log to file (TUI owns the terminal)
    {
        use std::fs::File;
        if let Ok(file) = File::create("mw75-tui.log") {
            env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
                .target(env_logger::Target::Pipe(Box::new(file)))
                .init();
        }
    }

    let simulate = std::env::args().any(|a| a == "--simulate");
    let sample_rate = if std::env::args().any(|a| a == "--256hz" || a == "--256") {
        SampleRate::Hz256
    } else {
        SampleRate::Hz500
    };
    let app = Arc::new(Mutex::new(App::new(sample_rate)));

    // Command channel: UI → data-source task (pause/resume/scan/connect)
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<DataCmd>();

    // Spawn tokio on a background thread (main thread pumps NSRunLoop on macOS)
    let app_clone = Arc::clone(&app);
    let (done_tx, _done_rx) = std::sync::mpsc::channel::<()>();

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        rt.block_on(async {
            run_data_source(simulate, sample_rate, app_clone, cmd_rx).await;
        });
        let _ = done_tx.send(());
    });

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    let tick = Duration::from_millis(33); // ~30 FPS

    // Main loop — also pumps CFRunLoop on macOS for IOBluetooth callbacks
    loop {
        // Pump macOS run loop
        #[cfg(target_os = "macos")]
        unsafe {
            extern "C" {
                fn CFRunLoopRunInMode(
                    mode: *const std::ffi::c_void,
                    seconds: f64,
                    ret: bool,
                ) -> i32;
            }
            extern "C" {
                static kCFRunLoopDefaultMode: *const std::ffi::c_void;
            }
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, false);
        }

        // Render
        {
            let mut s = app.lock().unwrap();
            terminal.draw(|f| draw(f, &mut s))?;
        }

        // Handle input
        if !event::poll(tick)? {
            continue;
        }
        let Event::Key(kev) = event::read()? else {
            continue;
        };

        let ctrl_c =
            kev.modifiers.contains(KeyModifiers::CONTROL) && kev.code == KeyCode::Char('c');

        // ── Device picker is open: capture keys ──────────────────────────
        let picker_open = app.lock().unwrap().picker.visible;
        if picker_open {
            match kev.code {
                KeyCode::Esc | KeyCode::Tab => {
                    app.lock().unwrap().picker.close();
                }
                KeyCode::Up => {
                    app.lock().unwrap().picker.move_up();
                }
                KeyCode::Down => {
                    app.lock().unwrap().picker.move_down();
                }
                KeyCode::Char('s') | KeyCode::Char('S') => {
                    {
                        let mut s = app.lock().unwrap();
                        s.picker.status = PickerStatus::Scanning;
                        s.picker.devices.clear();
                        s.picker.list_state.select(None);
                    }
                    let _ = cmd_tx.send(DataCmd::ScanDevices);
                }
                KeyCode::Enter => {
                    let idx = app.lock().unwrap().picker.selected_index();
                    if let Some(i) = idx {
                        let name = {
                            let s = app.lock().unwrap();
                            s.picker.devices.get(i).map(|d| d.name.clone())
                        };
                        if let Some(name) = name {
                            app.lock().unwrap().picker.status =
                                PickerStatus::Connecting(name);
                            let _ = cmd_tx.send(DataCmd::ConnectDevice(i));
                        }
                    }
                }
                _ if ctrl_c => break,
                KeyCode::Char('q') => break,
                _ => {}
            }
            continue;
        }

        // ── Normal mode keys ─────────────────────────────────────────────
        if kev.code == KeyCode::Char('q') || kev.code == KeyCode::Esc || ctrl_c {
            break;
        }

        match kev.code {
            KeyCode::Char('+') | KeyCode::Char('=') => app.lock().unwrap().scale_up(),
            KeyCode::Char('-') => app.lock().unwrap().scale_down(),
            KeyCode::Char('a') => app.lock().unwrap().auto_scale(),
            KeyCode::Char('v') => {
                app.lock().unwrap().smooth ^= true;
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                let mut s = app.lock().unwrap();
                if s.eeg_paused {
                    // Resume
                    s.paused = false;
                    s.eeg_paused = false;
                    s.mode = AppMode::Reconnecting;
                    let _ = cmd_tx.send(DataCmd::Resume);
                } else {
                    // Pause
                    s.paused = true;
                    s.eeg_paused = true;
                    let _ = cmd_tx.send(DataCmd::Pause);
                }
            }
            KeyCode::Char('c') if !kev.modifiers.contains(KeyModifiers::CONTROL) => {
                app.lock().unwrap().clear();
            }
            KeyCode::Char('1') => app.lock().unwrap().view = ChannelView::Group1,
            KeyCode::Char('2') => app.lock().unwrap().view = ChannelView::Group2,
            KeyCode::Char('3') => app.lock().unwrap().view = ChannelView::Group3,
            KeyCode::Char('4') => app.lock().unwrap().view = ChannelView::All12,
            KeyCode::Tab => {
                let mut s = app.lock().unwrap();
                s.picker.toggle();
                if matches!(s.picker.status, PickerStatus::Scanning) {
                    drop(s);
                    let _ = cmd_tx.send(DataCmd::ScanDevices);
                }
            }
            _ => {}
        }
    }

    // Teardown
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

// ── RFCOMM helper ─────────────────────────────────────────────────────────────

/// Start (or restart) the RFCOMM reader task.
#[cfg(feature = "rfcomm")]
async fn start_rfcomm(
    handle: &Arc<mw75::mw75_client::Mw75Handle>,
    bt_address: &str,
) -> Option<mw75::rfcomm::RfcommHandle> {
    let rfcomm_handle = handle.clone();
    match mw75::rfcomm::start_rfcomm_stream(rfcomm_handle, bt_address).await {
        Ok(rfcomm) => {
            log::info!("RFCOMM reader task started");
            Some(rfcomm)
        }
        Err(e) => {
            log::error!("RFCOMM failed: {e}");
            None
        }
    }
}

// ── Data source task ──────────────────────────────────────────────────────────

async fn run_data_source(
    simulate: bool,
    sample_rate: SampleRate,
    app: Arc<Mutex<App>>,
    mut cmd_rx: tokio::sync::mpsc::UnboundedReceiver<DataCmd>,
) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Mw75Event>(512);

    if simulate {
        {
            let mut s = app.lock().unwrap();
            s.mode = AppMode::Simulated;
            s.scale_idx = 4;
        }
        let _sim = spawn_simulator_with_rate(tx, true, sample_rate);
        // Simulated: just dispatch events, handle scan commands
        loop {
            tokio::select! {
                event = rx.recv() => {
                    match event {
                        Some(Mw75Event::Eeg(pkt)) => {
                            let mut s = app.lock().unwrap();
                            s.track_counter(pkt.counter);
                            s.push(&pkt.channels);
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(DataCmd::ScanDevices) => {
                            handle_scan(&app, sample_rate).await;
                        }
                        Some(DataCmd::ConnectDevice(_)) => {
                            // In simulation mode we can't connect to real devices
                            let mut s = app.lock().unwrap();
                            s.picker.status = PickerStatus::Idle;
                        }
                        _ => {} // pause/resume ignored in sim
                    }
                }
            }
        }
    } else {
        run_hardware_source(sample_rate, app, &mut cmd_rx, tx, &mut rx).await;
    }
}

/// Perform a BLE scan and populate the picker device list.
async fn handle_scan(app: &Arc<Mutex<App>>, sample_rate: SampleRate) {
    log::info!("TUI: scanning for devices…");
    let config = Mw75ClientConfig {
        scan_timeout_secs: 6,
        sample_rate,
        ..Mw75ClientConfig::default()
    };
    let client = Mw75Client::new(config);
    match client.scan_all().await {
        Ok(devices) => {
            let mut s = app.lock().unwrap();
            s.picker.devices = devices
                .iter()
                .map(|d| DeviceEntry {
                    name: d.name.clone(),
                    id: d.id.clone(),
                })
                .collect();
            if !s.picker.devices.is_empty() {
                s.picker.list_state.select(Some(0));
            }
            s.picker.status = PickerStatus::Idle;
            log::info!("TUI: scan found {} device(s)", s.picker.devices.len());
        }
        Err(e) => {
            log::error!("TUI: scan failed: {e}");
            let mut s = app.lock().unwrap();
            s.picker.status = PickerStatus::Idle;
        }
    }
}

/// Hardware data source: connect, activate, stream, handle commands.
///
/// Supports multiple connect cycles — when the user picks a new device
/// via the Tab popup, we tear down the old connection and start a new one.
async fn run_hardware_source(
    sample_rate: SampleRate,
    app: Arc<Mutex<App>>,
    cmd_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DataCmd>,
    tx: tokio::sync::mpsc::Sender<Mw75Event>,
    rx: &mut tokio::sync::mpsc::Receiver<Mw75Event>,
) {
    let config = Mw75ClientConfig {
        sample_rate,
        ..Mw75ClientConfig::default()
    };
    let client = Mw75Client::new(config.clone());

    // Try initial auto-connect
    let initial = client.connect().await;
    let (device_rx, handle) = match initial {
        Ok(pair) => pair,
        Err(e) => {
            log::error!("Initial connection failed: {e}");
            app.lock().unwrap().mode = AppMode::Disconnected;

            // Still service commands (scan / connect from picker)
            loop {
                match cmd_rx.recv().await {
                    Some(DataCmd::ScanDevices) => {
                        handle_scan(&app, sample_rate).await;
                    }
                    Some(DataCmd::ConnectDevice(idx)) => {
                        // Try to connect to the selected device
                        if let Some(result) =
                            handle_connect_device(&app, sample_rate, idx).await
                        {
                            let (drx, handle) = result;
                            run_connected_session(
                                &app, cmd_rx, &tx, rx, drx, handle, sample_rate,
                            )
                            .await;
                            // After session ends, loop back to wait for commands
                            continue;
                        }
                    }
                    None => return,
                    _ => {}
                }
            }
        }
    };

    if let Err(e) = handle.start().await {
        log::error!("Activation failed: {e}");
        return;
    }

    run_connected_session(&app, cmd_rx, &tx, rx, device_rx, handle, sample_rate).await;
}

/// Connect to a specific scanned device by index.
async fn handle_connect_device(
    app: &Arc<Mutex<App>>,
    sample_rate: SampleRate,
    idx: usize,
) -> Option<(tokio::sync::mpsc::Receiver<Mw75Event>, mw75::mw75_client::Mw75Handle)> {
    // We need to re-scan to get the actual Mw75Device (with peripheral handle).
    // The picker only stores name/id.
    let target_id = {
        let s = app.lock().unwrap();
        s.picker.devices.get(idx).map(|d| d.id.clone())
    };
    let target_id = target_id?;

    log::info!("TUI: scanning to find device id={target_id}…");
    let scan_config = Mw75ClientConfig {
        scan_timeout_secs: 6,
        sample_rate,
        ..Mw75ClientConfig::default()
    };
    let scan_client = Mw75Client::new(scan_config);
    let devices = match scan_client.scan_all().await {
        Ok(d) => d,
        Err(e) => {
            log::error!("TUI: re-scan failed: {e}");
            let mut s = app.lock().unwrap();
            s.picker.status = PickerStatus::Idle;
            return None;
        }
    };

    let device = devices.into_iter().find(|d| d.id == target_id);
    let device = match device {
        Some(d) => d,
        None => {
            log::error!("TUI: device {target_id} not found in re-scan");
            let mut s = app.lock().unwrap();
            s.picker.status = PickerStatus::Idle;
            return None;
        }
    };

    let connect_config = Mw75ClientConfig {
        sample_rate,
        ..Mw75ClientConfig::default()
    };
    let connect_client = Mw75Client::new(connect_config);
    match connect_client.connect_to(device).await {
        Ok((drx, handle)) => {
            if let Err(e) = handle.start().await {
                log::error!("TUI: activation failed for new device: {e}");
                let mut s = app.lock().unwrap();
                s.picker.status = PickerStatus::Idle;
                return None;
            }
            {
                let mut s = app.lock().unwrap();
                s.picker.close();
                s.clear();
            }
            Some((drx, handle))
        }
        Err(e) => {
            log::error!("TUI: connect_to failed: {e}");
            let mut s = app.lock().unwrap();
            s.picker.status = PickerStatus::Idle;
            None
        }
    }
}

/// Run a connected session: forward events, handle pause/resume/scan/connect.
async fn run_connected_session(
    app: &Arc<Mutex<App>>,
    cmd_rx: &mut tokio::sync::mpsc::UnboundedReceiver<DataCmd>,
    tx: &tokio::sync::mpsc::Sender<Mw75Event>,
    rx: &mut tokio::sync::mpsc::Receiver<Mw75Event>,
    mut device_rx: tokio::sync::mpsc::Receiver<Mw75Event>,
    handle: mw75::mw75_client::Mw75Handle,
    sample_rate: SampleRate,
) {
    let handle = Arc::new(handle);

    // Track the connected device id
    {
        let mut s = app.lock().unwrap();
        s.connected_id = Some(handle.peripheral_id());
    }

    // ── Start RFCOMM transport ───────────────────────────────────────
    #[cfg(feature = "rfcomm")]
    let bt_address = handle.peripheral_id();

    #[cfg(feature = "rfcomm")]
    let mut rfcomm_task: Option<mw75::rfcomm::RfcommHandle> = {
        log::info!("Disconnecting BLE before RFCOMM…");
        handle.disconnect_ble().await.ok();
        start_rfcomm(&handle, &bt_address).await
    };

    // Forward device events to the local channel
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        while let Some(event) = device_rx.recv().await {
            if tx_clone.send(event).await.is_err() {
                break;
            }
        }
    });

    // Saved mode name to restore after reconnect
    let device_name = handle.device_name().to_string();

    // ── Event + command loop ─────────────────────────────────────────
    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(ev) => {
                        let mut s = app.lock().unwrap();
                        match ev {
                            Mw75Event::Connected(name) => {
                                s.mode = AppMode::Connected { name };
                            }
                            Mw75Event::Disconnected => {
                                s.mode = AppMode::Disconnected;
                                s.connected_id = None;
                                break;
                            }
                            Mw75Event::Battery(b) => {
                                s.battery = Some(b.level);
                            }
                            Mw75Event::Eeg(pkt) => {
                                // First EEG packet after resume → clear Reconnecting
                                if matches!(s.mode, AppMode::Reconnecting) {
                                    s.mode = AppMode::Connected { name: device_name.clone() };
                                }
                                s.track_counter(pkt.counter);
                                s.push(&pkt.channels);
                            }
                            _ => {}
                        }
                    }
                    None => break, // channel closed
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    #[cfg(feature = "rfcomm")]
                    Some(DataCmd::Pause) => {
                        log::info!("TUI: pausing EEG streaming…");
                        if let Some(rfcomm) = rfcomm_task.take() {
                            rfcomm.shutdown();
                            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        }
                        log::info!("TUI: ⏸ EEG streaming paused");
                    }
                    #[cfg(feature = "rfcomm")]
                    Some(DataCmd::Resume) => {
                        log::info!("TUI: resuming EEG streaming…");
                        rfcomm_task = start_rfcomm(&handle, &bt_address).await;
                        // Mode transitions to Connected when first EEG packet arrives
                        log::info!("TUI: ✅ EEG streaming resumed");
                    }
                    #[cfg(not(feature = "rfcomm"))]
                    Some(DataCmd::Pause) => {
                        log::info!("TUI: pausing EEG streaming…");
                        if let Err(e) = handle.stop().await {
                            log::error!("TUI: failed to pause EEG: {e}");
                        } else {
                            log::info!("TUI: ⏸ EEG streaming paused");
                        }
                    }
                    #[cfg(not(feature = "rfcomm"))]
                    Some(DataCmd::Resume) => {
                        log::info!("TUI: resuming EEG streaming…");
                        match handle.start().await {
                            Ok(()) => log::info!("TUI: ✅ EEG streaming resumed"),
                            Err(e) => log::error!("TUI: failed to resume EEG: {e}"),
                        }
                    }
                    Some(DataCmd::ScanDevices) => {
                        handle_scan(app, sample_rate).await;
                    }
                    Some(DataCmd::ConnectDevice(idx)) => {
                        // Tear down current connection
                        #[cfg(feature = "rfcomm")]
                        if let Some(rfcomm) = rfcomm_task.take() {
                            rfcomm.shutdown();
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                        // Try connecting to the new device
                        if let Some((drx, new_handle)) =
                            handle_connect_device(app, sample_rate, idx).await
                        {
                            // Recurse into a new session
                            // (the current session's handle/rfcomm are dropped)
                            return Box::pin(run_connected_session(
                                app, cmd_rx, tx, rx, drx, new_handle, sample_rate,
                            )).await;
                        }
                    }
                    None => break, // cmd channel closed (UI exited)
                }
            }
        }
    }

    // Clean up RFCOMM on exit
    #[cfg(feature = "rfcomm")]
    if let Some(rfcomm) = rfcomm_task {
        rfcomm.shutdown();
    }
}
