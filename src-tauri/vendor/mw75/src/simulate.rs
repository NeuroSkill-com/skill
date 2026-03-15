//! Synthetic EEG packet generator for development and testing.
//!
//! Generates valid 63-byte MW75 EEG packets with realistic random data at
//! 500 Hz, mirroring the Python `MockRFCOMMManager` / `build_eeg_packet`.
//!
//! # Usage
//!
//! ```
//! use mw75::simulate::build_eeg_packet;
//!
//! let packet = build_eeg_packet(0);
//! assert_eq!(packet.len(), 63);
//! assert_eq!(packet[0], 0xAA);   // sync byte
//! assert_eq!(packet[1], 239);    // EEG event ID
//! ```

use rand::Rng;

use crate::protocol::{EEG_EVENT_ID, NUM_EEG_CHANNELS, PACKET_SIZE, SYNC_BYTE, SampleRate};

/// Build a complete 63-byte EEG packet with a valid checksum.
///
/// Generates a packet with:
/// * Valid header (sync byte `0xAA`, event ID `239`, data length, counter)
/// * Random REF/DRL values in the range ±50.0
/// * Random EEG channel ADC values in the range ±8000.0
///   (after scaling by `EEG_SCALING_FACTOR` this yields ≈ ±190 µV)
/// * Correct 16-bit little-endian checksum
///
/// # Arguments
///
/// * `counter` — Packet counter byte (0–255).  Only the low 8 bits are used.
///
/// # Example
///
/// ```
/// # use mw75::simulate::build_eeg_packet;
/// let pkt = build_eeg_packet(42);
/// assert_eq!(pkt.len(), 63);
/// assert_eq!(pkt[0], 0xAA);
/// assert_eq!(pkt[1], 239);
/// assert_eq!(pkt[3], 42);
/// ```
pub fn build_eeg_packet(counter: u8) -> Vec<u8> {
    let mut rng = rand::rng();
    let mut packet = vec![0u8; PACKET_SIZE];

    // Header
    packet[0] = SYNC_BYTE;
    packet[1] = EEG_EVENT_ID;
    packet[2] = 0x3C; // data length (60 bytes)
    packet[3] = counter;

    // REF and DRL values (f32 LE)
    let ref_val: f32 = rng.random_range(-50.0..50.0);
    let drl_val: f32 = rng.random_range(-50.0..50.0);
    packet[4..8].copy_from_slice(&ref_val.to_le_bytes());
    packet[8..12].copy_from_slice(&drl_val.to_le_bytes());

    // 12 EEG channels (f32 LE raw ADC values)
    for ch in 0..NUM_EEG_CHANNELS {
        let offset = 12 + ch * 4;
        let raw_adc: f32 = rng.random_range(-8000.0..8000.0);
        packet[offset..offset + 4].copy_from_slice(&raw_adc.to_le_bytes());
    }

    // Feature status
    packet[60] = 0x00;

    // Checksum: sum of bytes 0..61, masked to 16 bits, LE
    let checksum: u16 = packet[..61].iter().map(|&b| b as u16).sum::<u16>() & 0xFFFF;
    packet[61] = (checksum & 0xFF) as u8;
    packet[62] = (checksum >> 8) as u8;

    packet
}

/// Build a synthetic EEG packet with deterministic sinusoidal channel data.
///
/// Useful for TUI visualisation where smooth waveforms are more meaningful
/// than random noise.
///
/// Each channel produces a superposition of:
/// * Alpha band (10 Hz, ±3000 ADC → ≈ ±72 µV)
/// * Beta band (22 Hz, ±1000 ADC → ≈ ±24 µV)
/// * Theta band (6 Hz, ±1500 ADC → ≈ ±36 µV)
/// * Deterministic noise (±500 ADC → ≈ ±12 µV)
///
/// Peak amplitude ≈ ±144 µV after scaling.
///
/// # Arguments
///
/// * `counter` — Packet counter (0–255)
/// * `t` — Time in seconds since simulation start
pub fn build_sim_packet(counter: u8, t: f64) -> Vec<u8> {
    use std::f64::consts::PI;

    let mut packet = vec![0u8; PACKET_SIZE];

    // Header
    packet[0] = SYNC_BYTE;
    packet[1] = EEG_EVENT_ID;
    packet[2] = 0x3C;
    packet[3] = counter;

    // REF and DRL: slow drift
    let ref_val = (5.0 * (2.0 * PI * 0.1 * t).sin()) as f32;
    let drl_val = (3.0 * (2.0 * PI * 0.15 * t).cos()) as f32;
    packet[4..8].copy_from_slice(&ref_val.to_le_bytes());
    packet[8..12].copy_from_slice(&drl_val.to_le_bytes());

    // 12 EEG channels with per-channel phase offsets
    for ch in 0..NUM_EEG_CHANNELS {
        let phi = ch as f64 * PI / 4.0;
        let alpha = 3000.0 * (2.0 * PI * 10.0 * t + phi).sin();
        let beta = 1000.0 * (2.0 * PI * 22.0 * t + phi * 1.7).sin();
        let theta = 1500.0 * (2.0 * PI * 6.0 * t + phi * 0.9).sin();
        // Deterministic pseudo-noise
        let nx = t * 1000.7 + ch as f64 * 137.508;
        let noise = ((nx.sin() * 9973.1).fract() - 0.5) * 1000.0;
        let raw_adc = (alpha + beta + theta + noise) as f32;

        let offset = 12 + ch * 4;
        packet[offset..offset + 4].copy_from_slice(&raw_adc.to_le_bytes());
    }

    packet[60] = 0x00;

    let checksum: u16 = packet[..61].iter().map(|&b| b as u16).sum::<u16>() & 0xFFFF;
    packet[61] = (checksum & 0xFF) as u8;
    packet[62] = (checksum >> 8) as u8;

    packet
}

/// Spawn a background task that generates synthetic EEG packets at the
/// specified sample rate and sends them through the provided channel.
///
/// Returns a [`tokio::task::JoinHandle`] that can be aborted to stop the
/// simulation.
///
/// # Arguments
///
/// * `tx` — Channel sender for [`crate::types::Mw75Event`] values.
/// * `deterministic` — If `true`, uses [`build_sim_packet`] (sinusoidal)
///   instead of [`build_eeg_packet`] (random).
/// * `sample_rate` — The streaming rate (256 Hz or 500 Hz).
pub fn spawn_simulator(
    tx: tokio::sync::mpsc::Sender<crate::types::Mw75Event>,
    deterministic: bool,
) -> tokio::task::JoinHandle<()> {
    spawn_simulator_with_rate(tx, deterministic, SampleRate::default())
}

/// Like [`spawn_simulator`] but with an explicit [`SampleRate`].
pub fn spawn_simulator_with_rate(
    tx: tokio::sync::mpsc::Sender<crate::types::Mw75Event>,
    deterministic: bool,
    sample_rate: SampleRate,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        use crate::parse::PacketProcessor;

        let mut counter: u8 = 0;
        let mut processor = PacketProcessor::new(false);
        let interval = tokio::time::Duration::from_micros(sample_rate.interval_micros());
        let mut ticker = tokio::time::interval(interval);
        let start = tokio::time::Instant::now();

        // Send initial connected event
        let _ = tx
            .send(crate::types::Mw75Event::Connected("MW75-SIM".into()))
            .await;
        let _ = tx
            .send(crate::types::Mw75Event::Activated(
                crate::types::ActivationStatus {
                    eeg_enabled: true,
                    raw_mode_enabled: sample_rate.needs_raw_mode(),
                },
            ))
            .await;
        let _ = tx
            .send(crate::types::Mw75Event::Battery(
                crate::types::BatteryInfo { level: 85 },
            ))
            .await;

        loop {
            ticker.tick().await;
            let t = start.elapsed().as_secs_f64();

            let packet = if deterministic {
                build_sim_packet(counter, t)
            } else {
                build_eeg_packet(counter)
            };

            let events = processor.process_data(&packet);
            for event in events {
                if tx.send(event).await.is_err() {
                    return; // Receiver dropped
                }
            }

            counter = counter.wrapping_add(1);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::{validate_checksum, PacketProcessor};
    use crate::protocol::{EEG_EVENT_ID, PACKET_SIZE, SYNC_BYTE};

    #[test]
    fn test_build_eeg_packet_structure() {
        let pkt = build_eeg_packet(0);
        assert_eq!(pkt.len(), PACKET_SIZE);
        assert_eq!(pkt[0], SYNC_BYTE);
        assert_eq!(pkt[1], EEG_EVENT_ID);
        assert_eq!(pkt[3], 0);
    }

    #[test]
    fn test_build_eeg_packet_valid_checksum() {
        for counter in 0..10 {
            let pkt = build_eeg_packet(counter);
            let (valid, _, _) = validate_checksum(&pkt);
            assert!(valid, "Packet with counter={counter} has invalid checksum");
        }
    }

    #[test]
    fn test_build_eeg_packet_counter_wraps() {
        let pkt = build_eeg_packet(255);
        assert_eq!(pkt[3], 255);
    }

    #[test]
    fn test_build_sim_packet_structure() {
        let pkt = build_sim_packet(42, 1.0);
        assert_eq!(pkt.len(), PACKET_SIZE);
        assert_eq!(pkt[0], SYNC_BYTE);
        assert_eq!(pkt[1], EEG_EVENT_ID);
        assert_eq!(pkt[3], 42);
    }

    #[test]
    fn test_build_sim_packet_valid_checksum() {
        for i in 0..20 {
            let t = i as f64 * 0.002;
            let pkt = build_sim_packet(i as u8, t);
            let (valid, _, _) = validate_checksum(&pkt);
            assert!(valid, "Sim packet at t={t} has invalid checksum");
        }
    }

    #[test]
    fn test_build_sim_packet_deterministic() {
        let a = build_sim_packet(5, 0.5);
        let b = build_sim_packet(5, 0.5);
        assert_eq!(a, b, "Same inputs should produce identical packets");
    }

    #[test]
    fn test_build_sim_packet_varies_with_time() {
        let a = build_sim_packet(0, 0.0);
        let b = build_sim_packet(0, 0.1);
        // Channel data should differ (bytes 12..60)
        assert_ne!(&a[12..60], &b[12..60]);
    }

    #[test]
    fn test_build_eeg_packet_parseable() {
        let pkt = build_eeg_packet(7);
        let mut proc = PacketProcessor::new(false);
        let events = proc.process_data(&pkt);
        assert_eq!(events.len(), 1);
        match &events[0] {
            crate::types::Mw75Event::Eeg(eeg) => {
                assert_eq!(eeg.counter, 7);
                assert_eq!(eeg.channels.len(), 12);
                assert!(eeg.checksum_valid);
            }
            _ => panic!("Expected Eeg event"),
        }
    }

    #[test]
    fn test_build_sim_packet_parseable() {
        let pkt = build_sim_packet(99, 2.5);
        let mut proc = PacketProcessor::new(false);
        let events = proc.process_data(&pkt);
        assert_eq!(events.len(), 1);
        match &events[0] {
            crate::types::Mw75Event::Eeg(eeg) => {
                assert_eq!(eeg.counter, 99);
                assert_eq!(eeg.channels.len(), 12);
                // Check that channel values are in a reasonable µV range
                for &v in &eeg.channels {
                    assert!(
                        v.abs() < 250.0,
                        "Channel value {v} out of expected ±250 µV range"
                    );
                }
            }
            _ => panic!("Expected Eeg event"),
        }
    }

    #[tokio::test]
    async fn test_spawn_simulator_produces_events() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(256);
        let handle = spawn_simulator(tx, true);

        // Collect a few events
        let mut eeg_count = 0;
        let timeout = tokio::time::Duration::from_millis(100);
        let deadline = tokio::time::Instant::now() + timeout;

        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(
                tokio::time::Duration::from_millis(10),
                rx.recv(),
            )
            .await
            {
                Ok(Some(crate::types::Mw75Event::Eeg(_))) => eeg_count += 1,
                Ok(Some(_)) => {} // Connected, Activated, Battery
                _ => break,
            }
        }

        handle.abort();
        assert!(eeg_count > 0, "Should have received at least one EEG event");
    }
}
