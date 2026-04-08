//! BrainMaster TUI — real-time EEG display in the terminal.
//!
//! Usage:
//!   cargo run -p brainmaster --features tui -- [--port /dev/ttyUSB0] [--model discovery]
//!
//! Models: atlantis2, atlantis4, discovery, freedom

use brainmaster::prelude::*;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "tui")]
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
#[cfg(feature = "tui")]
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Sparkline},
    Terminal,
};

fn parse_model(s: &str) -> DeviceModel {
    match s.to_lowercase().as_str() {
        "atlantis2" | "a2" => DeviceModel::Atlantis2,
        "atlantis4" | "a4" => DeviceModel::Atlantis4,
        "discovery" | "d24" => DeviceModel::Discovery,
        "freedom" | "f24" => DeviceModel::Freedom,
        _ => DeviceModel::Atlantis4,
    }
}

fn main() -> anyhow::Result<()> {
    let mut port_arg: Option<String> = None;
    let mut model_arg = "atlantis4".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" | "-p" => port_arg = args.next(),
            "--model" | "-m" => {
                if let Some(m) = args.next() {
                    model_arg = m;
                }
            }
            "--help" | "-h" => {
                println!("brainmaster TUI — real-time EEG viewer");
                println!("  --port  <path>   Serial port (auto-detect if omitted)");
                println!("  --model <name>   atlantis2|atlantis4|discovery|freedom");
                println!("  --help           Show this help");
                return Ok(());
            }
            _ => {}
        }
    }

    let model = parse_model(&model_arg);
    let port = if let Some(p) = port_arg {
        p
    } else {
        let ports = BrainMasterDevice::scan()?;
        ports
            .into_iter()
            .next()
            .ok_or("No BrainMaster serial port found. Use --port to specify.")?
    };

    println!(
        "Connecting to BrainMaster {:?} on {} ({} ch, {} Hz)...",
        model,
        port,
        model.channel_count(),
        model.sample_rate()
    );

    let mut device = BrainMasterDevice::open(&port, model)?;
    device.start_streaming()?;

    #[cfg(feature = "tui")]
    {
        run_tui(&mut device)?;
    }

    #[cfg(not(feature = "tui"))]
    {
        // Fallback: simple text output
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        ctrlc::set_handler(move || r.store(false, Ordering::Relaxed)).ok();

        let mut count = 0u64;
        while running.load(Ordering::Relaxed) {
            match device.read_sample() {
                Ok(s) => {
                    count += 1;
                    if count % (model.sample_rate() as u64) == 0 {
                        let preview: Vec<String> = s.channels.iter().take(4).map(|v| format!("{v:>8.2}")).collect();
                        println!("[{count:>6}] {}", preview.join(" "));
                    }
                }
                Err(BrainMasterError::Timeout) => continue,
                Err(e) => {
                    eprintln!("Error: {e}");
                    break;
                }
            }
        }
    }

    device.stop_streaming()?;
    println!("Done.");
    Ok(())
}

#[cfg(feature = "tui")]
fn run_tui(device: &mut BrainMasterDevice) -> anyhow::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let n_ch = device.channel_count();
    let model = device.model();
    let names = model.channel_names();
    let rate = model.sample_rate() as usize;

    // Ring buffers for sparklines (last 2 seconds per channel).
    let buf_size = rate * 2;
    let mut ring: Vec<Vec<u64>> = (0..n_ch).map(|_| vec![0u64; buf_size]).collect();
    let mut write_pos = 0usize;
    let mut sample_count = 0u64;
    let start = Instant::now();

    loop {
        // Poll for key press (non-blocking).
        if event::poll(Duration::from_millis(1))? {
            if let Event::Key(key) = event::read()? {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break;
                }
            }
        }

        // Read a batch of samples.
        for _ in 0..4 {
            match device.read_sample() {
                Ok(s) => {
                    sample_count += 1;
                    for (ch, &uv) in s.channels.iter().enumerate().take(n_ch) {
                        // Map µV to 0..100 for sparkline (centered at 50).
                        let norm = ((uv / 100.0) * 50.0 + 50.0).clamp(0.0, 100.0) as u64;
                        ring[ch][write_pos % buf_size] = norm;
                    }
                    write_pos += 1;
                }
                Err(BrainMasterError::Timeout) => break,
                Err(_) => break,
            }
        }

        // Draw.
        let elapsed = start.elapsed().as_secs_f64();
        let hz = if elapsed > 0.0 {
            sample_count as f64 / elapsed
        } else {
            0.0
        };

        terminal.draw(|f| {
            let display_ch = n_ch.min(8); // Show max 8 channels in TUI
            let mut constraints: Vec<Constraint> = (0..display_ch)
                .map(|_| Constraint::Ratio(1, display_ch as u32))
                .collect();
            constraints.push(Constraint::Length(1)); // status bar

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(f.area());

            for ch in 0..display_ch {
                let data: Vec<u64> = {
                    let start_idx = if write_pos >= buf_size { write_pos - buf_size } else { 0 };
                    let end_idx = write_pos;
                    (start_idx..end_idx).map(|i| ring[ch][i % buf_size]).collect()
                };

                let label = if ch < names.len() { names[ch] } else { "?" };
                let block = Block::default().title(format!(" {label} ")).borders(Borders::ALL);
                let sparkline = Sparkline::default()
                    .block(block)
                    .data(&data)
                    .max(100)
                    .style(Style::default().fg(Color::Cyan));
                f.render_widget(sparkline, chunks[ch]);
            }

            // Status bar.
            let status = Line::from(vec![Span::styled(
                format!(
                    " {:?} | {} ch | {:.0} Hz | {} samples | press 'q' to quit ",
                    model, n_ch, hz, sample_count
                ),
                Style::default().fg(Color::Green),
            )]);
            f.render_widget(Paragraph::new(status), chunks[display_ch]);
        })?;
    }

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
