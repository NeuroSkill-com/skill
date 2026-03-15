//! Binary decoders for MW75 BLE notification and RFCOMM data payloads.
//!
//! All public functions in this module are pure (no I/O, no allocation beyond
//! the returned collections) and are safe to call from any async or sync context.
//!
//! # Packet layout
//!
//! The MW75 streams 63-byte binary packets at 500 Hz over RFCOMM channel 25.
//! Each packet starts with a `0xAA` sync byte and ends with a 16-bit
//! little-endian checksum:
//!
//! ```text
//! Offset  Size  Field
//! ──────  ────  ─────
//!   0       1   Sync byte (0xAA)
//!   1       1   Event ID (239 = EEG)
//!   2       1   Data length
//!   3       1   Counter (0–255, wrapping)
//!   4       4   REF electrode value (f32 LE)
//!   8       4   DRL electrode value (f32 LE)
//!  12      48   12 × EEG channels (f32 LE each)
//!  60       1   Feature status byte
//!  61       2   Checksum (u16 LE = sum of bytes[0..61] & 0xFFFF)
//! ```
//!
//! # Buffered processing
//!
//! RFCOMM delivers arbitrary-sized chunks (e.g. 64 bytes) while MW75 packets
//! are exactly 63 bytes.  [`PacketProcessor`] accumulates data across chunks
//! and handles sync-byte alignment, checksum validation, and buffer overflow
//! protection.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::protocol::{
    EEG_EVENT_ID, EEG_SCALING_FACTOR, NUM_EEG_CHANNELS, PACKET_SIZE, SYNC_BYTE,
};
use crate::types::{ChecksumStats, EegPacket, Mw75Event};

// ── Timestamp helper ──────────────────────────────────────────────────────────

/// Return the current wall-clock time as seconds since Unix epoch.
fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_secs_f64()
}

// ── Checksum ──────────────────────────────────────────────────────────────────

/// Validate the MW75 packet checksum.
///
/// The MW75 checksum is computed as:
/// 1. Sum of the first 61 bytes (indices 0–60)
/// 2. Masked to 16 bits (`& 0xFFFF`)
/// 3. Stored as a little-endian `u16` at bytes 61–62
///
/// Returns `(is_valid, calculated_checksum, received_checksum)`.
///
/// Returns `(false, 0, 0)` if `packet` is shorter than [`PACKET_SIZE`].
///
/// # Example
///
/// ```
/// # use mw75::parse::validate_checksum;
/// let mut pkt = vec![0u8; 63];
/// pkt[0] = 0xAA;
/// // Set checksum to match: sum of bytes[0..61]
/// let sum: u16 = pkt[..61].iter().map(|&b| b as u16).sum();
/// pkt[61] = (sum & 0xFF) as u8;
/// pkt[62] = (sum >> 8) as u8;
/// let (valid, calc, recv) = validate_checksum(&pkt);
/// assert!(valid);
/// assert_eq!(calc, recv);
/// ```
pub fn validate_checksum(packet: &[u8]) -> (bool, u16, u16) {
    if packet.len() < PACKET_SIZE {
        return (false, 0, 0);
    }
    let calculated: u16 = packet[..61].iter().map(|&b| b as u16).sum::<u16>() & 0xFFFF;
    let received: u16 = packet[61] as u16 | ((packet[62] as u16) << 8);
    (calculated == received, calculated, received)
}

// ── EEG Packet Parsing ───────────────────────────────────────────────────────

/// Parse a 63-byte MW75 packet into a structured [`EegPacket`].
///
/// Returns `None` if:
/// * The packet is not exactly [`PACKET_SIZE`] (63) bytes
/// * The first byte is not [`SYNC_BYTE`] (`0xAA`)
/// * The checksum does not validate
///
/// Channel values are scaled from raw ADC floats to microvolts using
/// [`EEG_SCALING_FACTOR`] (`0.023842`).
///
/// # Wire format
///
/// See the [module-level documentation](self) for the complete packet layout.
///
/// # Example
///
/// ```
/// # use mw75::parse::parse_eeg_packet;
/// # use mw75::simulate::build_eeg_packet;
/// let pkt = build_eeg_packet(0);
/// let eeg = parse_eeg_packet(&pkt).expect("valid packet");
/// assert_eq!(eeg.channels.len(), 12);
/// assert!(eeg.checksum_valid);
/// ```
pub fn parse_eeg_packet(packet: &[u8]) -> Option<EegPacket> {
    if packet.len() != PACKET_SIZE || packet[0] != SYNC_BYTE {
        return None;
    }

    let (is_valid, _calc, _recv) = validate_checksum(packet);
    if !is_valid {
        return None;
    }

    let event_id = packet[1];
    let counter = packet[3];
    let timestamp = now_secs();

    // REF and DRL as f32 LE
    let ref_value = f32::from_le_bytes([packet[4], packet[5], packet[6], packet[7]]);
    let drl = f32::from_le_bytes([packet[8], packet[9], packet[10], packet[11]]);

    // 12 EEG channels, each f32 LE, scaled to µV
    let mut channels = Vec::with_capacity(NUM_EEG_CHANNELS);
    for ch in 0..NUM_EEG_CHANNELS {
        let offset = 12 + ch * 4;
        if offset + 4 <= packet.len() {
            let raw = f32::from_le_bytes([
                packet[offset],
                packet[offset + 1],
                packet[offset + 2],
                packet[offset + 3],
            ]);
            channels.push(raw * EEG_SCALING_FACTOR);
        }
    }

    let feature_status = if packet.len() > 60 { packet[60] } else { 0 };

    Some(EegPacket {
        timestamp,
        event_id,
        counter,
        ref_value,
        drl,
        channels,
        feature_status,
        checksum_valid: true,
    })
}

// ── Packet Processor (continuous buffer) ──────────────────────────────────────

/// Processes a continuous byte stream into MW75 packets.
///
/// Accumulates data across transport chunks (RFCOMM delivers arbitrary-sized
/// reads while packets are exactly 63 bytes) and handles sync-byte alignment.
///
/// Mirrors the Python `PacketProcessor` class.
///
/// # Features
///
/// * **Split delivery** — a packet that spans two `process_data` calls is
///   correctly reassembled.
/// * **Sync recovery** — garbage bytes before a sync byte are silently skipped.
/// * **Checksum validation** — invalid packets advance by 1 byte (not 63) to
///   avoid skipping a valid alignment when the payload contains `0xAA`.
/// * **Buffer overflow protection** — if the buffer grows beyond 10 packets
///   worth of data without producing output, it is truncated to the last
///   sync byte or cleared entirely.
/// * **Statistics tracking** — valid / invalid / total packet counts are
///   maintained in [`ChecksumStats`].
///
/// # Usage
///
/// ```
/// # use mw75::parse::PacketProcessor;
/// let mut proc = PacketProcessor::new(false);
/// // Feed raw bytes from the transport:
/// let events = proc.process_data(&[0xAA, /* ... 62 more bytes ... */]);
/// ```
pub struct PacketProcessor {
    /// Internal accumulation buffer.
    buffer: Vec<u8>,
    /// Running checksum statistics.
    pub stats: ChecksumStats,
    /// When `true`, log warnings for individual checksum failures.
    pub verbose: bool,
}

impl PacketProcessor {
    /// Create a new processor.
    ///
    /// Set `verbose` to `true` to enable per-packet checksum failure logging.
    pub fn new(verbose: bool) -> Self {
        Self {
            buffer: Vec::with_capacity(PACKET_SIZE * 10),
            stats: ChecksumStats::default(),
            verbose,
        }
    }

    /// Feed raw bytes from the transport and return any complete events.
    ///
    /// The processor maintains an internal buffer across calls.  Incomplete
    /// packets at the end of a chunk are retained until more data arrives.
    ///
    /// Returns a `Vec<Mw75Event>` containing:
    /// * [`Mw75Event::Eeg`] for valid EEG packets (event ID 239)
    /// * [`Mw75Event::OtherEvent`] for valid non-EEG packets
    pub fn process_data(&mut self, data: &[u8]) -> Vec<Mw75Event> {
        self.buffer.extend_from_slice(data);
        let mut events = Vec::new();

        let mut i = 0;
        while i < self.buffer.len() {
            if self.buffer[i] == SYNC_BYTE {
                // Do we have a full packet?
                if i + PACKET_SIZE <= self.buffer.len() {
                    let packet = &self.buffer[i..i + PACKET_SIZE];

                    // Validate checksum first
                    let (is_valid, calc, recv) = validate_checksum(packet);
                    self.stats.total_packets += 1;

                    if !is_valid {
                        self.stats.invalid_packets += 1;
                        if self.verbose {
                            log::warn!(
                                "Checksum mismatch: calc=0x{:04x} recv=0x{:04x} (event={}, counter={})",
                                calc, recv, packet[1], packet[3]
                            );
                        }
                        // Slide by 1 byte to avoid skipping a valid alignment
                        i += 1;
                        continue;
                    }

                    self.stats.valid_packets += 1;

                    if packet[1] == EEG_EVENT_ID {
                        if let Some(eeg) = parse_eeg_packet(packet) {
                            events.push(Mw75Event::Eeg(eeg));
                        }
                    } else {
                        events.push(Mw75Event::OtherEvent {
                            event_id: packet[1],
                            counter: packet[3],
                            raw: packet.to_vec(),
                        });
                    }

                    i += PACKET_SIZE;
                } else {
                    // Not enough data for a complete packet — wait for more
                    break;
                }
            } else {
                i += 1;
            }
        }

        // Remove processed data, keep the remainder
        if i > 0 {
            self.buffer.drain(..i);
        }

        // Prevent unbounded buffer growth
        let max_buf = PACKET_SIZE * 10;
        if self.buffer.len() > max_buf {
            // Try to find a sync byte near the end
            if let Some(pos) = self.buffer.iter().rposition(|&b| b == SYNC_BYTE) {
                self.buffer.drain(..pos);
                log::debug!("Buffer overflow — recovered sync at {pos}");
            } else {
                self.buffer.clear();
                log::warn!("Buffer overflow — no sync byte found, cleared");
            }
        }

        events
    }

    /// Return the number of bytes currently buffered (waiting for more data).
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Return a snapshot of the current checksum statistics.
    pub fn get_stats(&self) -> ChecksumStats {
        self.stats.clone()
    }

    /// Reset the internal buffer and statistics.
    pub fn reset(&mut self) {
        self.buffer.clear();
        self.stats = ChecksumStats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Build a minimal valid 63-byte packet with correct checksum.
    fn make_packet(event_id: u8, counter: u8) -> Vec<u8> {
        let mut pkt = vec![0u8; PACKET_SIZE];
        pkt[0] = SYNC_BYTE;
        pkt[1] = event_id;
        pkt[2] = 58; // data length
        pkt[3] = counter;
        // Leave REF/DRL/channels as zero — just need valid checksum
        fix_checksum(&mut pkt);
        pkt
    }

    /// Build a packet with specific channel f32 values (before scaling).
    fn make_packet_with_channels(counter: u8, raw_values: &[f32; 12]) -> Vec<u8> {
        let mut pkt = vec![0u8; PACKET_SIZE];
        pkt[0] = SYNC_BYTE;
        pkt[1] = EEG_EVENT_ID;
        pkt[2] = 58;
        pkt[3] = counter;
        // REF and DRL
        pkt[4..8].copy_from_slice(&42.0_f32.to_le_bytes());
        pkt[8..12].copy_from_slice(&(-7.5_f32).to_le_bytes());
        // Channels
        for (i, &val) in raw_values.iter().enumerate() {
            let off = 12 + i * 4;
            pkt[off..off + 4].copy_from_slice(&val.to_le_bytes());
        }
        pkt[60] = 0x01; // feature status
        fix_checksum(&mut pkt);
        pkt
    }

    /// Recalculate and store the checksum in-place.
    fn fix_checksum(pkt: &mut [u8]) {
        let sum: u16 = pkt[..61].iter().map(|&b| b as u16).sum::<u16>() & 0xFFFF;
        pkt[61] = (sum & 0xFF) as u8;
        pkt[62] = (sum >> 8) as u8;
    }

    // ── validate_checksum tests ──────────────────────────────────────────────

    #[test]
    fn checksum_valid_packet() {
        let pkt = make_packet(EEG_EVENT_ID, 0);
        let (valid, calc, recv) = validate_checksum(&pkt);
        assert!(valid);
        assert_eq!(calc, recv);
    }

    #[test]
    fn checksum_invalid_corrupted_byte() {
        let mut pkt = make_packet(EEG_EVENT_ID, 0);
        pkt[62] ^= 0xFF; // corrupt checksum high byte
        let (valid, _, _) = validate_checksum(&pkt);
        assert!(!valid);
    }

    #[test]
    fn checksum_invalid_corrupted_payload() {
        let mut pkt = make_packet(EEG_EVENT_ID, 10);
        pkt[30] = 0xFF; // corrupt a data byte
        let (valid, _, _) = validate_checksum(&pkt);
        assert!(!valid);
    }

    #[test]
    fn checksum_too_short() {
        let (valid, calc, recv) = validate_checksum(&[0xAA, 0x00]);
        assert!(!valid);
        assert_eq!(calc, 0);
        assert_eq!(recv, 0);
    }

    #[test]
    fn checksum_empty() {
        let (valid, _, _) = validate_checksum(&[]);
        assert!(!valid);
    }

    #[test]
    fn checksum_exact_minimum_length() {
        // Exactly PACKET_SIZE bytes (63)
        let pkt = make_packet(EEG_EVENT_ID, 0);
        assert_eq!(pkt.len(), 63);
        let (valid, _, _) = validate_checksum(&pkt);
        assert!(valid);
    }

    #[test]
    fn checksum_longer_than_packet_still_valid() {
        // Extra bytes after PACKET_SIZE are ignored by validate_checksum
        let mut pkt = make_packet(EEG_EVENT_ID, 0);
        pkt.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        let (valid, _, _) = validate_checksum(&pkt);
        assert!(valid);
    }

    // ── parse_eeg_packet tests ───────────────────────────────────────────────

    #[test]
    fn parse_basic_eeg_packet() {
        let pkt = make_packet(EEG_EVENT_ID, 42);
        let eeg = parse_eeg_packet(&pkt).expect("should parse");
        assert_eq!(eeg.event_id, EEG_EVENT_ID);
        assert_eq!(eeg.counter, 42);
        assert_eq!(eeg.channels.len(), NUM_EEG_CHANNELS);
        assert!(eeg.checksum_valid);
        assert!(eeg.timestamp > 0.0);
    }

    #[test]
    fn parse_rejects_wrong_sync_byte() {
        let mut pkt = make_packet(EEG_EVENT_ID, 0);
        pkt[0] = 0xBB; // wrong sync
        fix_checksum(&mut pkt);
        assert!(parse_eeg_packet(&pkt).is_none());
    }

    #[test]
    fn parse_rejects_short_packet() {
        assert!(parse_eeg_packet(&[0xAA]).is_none());
        assert!(parse_eeg_packet(&[0xAA; 62]).is_none());
    }

    #[test]
    fn parse_rejects_invalid_checksum() {
        let mut pkt = make_packet(EEG_EVENT_ID, 0);
        pkt[61] = 0; // zero out checksum
        pkt[62] = 0;
        assert!(parse_eeg_packet(&pkt).is_none());
    }

    #[test]
    fn parse_channel_values_scaled() {
        // Each channel raw value = 1000.0
        // After scaling: 1000.0 * 0.023842 = 23.842
        let raw = [1000.0_f32; 12];
        let pkt = make_packet_with_channels(0, &raw);
        let eeg = parse_eeg_packet(&pkt).unwrap();
        for &ch in &eeg.channels {
            assert!((ch - 23.842).abs() < 0.01, "Expected ~23.842, got {ch}");
        }
    }

    #[test]
    fn parse_negative_channel_values() {
        let raw = [-5000.0_f32; 12];
        let pkt = make_packet_with_channels(0, &raw);
        let eeg = parse_eeg_packet(&pkt).unwrap();
        for &ch in &eeg.channels {
            let expected = -5000.0 * EEG_SCALING_FACTOR;
            assert!((ch - expected).abs() < 0.1, "Expected ~{expected}, got {ch}");
        }
    }

    #[test]
    fn parse_ref_and_drl() {
        let raw = [0.0_f32; 12];
        let pkt = make_packet_with_channels(0, &raw);
        let eeg = parse_eeg_packet(&pkt).unwrap();
        assert!((eeg.ref_value - 42.0).abs() < 0.001);
        assert!((eeg.drl - (-7.5)).abs() < 0.001);
    }

    #[test]
    fn parse_feature_status() {
        let raw = [0.0_f32; 12];
        let pkt = make_packet_with_channels(0, &raw);
        let eeg = parse_eeg_packet(&pkt).unwrap();
        assert_eq!(eeg.feature_status, 0x01);
    }

    #[test]
    fn parse_all_counter_values() {
        for c in 0..=255u8 {
            let pkt = make_packet(EEG_EVENT_ID, c);
            let eeg = parse_eeg_packet(&pkt).unwrap();
            assert_eq!(eeg.counter, c);
        }
    }

    // ── PacketProcessor tests ────────────────────────────────────────────────

    #[test]
    fn processor_basic_single_packet() {
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(EEG_EVENT_ID, 1);
        let events = proc.process_data(&pkt);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Mw75Event::Eeg(e) if e.counter == 1));
        assert_eq!(proc.stats.valid_packets, 1);
        assert_eq!(proc.stats.total_packets, 1);
        assert_eq!(proc.stats.invalid_packets, 0);
    }

    #[test]
    fn processor_multiple_packets_in_one_call() {
        let mut proc = PacketProcessor::new(false);
        let mut data = Vec::new();
        for i in 0..5 {
            data.extend_from_slice(&make_packet(EEG_EVENT_ID, i));
        }
        let events = proc.process_data(&data);
        assert_eq!(events.len(), 5);
        assert_eq!(proc.stats.valid_packets, 5);
    }

    #[test]
    fn processor_split_delivery_across_two_calls() {
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(EEG_EVENT_ID, 1);

        // Deliver first 30 bytes
        let events1 = proc.process_data(&pkt[..30]);
        assert!(events1.is_empty());
        assert_eq!(proc.buffered_len(), 30);

        // Deliver remaining 33 bytes
        let events2 = proc.process_data(&pkt[30..]);
        assert_eq!(events2.len(), 1);
        assert_eq!(proc.buffered_len(), 0);
    }

    #[test]
    fn processor_split_at_every_byte() {
        // Extreme case: deliver one byte at a time
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(EEG_EVENT_ID, 99);

        let mut total_events = 0;
        for &byte in &pkt {
            let events = proc.process_data(&[byte]);
            total_events += events.len();
        }
        assert_eq!(total_events, 1);
    }

    #[test]
    fn processor_garbage_prefix_skipped() {
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(EEG_EVENT_ID, 5);

        // Prepend garbage bytes (non-0xAA)
        let mut data = vec![0x01, 0x02, 0x03, 0x04, 0x05];
        data.extend_from_slice(&pkt);

        let events = proc.process_data(&data);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Mw75Event::Eeg(e) if e.counter == 5));
    }

    #[test]
    fn processor_garbage_between_packets() {
        let mut proc = PacketProcessor::new(false);
        let mut data = Vec::new();
        data.extend_from_slice(&make_packet(EEG_EVENT_ID, 1));
        data.extend_from_slice(&[0x01, 0x02, 0x03]); // garbage
        data.extend_from_slice(&make_packet(EEG_EVENT_ID, 2));

        let events = proc.process_data(&data);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn processor_other_event_type() {
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(100, 7); // event_id=100, not EEG
        let events = proc.process_data(&pkt);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            Mw75Event::OtherEvent { event_id: 100, counter: 7, .. }
        ));
    }

    #[test]
    fn processor_invalid_checksum_skips_and_counts() {
        let mut proc = PacketProcessor::new(false);
        let mut pkt = make_packet(EEG_EVENT_ID, 1);
        pkt[30] = 0xFF; // corrupt data, checksum mismatch

        let events = proc.process_data(&pkt);
        assert!(events.is_empty());
        assert!(proc.stats.invalid_packets > 0);
    }

    #[test]
    fn processor_invalid_then_valid() {
        let mut proc = PacketProcessor::new(false);

        // Invalid packet (corrupted data)
        let mut bad = make_packet(EEG_EVENT_ID, 1);
        bad[30] = 0xFF;

        // Valid packet
        let good = make_packet(EEG_EVENT_ID, 2);

        let mut data = Vec::new();
        data.extend_from_slice(&bad);
        data.extend_from_slice(&good);

        let events = proc.process_data(&data);
        // The good packet should still be found
        assert!(!events.is_empty());
        assert!(proc.stats.valid_packets >= 1);
    }

    #[test]
    fn processor_sync_byte_in_payload() {
        // Build a packet where some data bytes happen to be 0xAA
        let mut pkt = vec![0u8; PACKET_SIZE];
        pkt[0] = SYNC_BYTE;
        pkt[1] = EEG_EVENT_ID;
        pkt[2] = 58;
        pkt[3] = 10;
        // Put 0xAA in several data positions
        pkt[15] = 0xAA;
        pkt[20] = 0xAA;
        pkt[40] = 0xAA;
        fix_checksum(&mut pkt);

        let mut proc = PacketProcessor::new(false);
        let events = proc.process_data(&pkt);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], Mw75Event::Eeg(e) if e.counter == 10));
    }

    #[test]
    fn processor_reset_clears_state() {
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(EEG_EVENT_ID, 1);
        proc.process_data(&pkt);
        assert_eq!(proc.stats.valid_packets, 1);

        proc.reset();
        assert_eq!(proc.stats.valid_packets, 0);
        assert_eq!(proc.stats.total_packets, 0);
        assert_eq!(proc.buffered_len(), 0);
    }

    #[test]
    fn processor_partial_packet_retained() {
        let mut proc = PacketProcessor::new(false);
        let pkt = make_packet(EEG_EVENT_ID, 1);

        // Feed only first 40 bytes — no packet emitted, buffer retains them
        let events = proc.process_data(&pkt[..40]);
        assert!(events.is_empty());
        assert_eq!(proc.buffered_len(), 40);

        // Feed the rest — packet emitted, buffer empty
        let events = proc.process_data(&pkt[40..]);
        assert_eq!(events.len(), 1);
        assert_eq!(proc.buffered_len(), 0);
    }

    #[test]
    fn processor_buffer_overflow_protection() {
        let mut proc = PacketProcessor::new(false);
        // Feed a large amount of garbage with no sync bytes
        let garbage = vec![0x01; PACKET_SIZE * 15];
        let events = proc.process_data(&garbage);
        assert!(events.is_empty());
        // Buffer should have been truncated/cleared
        assert!(proc.buffered_len() < PACKET_SIZE * 11);
    }

    #[test]
    fn processor_stats_error_rate() {
        let mut proc = PacketProcessor::new(false);

        // 3 valid packets
        for i in 0..3 {
            let pkt = make_packet(EEG_EVENT_ID, i);
            proc.process_data(&pkt);
        }

        assert_eq!(proc.stats.valid_packets, 3);
        assert_eq!(proc.stats.total_packets, 3);
        assert!((proc.stats.error_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn processor_two_packets_in_64_byte_chunk() {
        // RFCOMM often delivers 64-byte chunks; this tests the boundary
        // where a 63-byte packet fits in the first chunk with 1 byte spillover
        let mut proc = PacketProcessor::new(false);

        let pkt1 = make_packet(EEG_EVENT_ID, 1);
        let pkt2 = make_packet(EEG_EVENT_ID, 2);

        // Simulate 64-byte RFCOMM chunks
        let mut combined = Vec::new();
        combined.extend_from_slice(&pkt1);
        combined.extend_from_slice(&pkt2);

        // Deliver as 64+62 byte chunks
        let events1 = proc.process_data(&combined[..64]);
        // First packet parsed, 1 byte of second packet buffered
        assert_eq!(events1.len(), 1);

        let events2 = proc.process_data(&combined[64..]);
        assert_eq!(events2.len(), 1);
    }

    #[test]
    fn processor_empty_input() {
        let mut proc = PacketProcessor::new(false);
        let events = proc.process_data(&[]);
        assert!(events.is_empty());
        assert_eq!(proc.buffered_len(), 0);
    }

    #[test]
    fn processor_verbose_mode() {
        let mut proc = PacketProcessor::new(true);
        assert!(proc.verbose);
        // Should not panic even with verbose logging on invalid packets
        let mut bad = make_packet(EEG_EVENT_ID, 0);
        bad[50] = 0xFF;
        proc.process_data(&bad);
        assert!(proc.stats.invalid_packets > 0);
    }

    // ── ChecksumStats tests ──────────────────────────────────────────────────

    #[test]
    fn stats_default() {
        let stats = ChecksumStats::default();
        assert_eq!(stats.valid_packets, 0);
        assert_eq!(stats.invalid_packets, 0);
        assert_eq!(stats.total_packets, 0);
        assert_eq!(stats.error_rate(), 0.0);
    }

    #[test]
    fn stats_error_rate_calculation() {
        let stats = ChecksumStats {
            valid_packets: 90,
            invalid_packets: 10,
            total_packets: 100,
        };
        assert!((stats.error_rate() - 10.0).abs() < 0.01);
    }

    #[test]
    fn stats_error_rate_zero_packets() {
        let stats = ChecksumStats::default();
        assert_eq!(stats.error_rate(), 0.0); // No division by zero
    }

    #[test]
    fn stats_error_rate_all_invalid() {
        let stats = ChecksumStats {
            valid_packets: 0,
            invalid_packets: 50,
            total_packets: 50,
        };
        assert!((stats.error_rate() - 100.0).abs() < 0.01);
    }
}
