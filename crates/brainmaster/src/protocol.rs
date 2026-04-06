//! BrainMaster binary serial protocol.
//!
//! ## Packet format
//!
//! All BrainMaster amplifiers use a common binary packet format over serial:
//!
//! ```text
//! [SYNC_HI] [SYNC_LO] [COUNTER] [CH0_HI CH0_LO] [CH1_HI CH1_LO] … [CHECKSUM]
//! ```
//!
//! - **Sync**: `0xAA 0x55` (2 bytes)
//! - **Counter**: packet sequence number (1 byte, wraps at 255)
//! - **Channels**: 16-bit signed big-endian integers, count depends on model
//! - **Checksum**: XOR of all bytes from counter through last channel byte
//!
//! ## ADC scaling
//!
//! The 16-bit ADC values are converted to microvolts using the model-specific
//! scale factor. Atlantis uses a 16-bit ADC with ±400 µV range; Discovery/Freedom
//! use 24-bit ADCs (top 16 bits transmitted) with ±3200 µV range.

/// Sync byte pair that marks the start of every packet.
pub const SYNC: [u8; 2] = [0xAA, 0x55];

/// Maximum number of EEG channels in any BrainMaster model.
pub const MAX_CHANNELS: usize = 24;

/// Parse a complete packet from `buf` (must start after sync bytes).
/// Returns `(counter, channels_i16, checksum_ok)`.
pub fn parse_packet(buf: &[u8], n_channels: usize) -> Option<(u8, Vec<i16>, bool)> {
    // buf layout: [counter] [ch0_hi ch0_lo] [ch1_hi ch1_lo] ... [checksum]
    let expected_len = 1 + n_channels * 2 + 1;
    if buf.len() < expected_len {
        return None;
    }

    let counter = buf[0];
    let mut channels = Vec::with_capacity(n_channels);
    for i in 0..n_channels {
        let hi = buf[1 + i * 2] as i16;
        let lo = buf[2 + i * 2] as i16;
        channels.push((hi << 8) | (lo & 0xFF));
    }

    let checksum_pos = 1 + n_channels * 2;
    let expected_checksum = buf[..checksum_pos].iter().fold(0u8, |acc, &b| acc ^ b);
    let got_checksum = buf[checksum_pos];

    Some((counter, channels, expected_checksum == got_checksum))
}

/// Scale raw 16-bit ADC value to microvolts.
///
/// - Atlantis: ±400 µV full scale over 16 bits → 0.01221 µV/LSB
/// - Discovery/Freedom: ±3200 µV over 16 bits (of 24-bit ADC) → 0.09766 µV/LSB
pub fn adc_to_uv(raw: i16, scale_uv_per_lsb: f64) -> f64 {
    raw as f64 * scale_uv_per_lsb
}

/// Atlantis 2/4ch: 16-bit ADC, ±400 µV range.
pub const ATLANTIS_UV_PER_LSB: f64 = 400.0 / 32768.0; // ~0.01221

/// Discovery/Freedom: 24-bit ADC (top 16 bits), ±3200 µV range.
pub const DISCOVERY_UV_PER_LSB: f64 = 3200.0 / 32768.0; // ~0.09766

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_2ch_packet() {
        // counter=1, ch0=0x0100 (256), ch1=0xFF00 (-256), checksum
        let counter: u8 = 1;
        let ch0: [u8; 2] = [0x01, 0x00];
        let ch1: [u8; 2] = [0xFF, 0x00];
        let payload = [counter, ch0[0], ch0[1], ch1[0], ch1[1]];
        let checksum = payload.iter().fold(0u8, |acc, &b| acc ^ b);
        let mut buf = payload.to_vec();
        buf.push(checksum);

        let (c, channels, ok) = parse_packet(&buf, 2).unwrap();
        assert_eq!(c, 1);
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0], 256);
        assert_eq!(channels[1], -256);
        assert!(ok);
    }

    #[test]
    fn adc_scaling() {
        let uv = adc_to_uv(32767, ATLANTIS_UV_PER_LSB);
        assert!((uv - 400.0).abs() < 0.02);

        let uv = adc_to_uv(32767, DISCOVERY_UV_PER_LSB);
        assert!((uv - 3200.0).abs() < 0.1);
    }
}
