//! All event and data types produced by the MW75 client.
//!
//! These types form the public API surface for consumers of the library.
//! All sensor data flows through [`Mw75Event`] variants, which are received
//! via the `tokio::sync::mpsc::Receiver` returned by
//! [`crate::mw75_client::Mw75Client::connect`].

/// A parsed EEG data packet from the MW75 device.
///
/// Each packet carries one sample across all 12 EEG channels plus reference
/// and DRL (Driven Right Leg) values.  The MW75 streams at 500 Hz, so one
/// packet arrives approximately every 2 ms.
///
/// # Wire format
///
/// ```text
/// byte[0]       : 0xAA sync byte
/// byte[1]       : event ID (239 = EEG)
/// byte[2]       : data length
/// byte[3]       : counter (0–255, wrapping)
/// bytes[4..8]   : REF value (f32 LE)
/// bytes[8..12]  : DRL value (f32 LE)
/// bytes[12..60] : 12 × f32 LE channel values
/// byte[60]      : feature status
/// bytes[61..63] : checksum (u16 LE, sum of bytes 0..61 & 0xFFFF)
/// ```
///
/// # Channel scaling
///
/// Raw ADC float values are multiplied by the EEG scaling factor
/// ([`crate::protocol::EEG_SCALING_FACTOR`] = `0.023842`) to convert
/// to microvolts.  A raw value of 1000.0 becomes ≈ 23.84 µV.
///
/// # Example
///
/// ```
/// # use mw75::types::EegPacket;
/// let packet = EegPacket {
///     timestamp: 1710000000.0,
///     event_id: 239,
///     counter: 42,
///     ref_value: 5.0,
///     drl: -3.0,
///     channels: vec![10.0; 12],
///     feature_status: 0,
///     checksum_valid: true,
/// };
/// assert_eq!(packet.channels.len(), 12);
/// ```
#[derive(Debug, Clone)]
pub struct EegPacket {
    /// Wall-clock timestamp in seconds since Unix epoch, captured when the
    /// packet was received from the transport.
    pub timestamp: f64,
    /// Event ID byte from the packet header. Always `239` for EEG data.
    pub event_id: u8,
    /// Monotonically increasing packet counter (wraps at 255).
    /// Used for dropped-packet detection: if `counter` jumps by more than 1,
    /// `(counter - last_counter - 1)` packets were lost.
    pub counter: u8,
    /// Reference electrode value (f32, already in correct units).
    pub ref_value: f32,
    /// DRL (Driven Right Leg) electrode value (f32, already in correct units).
    pub drl: f32,
    /// 12 EEG channel values in µV.
    ///
    /// Raw ADC float values are multiplied by the scaling factor
    /// (`0.023842`) to convert to microvolts.
    pub channels: Vec<f32>,
    /// Feature status byte from the packet.
    pub feature_status: u8,
    /// Whether the packet checksum validated correctly.
    /// Always `true` for packets returned by [`crate::parse::parse_eeg_packet`]
    /// (invalid packets are rejected before constructing this struct).
    pub checksum_valid: bool,
}

/// Battery level information received via BLE status notifications.
///
/// Sent by the MW75 in response to the battery query command during
/// the BLE activation sequence.
///
/// # Example
///
/// ```
/// # use mw75::types::BatteryInfo;
/// let bat = BatteryInfo { level: 85 };
/// assert_eq!(bat.level, 85);
/// ```
#[derive(Debug, Clone)]
pub struct BatteryInfo {
    /// Battery state-of-charge in percent (0–100).
    pub level: u8,
}

/// Tracks packet validation statistics.
///
/// Mirrors the Python `ChecksumStats` dataclass.  Updated by
/// [`crate::parse::PacketProcessor`] as packets are processed.
///
/// # Example
///
/// ```
/// # use mw75::types::ChecksumStats;
/// let stats = ChecksumStats {
///     valid_packets: 990,
///     invalid_packets: 10,
///     total_packets: 1000,
/// };
/// assert!((stats.error_rate() - 1.0).abs() < 0.01);
/// ```
#[derive(Debug, Clone, Default)]
pub struct ChecksumStats {
    /// Number of packets that passed checksum validation.
    pub valid_packets: u64,
    /// Number of packets that failed checksum validation.
    pub invalid_packets: u64,
    /// Total number of packets seen (valid + invalid).
    pub total_packets: u64,
}

impl ChecksumStats {
    /// Calculate the checksum error rate as a percentage (0.0–100.0).
    ///
    /// Returns `0.0` when `total_packets` is zero (no division by zero).
    pub fn error_rate(&self) -> f64 {
        if self.total_packets == 0 {
            0.0
        } else {
            (self.invalid_packets as f64 / self.total_packets as f64) * 100.0
        }
    }
}

/// BLE activation status reported during the connection handshake.
///
/// The MW75 responds to the activation command sequence with status
/// notifications confirming whether EEG mode and raw mode were
/// successfully enabled.
///
/// # Example
///
/// ```
/// # use mw75::types::ActivationStatus;
/// let status = ActivationStatus {
///     eeg_enabled: true,
///     raw_mode_enabled: true,
/// };
/// assert!(status.eeg_enabled && status.raw_mode_enabled);
/// ```
#[derive(Debug, Clone)]
pub struct ActivationStatus {
    /// Whether EEG mode was confirmed enabled by the device.
    pub eeg_enabled: bool,
    /// Whether raw mode was confirmed enabled by the device.
    pub raw_mode_enabled: bool,
}

/// All data events emitted by [`crate::mw75_client::Mw75Client`].
///
/// Consumers receive these values through the `mpsc::Receiver` returned by
/// [`crate::mw75_client::Mw75Client::connect`] or
/// [`crate::mw75_client::Mw75Client::connect_to`].
///
/// # Event flow
///
/// A typical session produces events in this order:
///
/// 1. [`Connected`](Mw75Event::Connected) — BLE link established
/// 2. [`Battery`](Mw75Event::Battery) — battery level from activation
/// 3. [`Activated`](Mw75Event::Activated) — activation handshake complete
/// 4. [`Eeg`](Mw75Event::Eeg) — 500 Hz EEG data stream (from RFCOMM)
/// 5. [`Disconnected`](Mw75Event::Disconnected) — link lost
///
/// # Example
///
/// ```
/// # use mw75::types::{Mw75Event, EegPacket};
/// let event = Mw75Event::Eeg(EegPacket {
///     timestamp: 0.0,
///     event_id: 239,
///     counter: 0,
///     ref_value: 0.0,
///     drl: 0.0,
///     channels: vec![0.0; 12],
///     feature_status: 0,
///     checksum_valid: true,
/// });
/// assert!(matches!(event, Mw75Event::Eeg(_)));
/// ```
#[derive(Debug, Clone)]
pub enum Mw75Event {
    /// An EEG data packet with 12-channel sample data at 500 Hz.
    ///
    /// Produced by [`crate::parse::PacketProcessor`] when a valid packet
    /// with event ID 239 is decoded.
    Eeg(EegPacket),

    /// Battery level update received during BLE activation.
    Battery(BatteryInfo),

    /// BLE activation handshake completed successfully.
    /// The inner [`ActivationStatus`] confirms which modes were enabled.
    Activated(ActivationStatus),

    /// The BLE link has been established and GATT services discovered.
    /// The inner `String` is the advertised device name (e.g. `"MW75 Neuro"`).
    Connected(String),

    /// The BLE link was lost (device turned off, out of range, etc.).
    ///
    /// After receiving this event the channel will be closed; no further
    /// events will arrive.
    Disconnected,

    /// Raw data received from the transport (RFCOMM or other).
    ///
    /// Emitted before packet parsing. Useful for debugging or implementing
    /// custom decoders.  Not emitted by the default pipeline unless
    /// explicitly forwarded.
    RawData(Vec<u8>),

    /// A non-EEG event packet (event ID ≠ 239).
    ///
    /// Carries the full 63-byte raw packet for inspection or logging.
    OtherEvent {
        /// Event ID byte from the packet header.
        event_id: u8,
        /// Packet counter byte.
        counter: u8,
        /// Full raw packet bytes.
        raw: Vec<u8>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eeg_packet_construction() {
        let pkt = EegPacket {
            timestamp: 1700000000.0,
            event_id: 239,
            counter: 100,
            ref_value: 1.5,
            drl: -2.3,
            channels: vec![10.0; 12],
            feature_status: 0,
            checksum_valid: true,
        };
        assert_eq!(pkt.channels.len(), 12);
        assert_eq!(pkt.event_id, 239);
        assert!(pkt.checksum_valid);
    }

    #[test]
    fn battery_info() {
        let b = BatteryInfo { level: 100 };
        assert_eq!(b.level, 100);
    }

    #[test]
    fn checksum_stats_default() {
        let s = ChecksumStats::default();
        assert_eq!(s.valid_packets, 0);
        assert_eq!(s.invalid_packets, 0);
        assert_eq!(s.total_packets, 0);
        assert_eq!(s.error_rate(), 0.0);
    }

    #[test]
    fn checksum_stats_error_rate() {
        let s = ChecksumStats {
            valid_packets: 80,
            invalid_packets: 20,
            total_packets: 100,
        };
        assert!((s.error_rate() - 20.0).abs() < 0.01);
    }

    #[test]
    fn checksum_stats_all_valid() {
        let s = ChecksumStats {
            valid_packets: 1000,
            invalid_packets: 0,
            total_packets: 1000,
        };
        assert_eq!(s.error_rate(), 0.0);
    }

    #[test]
    fn activation_status() {
        let a = ActivationStatus {
            eeg_enabled: true,
            raw_mode_enabled: false,
        };
        assert!(a.eeg_enabled);
        assert!(!a.raw_mode_enabled);
    }

    #[test]
    fn event_matching() {
        let events: Vec<Mw75Event> = vec![
            Mw75Event::Connected("MW75".into()),
            Mw75Event::Battery(BatteryInfo { level: 50 }),
            Mw75Event::Activated(ActivationStatus {
                eeg_enabled: true,
                raw_mode_enabled: true,
            }),
            Mw75Event::Eeg(EegPacket {
                timestamp: 0.0,
                event_id: 239,
                counter: 0,
                ref_value: 0.0,
                drl: 0.0,
                channels: vec![0.0; 12],
                feature_status: 0,
                checksum_valid: true,
            }),
            Mw75Event::Disconnected,
            Mw75Event::RawData(vec![0xAA]),
            Mw75Event::OtherEvent {
                event_id: 100,
                counter: 5,
                raw: vec![0; 63],
            },
        ];

        assert!(matches!(events[0], Mw75Event::Connected(_)));
        assert!(matches!(events[1], Mw75Event::Battery(_)));
        assert!(matches!(events[2], Mw75Event::Activated(_)));
        assert!(matches!(events[3], Mw75Event::Eeg(_)));
        assert!(matches!(events[4], Mw75Event::Disconnected));
        assert!(matches!(events[5], Mw75Event::RawData(_)));
        assert!(matches!(events[6], Mw75Event::OtherEvent { .. }));
    }

    #[test]
    fn event_clone() {
        let event = Mw75Event::Eeg(EegPacket {
            timestamp: 1.0,
            event_id: 239,
            counter: 0,
            ref_value: 0.0,
            drl: 0.0,
            channels: vec![1.0, 2.0, 3.0],
            feature_status: 0,
            checksum_valid: true,
        });
        let cloned = event.clone();
        if let (Mw75Event::Eeg(a), Mw75Event::Eeg(b)) = (&event, &cloned) {
            assert_eq!(a.channels, b.channels);
        } else {
            panic!("Clone should preserve variant");
        }
    }

    #[test]
    fn event_debug() {
        let event = Mw75Event::Connected("Test".into());
        let debug = format!("{:?}", event);
        assert!(debug.contains("Connected"));
        assert!(debug.contains("Test"));
    }
}
