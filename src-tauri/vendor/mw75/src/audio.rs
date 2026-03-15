//! Bluetooth A2DP audio management for MW75 Neuro headphones (Linux only).
//!
//! This module provides automatic discovery, pairing, A2DP connection, and
//! PulseAudio/PipeWire audio sink routing for the MW75 headphones — all from
//! Rust with no user interaction required.
//!
//! Requires the `audio` feature flag:
//!
//! ```toml
//! [dependencies]
//! mw75 = { version = "0.1.0", features = ["audio"] }
//! ```
//!
//! # Architecture
//!
//! ```text
//! BlueZ D-Bus ──► discover MW75
//!                  ├── pair (if needed)
//!                  ├── connect A2DP profile
//!                  └── trust device
//!
//! pactl CLI ────► set MW75 as default audio sink
//!
//! rodio ────────► decode audio file → play to default sink
//! ```
//!
//! # Platform support
//!
//! | Platform | Support | Backend |
//! |---|---|---|
//! | Linux | ✓ | BlueZ D-Bus (`bluer`) + PipeWire/PulseAudio (`pactl`) |
//! | macOS | ✗ | Would need IOBluetooth + CoreAudio (not implemented) |
//! | Windows | ✗ | Would need Windows.Devices.Bluetooth (not implemented) |
//!
//! # Example
//!
//! ```no_run
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! use mw75::audio::{Mw75Audio, AudioConfig};
//!
//! let mut audio = Mw75Audio::new(AudioConfig::default());
//!
//! // Discover, pair, connect A2DP, set as audio sink — all automatic
//! let device = audio.connect().await?;
//! println!("Connected: {}", device.name);
//!
//! // Play a file through the headphones
//! audio.play_file("music.mp3").await?;
//!
//! // Later: disconnect and restore previous audio sink
//! audio.disconnect().await?;
//! # Ok(())
//! # }
//! ```

use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use bluer::{Adapter, Address};
use log::{debug, info, warn};
use tokio::process::Command;

use crate::protocol::MW75_DEVICE_NAME_PATTERN;

// ── A2DP Profile UUID ─────────────────────────────────────────────────────────

/// Bluetooth A2DP Sink profile UUID.
///
/// Standard UUID for Advanced Audio Distribution Profile (sink role).
/// The MW75 advertises this profile for music playback.
#[allow(dead_code)]
const A2DP_SINK_UUID: bluer::Uuid = bluer::Uuid::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb);

// ── AudioConfig ───────────────────────────────────────────────────────────────

/// Configuration for [`Mw75Audio`].
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Device name pattern for discovery (case-insensitive substring match).
    /// Default: `"MW75"`.
    pub name_pattern: String,
    /// Discovery timeout in seconds. Default: `10`.
    pub discovery_timeout_secs: u64,
    /// Whether to automatically set the MW75 as the default PulseAudio/PipeWire
    /// audio sink after connecting. Default: `true`.
    pub auto_set_sink: bool,
    /// Playback volume (0.0–1.0). Default: `0.8`.
    pub volume: f32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            name_pattern: MW75_DEVICE_NAME_PATTERN.into(),
            discovery_timeout_secs: 10,
            auto_set_sink: true,
            volume: 0.8,
        }
    }
}

// ── AudioDevice ───────────────────────────────────────────────────────────────

/// A discovered and connected MW75 audio device.
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Advertised device name (e.g. `"MW75 Neuro"`).
    pub name: String,
    /// Bluetooth MAC address.
    pub address: Address,
    /// Whether the device was already paired before we connected.
    pub was_paired: bool,
    /// PulseAudio/PipeWire sink name (set after `set_as_default_sink`).
    pub sink_name: Option<String>,
}

// ── Mw75Audio ─────────────────────────────────────────────────────────────────

/// Bluetooth audio manager for MW75 Neuro headphones.
///
/// Handles the complete audio lifecycle:
/// 1. Discovery via BlueZ D-Bus
/// 2. Pairing (if not already paired)
/// 3. A2DP profile connection
/// 4. PulseAudio/PipeWire sink routing
/// 5. Audio file playback via rodio
/// 6. Clean disconnection
pub struct Mw75Audio {
    config: AudioConfig,
    adapter: Option<Adapter>,
    device: Option<AudioDevice>,
    /// The default sink name before we switched to MW75, for restoration.
    previous_sink: Option<String>,
}

impl Mw75Audio {
    /// Create a new audio manager with the given configuration.
    pub fn new(config: AudioConfig) -> Self {
        Self {
            config,
            adapter: None,
            device: None,
            previous_sink: None,
        }
    }

    /// Discover, pair, and connect to the MW75 headphones over A2DP.
    ///
    /// This performs the complete connection sequence:
    /// 1. Get the default BlueZ adapter
    /// 2. Start discovery and find the MW75
    /// 3. Pair if not already paired
    /// 4. Trust the device (for auto-reconnect)
    /// 5. Connect (which establishes the A2DP audio profile)
    /// 6. Optionally set as default audio sink via `pactl`
    ///
    /// Returns the connected [`AudioDevice`] on success.
    pub async fn connect(&mut self) -> Result<AudioDevice> {
        // Get BlueZ adapter
        let session = bluer::Session::new().await
            .context("Failed to connect to BlueZ D-Bus session")?;
        let adapter = session.default_adapter().await
            .context("No Bluetooth adapter found")?;
        adapter.set_powered(true).await
            .context("Failed to power on Bluetooth adapter")?;

        info!("Bluetooth adapter: {}", adapter.name());

        // Discover MW75
        let (address, name) = self
            .discover_mw75(&adapter)
            .await
            .context("MW75 discovery failed")?;

        info!("Found MW75: {name} [{address}]");

        let device = adapter.device(address)?;

        // Check if already paired
        let was_paired = device.is_paired().await.unwrap_or(false);

        // Pair if needed
        if !was_paired {
            info!("Pairing with {name}…");
            device.pair().await.context("Pairing failed")?;
            info!("Paired successfully");
        } else {
            info!("Already paired with {name}");
        }

        // Trust the device for auto-reconnect
        if !device.is_trusted().await.unwrap_or(false) {
            device.set_trusted(true).await.ok();
            info!("Device trusted for auto-reconnect");
        }

        // Connect (establishes A2DP + other profiles)
        if !device.is_connected().await.unwrap_or(false) {
            info!("Connecting A2DP…");
            device.connect().await.context("A2DP connection failed")?;
            // Give BlueZ/PipeWire time to set up the audio sink
            tokio::time::sleep(Duration::from_secs(2)).await;
            info!("A2DP connected");
        } else {
            info!("Already connected");
        }

        let mut audio_device = AudioDevice {
            name: name.clone(),
            address,
            was_paired,
            sink_name: None,
        };

        // Set as default audio sink
        if self.config.auto_set_sink {
            match self.set_as_default_sink(&address).await {
                Ok(sink_name) => {
                    info!("Audio sink set to: {sink_name}");
                    audio_device.sink_name = Some(sink_name);
                }
                Err(e) => {
                    warn!("Could not set as default sink (audio may still work): {e}");
                }
            }
        }

        self.adapter = Some(adapter);
        self.device = Some(audio_device.clone());

        Ok(audio_device)
    }

    /// Play an audio file through the MW75 headphones.
    ///
    /// Supports formats: MP3, WAV, FLAC, OGG/Vorbis (via rodio).
    ///
    /// This function blocks until playback finishes. For non-blocking playback,
    /// spawn it in a separate task:
    ///
    /// ```no_run
    /// # async fn example(audio: &mw75::audio::Mw75Audio) {
    /// let handle = tokio::task::spawn_blocking(|| {
    ///     // audio.play_file_blocking("song.mp3")
    /// });
    /// # }
    /// ```
    pub async fn play_file(&self, path: &str) -> Result<()> {
        let path = path.to_string();
        let volume = self.config.volume;

        // rodio is sync — run on blocking thread pool
        tokio::task::spawn_blocking(move || {
            Self::play_file_sync(&path, volume)
        })
        .await
        .context("Playback task panicked")?
    }

    /// Synchronous audio file playback (runs on current thread).
    ///
    /// Blocks until the file finishes playing.
    pub fn play_file_sync(path: &str, volume: f32) -> Result<()> {
        use rodio::{Decoder, OutputStream, Sink};
        use std::fs::File;
        use std::io::BufReader;

        info!("Playing: {path}");

        let (_stream, stream_handle) = OutputStream::try_default()
            .context("No audio output device available")?;
        let sink = Sink::try_new(&stream_handle)
            .context("Failed to create audio sink")?;

        let file = File::open(path)
            .with_context(|| format!("Cannot open audio file: {path}"))?;
        let source = Decoder::new(BufReader::new(file))
            .with_context(|| format!("Cannot decode audio file: {path}"))?;

        sink.set_volume(volume.clamp(0.0, 1.0));
        sink.append(source);

        info!("Playback started (volume={:.0}%)", volume * 100.0);
        sink.sleep_until_end();
        info!("Playback finished");

        Ok(())
    }

    /// Disconnect from the MW75 and restore the previous audio sink.
    pub async fn disconnect(&mut self) -> Result<()> {
        // Restore previous audio sink
        if let Some(ref prev) = self.previous_sink {
            info!("Restoring previous audio sink: {prev}");
            let _ = run_pactl(&["set-default-sink", prev]).await;
        }

        // Disconnect Bluetooth
        if let (Some(adapter), Some(dev)) = (&self.adapter, &self.device) {
            let device = adapter.device(dev.address)?;
            if device.is_connected().await.unwrap_or(false) {
                info!("Disconnecting A2DP from {}…", dev.name);
                device.disconnect().await.ok();
                info!("Disconnected");
            }
        }

        self.device = None;
        self.adapter = None;
        self.previous_sink = None;

        Ok(())
    }

    /// Check if the MW75 is currently connected.
    pub async fn is_connected(&self) -> bool {
        if let (Some(adapter), Some(dev)) = (&self.adapter, &self.device) {
            if let Ok(device) = adapter.device(dev.address) {
                return device.is_connected().await.unwrap_or(false);
            }
        }
        false
    }

    /// Get the currently connected device info, if any.
    pub fn connected_device(&self) -> Option<&AudioDevice> {
        self.device.as_ref()
    }

    // ── Private: discovery ───────────────────────────────────────────────────

    async fn discover_mw75(&self, adapter: &Adapter) -> Result<(Address, String)> {
        use futures::StreamExt;

        let pattern = self.config.name_pattern.to_uppercase();
        let timeout = Duration::from_secs(self.config.discovery_timeout_secs);

        // First check already-known devices (previously paired)
        for addr in adapter.device_addresses().await? {
            if let Ok(device) = adapter.device(addr) {
                if let Ok(Some(name)) = device.name().await {
                    if name.to_uppercase().contains(&pattern) {
                        info!("Found already-known MW75: {name} [{addr}]");
                        return Ok((addr, name));
                    }
                }
            }
        }

        // Start active discovery
        info!(
            "Starting Bluetooth discovery (timeout: {} s)…",
            self.config.discovery_timeout_secs
        );

        let mut discover = adapter
            .discover_devices()
            .await
            .context("Failed to start discovery")?;

        let result = tokio::time::timeout(timeout, async {
            while let Some(event) = discover.next().await {
                if let bluer::AdapterEvent::DeviceAdded(addr) = event {
                    if let Ok(device) = adapter.device(addr) {
                        if let Ok(Some(name)) = device.name().await {
                            debug!("Discovered: {name} [{addr}]");
                            if name.to_uppercase().contains(&pattern) {
                                return Ok((addr, name));
                            }
                        }
                    }
                }
            }
            Err(anyhow!("Discovery stream ended without finding MW75"))
        })
        .await;

        match result {
            Ok(r) => r,
            Err(_) => Err(anyhow!(
                "MW75 not found within {} s — is it powered on and in range?",
                self.config.discovery_timeout_secs
            )),
        }
    }

    // ── Private: PulseAudio/PipeWire sink management ─────────────────────────

    async fn set_as_default_sink(&mut self, address: &Address) -> Result<String> {
        // Save current default sink for later restoration
        self.previous_sink = get_default_sink().await.ok();

        // Find the BlueTooth sink matching this address
        // PipeWire/PulseAudio creates sinks like "bluez_output.XX_XX_XX_XX_XX_XX.1"
        let addr_str = address.to_string().replace(':', "_");
        let sink_name = find_bt_sink(&addr_str).await
            .with_context(|| format!("No PulseAudio/PipeWire sink found for {address}"))?;

        // Set as default
        run_pactl(&["set-default-sink", &sink_name]).await
            .context("Failed to set default sink")?;

        Ok(sink_name)
    }
}

// ── pactl helpers ─────────────────────────────────────────────────────────────

/// Run a `pactl` command and return stdout.
async fn run_pactl(args: &[&str]) -> Result<String> {
    let output = Command::new("pactl")
        .args(args)
        .output()
        .await
        .context("Failed to run pactl — is PulseAudio/PipeWire installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("pactl {} failed: {}", args.join(" "), stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the current default PulseAudio/PipeWire sink name.
async fn get_default_sink() -> Result<String> {
    run_pactl(&["get-default-sink"]).await
}

/// Find a Bluetooth audio sink matching the given address pattern.
///
/// Parses `pactl list sinks short` output, which looks like:
/// ```text
/// 42  bluez_output.AA_BB_CC_DD_EE_FF.1  PipeWiremodule  s16le 2ch 48000Hz  RUNNING
/// ```
async fn find_bt_sink(addr_pattern: &str) -> Result<String> {
    let output = run_pactl(&["list", "sinks", "short"]).await?;

    for line in output.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 2 && fields[1].contains(addr_pattern) {
            return Ok(fields[1].to_string());
        }
    }

    Err(anyhow!(
        "No Bluetooth sink matching '{}' in pactl output:\n{}",
        addr_pattern,
        output
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_config_defaults() {
        let cfg = AudioConfig::default();
        assert_eq!(cfg.name_pattern, "MW75");
        assert_eq!(cfg.discovery_timeout_secs, 10);
        assert!(cfg.auto_set_sink);
        assert!((cfg.volume - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn audio_config_custom() {
        let cfg = AudioConfig {
            name_pattern: "TEST".into(),
            discovery_timeout_secs: 30,
            auto_set_sink: false,
            volume: 0.5,
        };
        assert_eq!(cfg.name_pattern, "TEST");
        assert!(!cfg.auto_set_sink);
    }

    #[test]
    fn audio_device_clone() {
        let dev = AudioDevice {
            name: "MW75 Neuro".into(),
            address: Address::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
            was_paired: true,
            sink_name: Some("bluez_output.AA_BB_CC".into()),
        };
        let cloned = dev.clone();
        assert_eq!(cloned.name, dev.name);
        assert_eq!(cloned.address, dev.address);
        assert_eq!(cloned.sink_name, dev.sink_name);
    }

    #[test]
    fn mw75_audio_initial_state() {
        let audio = Mw75Audio::new(AudioConfig::default());
        assert!(audio.device.is_none());
        assert!(audio.adapter.is_none());
        assert!(audio.previous_sink.is_none());
        assert!(audio.connected_device().is_none());
    }
}
