//! Low-level packet decoding for all OpenBCI wire formats.
//!
//! ## Cyton packet format (33 bytes)
//!
//! ```text
//! Byte  0    : 0xA0  (start byte)
//! Byte  1    : sample number (rolling 0–255)
//! Bytes 2–25 : 8 EEG channels × 3 bytes each (24-bit big-endian signed)
//! Bytes 26–31: auxiliary bytes (accelerometer or analog, depending on end byte)
//! Byte  32   : end byte (0xC0–0xC6)
//! ```
//!
//! EEG µV scale: `raw × (4.5 / 8_388_607 / gain × 1_000_000)`
//!
//! ## Cyton+Daisy packet merging
//!
//! The Daisy firmware interleaves packets: **even** `sample_num` packets
//! carry Daisy channels 8–15 and must be buffered; the following **odd**
//! packet carries Cyton channels 0–7 and triggers emission of a merged
//! 16-channel [`Sample`].
//!
//! ## Ganglion BLE packet format (20 bytes)
//!
//! The Ganglion uses a compressed EEG encoding to fit two samples into one
//! 20-byte BLE notification.  Each call to [`decode_ganglion`] returns 0, 1,
//! or 2 [`Sample`] values.
//!
//! | Packet ID range | Encoding | Accel byte |
//! |---|---|---|
//! | 0–99  (fw3) / 1–100 (fw2) | 18-bit MSB | byte 19 |
//! | 100–199 (fw3) / 101–200 (fw2) | 19-bit MSB | none |
//! | 0 (fw2 only) | uncompressed 24-bit init | none |
//! | 201–205 | ASCII impedance value | — |

use crate::channel_config::GainTracker;
use crate::sample::{now, Sample};

// ─── Constants ───────────────────────────────────────────────────────────────

pub const START_BYTE:        u8 = 0xA0;
pub const END_BYTE_STANDARD: u8 = 0xC0; // aux = accelerometer
pub const END_BYTE_ANALOG:   u8 = 0xC1; // aux = analog pins
pub const END_BYTE_MAX:      u8 = 0xC6;

/// Vref / max-code / reference-factor * 1e6 for µV output.
/// = 4.5 V / 8_388_607 / 1.5 / gain_factor × 1_000_000 µV  (ADS1299 formula)
const EEG_REF: f64 = 4.5 / 8_388_607.0 * 1_000_000.0;

/// Ganglion ADS1299 reference:
/// = 1.2 V / 8_388_607 / 1.5 / 51 × 1_000_000 µV
pub const GANGLION_EEG_SCALE: f64 = 1.2 * 1_000_000.0 / (8_388_607.0 * 1.5 * 51.0);

/// Cyton on-board accelerometer: 0.002 / 2^4 g per LSB
const ACCEL_SCALE: f64 = 0.002 / 16.0;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Decode a big-endian 24-bit signed integer from 3 bytes.
pub fn cast_24bit_to_i32(b: &[u8]) -> i32 {
    let v = ((b[0] as i32) << 16) | ((b[1] as i32) << 8) | (b[2] as i32);
    // sign-extend from 24 bits
    if v & 0x0080_0000 != 0 { v | !0x00FF_FFFF } else { v }
}

/// Decode a big-endian 16-bit signed integer from 2 bytes.
pub fn cast_16bit_to_i32(b: &[u8]) -> i32 {
    let v = ((b[0] as i32) << 8) | (b[1] as i32);
    if v & 0x8000 != 0 { v | !0xFFFF } else { v }
}

/// Expand byte `b` into 8 individual bit values written into `out`, MSB first.
///
/// Used by the Ganglion bit-packed decoder to unpack compressed EEG deltas.
fn byte_to_bits(b: u8, out: &mut [u8]) {
    for i in 0..8 {
        out[i] = (b >> (7 - i)) & 1;
    }
}

/// Expand a 20-byte Ganglion BLE payload into an array of 160 individual bits,
/// MSB first.  This is the first step in decoding the compressed 18-bit or
/// 19-bit EEG delta values.
fn bytes_to_bits_160(data: &[u8]) -> [u8; 160] {
    let mut bits = [0u8; 160];
    for (i, &b) in data.iter().enumerate().take(20) {
        byte_to_bits(b, &mut bits[i * 8..i * 8 + 8]);
    }
    bits
}

/// Read `n` bits from `bits[offset..]`, interpret them as a signed integer
/// (MSB first), and sign-extend to `i32`.
///
/// Used to recover signed EEG delta values from the Ganglion's bit-packed
/// 18-bit or 19-bit compressed format.
fn bits_to_i32(bits: &[u8], offset: usize, n: usize) -> i32 {
    let mut result: i32 = 0;
    for i in 0..n {
        result = (result << 1) | (bits[offset + i] as i32);
    }
    if n < 32 && bits[offset] == 1 {
        result |= !((1i32 << n) - 1);
    }
    result
}

// ─── Cyton ───────────────────────────────────────────────────────────────────

/// Decode one 33-byte Cyton packet.
///
/// `body` is the 32 bytes that follow the start byte (0xA0).
/// Returns `None` if the end byte is out of range or the packet is malformed.
pub fn decode_cyton(body: &[u8; 32], gains: &GainTracker, num_ch: usize) -> Option<Sample> {
    let end_byte = body[31];
    if !(END_BYTE_STANDARD..=END_BYTE_MAX).contains(&end_byte) {
        return None;
    }

    let mut sample = Sample::zeroed(num_ch);
    sample.sample_num = body[0];
    sample.end_byte   = end_byte;
    sample.timestamp  = now();
    sample.aux_bytes  = body[25..31].try_into().ok()?;

    // EEG channels: bytes 1–24 (8 channels × 3 bytes, big-endian 24-bit signed)
    for i in 0..num_ch.min(8) {
        let raw = cast_24bit_to_i32(&body[1 + i * 3..]);
        let scale = EEG_REF / gains.gain_for(i);
        sample.eeg[i] = scale * raw as f64;
    }

    // Aux bytes interpretation
    let aux = &body[25..31];
    if end_byte == END_BYTE_STANDARD {
        // 3 × 16-bit accelerometer (big-endian signed)
        let ax = cast_16bit_to_i32(&aux[0..2]);
        let ay = cast_16bit_to_i32(&aux[2..4]);
        let az = cast_16bit_to_i32(&aux[4..6]);
        if ax != 0 {
            sample.accel = Some([
                ACCEL_SCALE * ax as f64,
                ACCEL_SCALE * ay as f64,
                ACCEL_SCALE * az as f64,
            ]);
        }
    } else if end_byte == END_BYTE_ANALOG {
        sample.analog = Some([
            cast_16bit_to_i32(&aux[0..2]) as f64,
            cast_16bit_to_i32(&aux[2..4]) as f64,
            cast_16bit_to_i32(&aux[4..6]) as f64,
        ]);
    }

    Some(sample)
}

/// Decode one 33-byte CytonDaisy packet.
///
/// Alternating packets carry Daisy (even sample_num) and Cyton (odd sample_num)
/// channels.  Call this function with both packets; it returns a combined
/// 16-channel Sample only when the Cyton (odd) packet is received.
///
/// `buffer` is the caller-maintained partial sample being assembled.
pub fn decode_cyton_daisy(
    body: &[u8; 32],
    gains: &GainTracker,
    buffer: &mut Option<Sample>,
) -> Option<Sample> {
    let end_byte = body[31];
    if !(END_BYTE_STANDARD..=END_BYTE_MAX).contains(&end_byte) {
        return None;
    }

    let sample_num = body[0];
    let is_daisy_packet = sample_num % 2 == 0; // even → Daisy (channels 9-16)

    let aux = &body[25..31];
    let accel_from_aux = if end_byte == END_BYTE_STANDARD {
        let ax = cast_16bit_to_i32(&aux[0..2]);
        let ay = cast_16bit_to_i32(&aux[2..4]);
        let az = cast_16bit_to_i32(&aux[4..6]);
        if ax != 0 {
            Some([ACCEL_SCALE * ax as f64, ACCEL_SCALE * ay as f64, ACCEL_SCALE * az as f64])
        } else {
            None
        }
    } else {
        None
    };

    let analog_from_aux = if end_byte == END_BYTE_ANALOG {
        Some([
            cast_16bit_to_i32(&aux[0..2]) as f64,
            cast_16bit_to_i32(&aux[2..4]) as f64,
            cast_16bit_to_i32(&aux[4..6]) as f64,
        ])
    } else {
        None
    };

    if is_daisy_packet {
        // Start buffering a new 16-channel sample: fill channels 8–15 (Daisy)
        let mut s = Sample::zeroed(16);
        s.sample_num = sample_num;
        s.end_byte   = end_byte;
        s.aux_bytes  = aux.try_into().unwrap_or([0u8; 6]);

        for i in 0..8 {
            let raw = cast_24bit_to_i32(&body[1 + i * 3..]);
            let scale = EEG_REF / gains.gain_for(i + 8); // Daisy gains at indices 8-15
            s.eeg[i + 8] = scale * raw as f64;
        }
        s.accel = accel_from_aux;
        s.analog = analog_from_aux;
        *buffer = Some(s);
        None
    } else {
        // Cyton packet: fill channels 0–7, then complete the sample
        if let Some(ref mut s) = buffer {
            for i in 0..8 {
                let raw = cast_24bit_to_i32(&body[1 + i * 3..]);
                let scale = EEG_REF / gains.gain_for(i);
                s.eeg[i] = scale * raw as f64;
            }
            // Average accel / analog over the two packets
            if let Some(new_acc) = accel_from_aux {
                if let Some(old_acc) = s.accel {
                    s.accel = Some([
                        (old_acc[0] + new_acc[0]) * 0.5,
                        (old_acc[1] + new_acc[1]) * 0.5,
                        (old_acc[2] + new_acc[2]) * 0.5,
                    ]);
                } else {
                    s.accel = Some(new_acc);
                }
            }
            if let Some(new_an) = analog_from_aux {
                if let Some(old_an) = s.analog {
                    s.analog = Some([
                        (old_an[0] + new_an[0]) * 0.5,
                        (old_an[1] + new_an[1]) * 0.5,
                        (old_an[2] + new_an[2]) * 0.5,
                    ]);
                } else {
                    s.analog = Some(new_an);
                }
            }
            s.timestamp = now();
            buffer.take()
        } else {
            None
        }
    }
}

// ─── Ganglion ────────────────────────────────────────────────────────────────

/// Mutable state maintained between Ganglion BLE packet decodes.
///
/// Because the Ganglion compresses EEG data as deltas (fw2) or truncated
/// absolute values (fw3), the decoder needs to carry state across packets.
#[derive(Debug, Default)]
pub struct GanglionState {
    /// Eight slots of reconstructed raw EEG values (before µV scaling).
    ///
    /// Slots 0–3 hold the older sample; slots 4–7 hold the newer sample within
    /// each pair.  Firmware-2 decoding updates these as rolling accumulators.
    pub last_data: [f64; 8],
    /// Most recently decoded accelerometer reading in **g** (X, Y, Z).
    ///
    /// Updated one axis at a time from byte 19 of 18-bit EEG packets.
    pub accel: [f64; 3],
}

const GANGLION_ACCEL_SCALE: f64 = 0.016; // g per LSB (8-bit signed)

/// Decode one 20-byte Ganglion BLE notification packet.
///
/// Returns 0, 1, or 2 samples depending on the packet type.
///
/// The Ganglion firmware encodes EEG data in a compressed 20-byte format:
///
/// - Packet IDs 0–99 (fw3) / 1–100 (fw2): 18-bit compressed, + 1 accel byte
/// - Packet IDs 100–199 (fw3) / 101–200 (fw2): 19-bit compressed, no accel
/// - Packet ID 0 (fw2 only): uncompressed raw 24-bit values (initialization)
/// - Packet IDs 201–205: ASCII impedance values
pub fn decode_ganglion(
    data: &[u8],
    state: &mut GanglionState,
    firmware: u8, // 2 or 3
) -> Vec<Sample> {
    if data.len() < 2 {
        return vec![];
    }

    let id = data[0];

    // ── Impedance packets (fw3: 201-205, fw2: matches data[0]>200 && <206) ──
    if id > 200 && id < 206 {
        let mut s = Sample::zeroed(4);
        s.sample_num = id;
        s.timestamp = now();
        // Parse ASCII value up to 'Z' terminator
        let ascii_val: String = data[1..].iter()
            .take_while(|&&b| b != b'Z')
            .map(|&b| b as char)
            .collect();
        if let Ok(val) = ascii_val.trim().parse::<f64>() {
            let mut resist = vec![0.0f64; 5]; // [ch1, ch2, ch3, ch4, ref]
            let slot = (id % 10) as usize;
            if slot > 0 && slot <= 5 {
                resist[slot - 1] = val;
            }
            s.resistance = Some(resist);
        }
        return vec![s];
    }

    // ── Normal EEG packets ────────────────────────────────────────────────────
    if id > 200 {
        return vec![]; // unknown packet type
    }

    let bits = bytes_to_bits_160(data);

    if firmware == 2 {
        decode_ganglion_fw2(data, &bits, id, state)
    } else {
        decode_ganglion_fw3(data, &bits, id, state)
    }
}

/// Decode a firmware-3 Ganglion EEG packet.
///
/// Firmware 3 encodes EEG as absolute 18-bit (IDs 0–99) or 19-bit (IDs 100–199)
/// values, not deltas.  One packet yields two 4-channel samples.  Byte 19 of
/// 18-bit packets encodes one axis of the accelerometer.
fn decode_ganglion_fw3(
    data: &[u8],
    bits: &[u8; 160],
    id: u8,
    state: &mut GanglionState,
) -> Vec<Sample> {
    let bits_per_num: usize;

    if id < 100 {
        // 18-bit: accel axis encoded in byte 19 based on packet sub-id
        match id % 10 {
            0 => state.accel[2] = -GANGLION_ACCEL_SCALE * (data[19] as i8) as f64,
            1 => state.accel[1] =  GANGLION_ACCEL_SCALE * (data[19] as i8) as f64,
            2 => state.accel[0] =  GANGLION_ACCEL_SCALE * (data[19] as i8) as f64,
            _ => {}
        }
        bits_per_num = 18;
    } else {
        bits_per_num = 19;
    }

    // Extract 8 values from bits[8..], each `bits_per_num` bits wide
    let mut values = [0f64; 8];
    let shift = 24 - bits_per_num; // left-shift to restore 24-bit magnitude
    for (counter, bit_offset) in (8..bits_per_num * 8).step_by(bits_per_num).enumerate().take(8) {
        let raw = bits_to_i32(bits, bit_offset, bits_per_num);
        values[counter] = (raw << shift) as f64;
    }

    // First sample (channels 0-3)
    let mut s1 = Sample::zeroed(4);
    s1.sample_num = id;
    s1.timestamp = now();
    s1.accel = Some(state.accel);
    for i in 0..4 {
        s1.eeg[i] = GANGLION_EEG_SCALE * values[i];
        state.last_data[i] = values[i];
    }

    // Second sample (channels 0-3, second measurement in packet)
    let mut s2 = Sample::zeroed(4);
    s2.sample_num = id;
    s2.timestamp = now();
    s2.accel = Some(state.accel);
    for i in 0..4 {
        s2.eeg[i] = GANGLION_EEG_SCALE * values[i + 4];
        state.last_data[i + 4] = values[i + 4];
    }

    vec![s1, s2]
}

/// Decode a firmware-2 Ganglion EEG packet.
///
/// Firmware 2 encodes EEG as **delta** values relative to the previous packet.
/// Packet ID 0 is a special initialisation packet containing four uncompressed
/// 24-bit values.  IDs 1–100 carry 18-bit deltas (+ accelerometer byte);
/// IDs 101–200 carry 19-bit deltas.  One packet yields two 4-channel samples.
fn decode_ganglion_fw2(
    data: &[u8],
    bits: &[u8; 160],
    id: u8,
    state: &mut GanglionState,
) -> Vec<Sample> {
    // Packet ID 0 = uncompressed initialization packet
    if id == 0 {
        // Shift previous last_data[4..7] → last_data[0..3]
        state.last_data[0] = state.last_data[4];
        state.last_data[1] = state.last_data[5];
        state.last_data[2] = state.last_data[6];
        state.last_data[3] = state.last_data[7];

        // New uncompressed values at data[1..12] (4 × 24-bit)
        for i in 0..4 {
            state.last_data[4 + i] = cast_24bit_to_i32(&data[1 + i * 3..]) as f64;
        }

        let mut s = Sample::zeroed(4);
        s.sample_num = 0;
        s.timestamp = now();
        s.accel = Some(state.accel);
        for i in 0..4 {
            s.eeg[i] = GANGLION_EEG_SCALE * state.last_data[4 + i];
        }
        return vec![s];
    }

    let bits_per_num: usize;
    if id >= 1 && id <= 100 {
        match id % 10 {
            0 => state.accel[2] = -GANGLION_ACCEL_SCALE * (data[19] as i8) as f64,
            1 => state.accel[1] =  GANGLION_ACCEL_SCALE * (data[19] as i8) as f64,
            2 => state.accel[0] =  GANGLION_ACCEL_SCALE * (data[19] as i8) as f64,
            _ => {}
        }
        bits_per_num = 18;
    } else {
        bits_per_num = 19;
    }

    // Extract 8 deltas
    let mut delta = [0f64; 8];
    for (counter, bit_offset) in (8..bits_per_num * 8).step_by(bits_per_num).enumerate().take(8) {
        delta[counter] = bits_to_i32(bits, bit_offset, bits_per_num) as f64;
    }

    // Apply deltas: last_data[0..3] = last_data[4..7] - delta[0..3]
    for i in 0..4 {
        state.last_data[i] = state.last_data[i + 4] - delta[i];
    }
    // last_data[4..7] = last_data[0..3] - delta[4..7]
    for i in 0..4 {
        state.last_data[i + 4] = state.last_data[i] - delta[i + 4];
    }

    let mut s1 = Sample::zeroed(4);
    s1.sample_num = id;
    s1.timestamp = now();
    s1.accel = Some(state.accel);
    for i in 0..4 {
        s1.eeg[i] = GANGLION_EEG_SCALE * state.last_data[i];
    }

    let mut s2 = Sample::zeroed(4);
    s2.sample_num = id;
    s2.timestamp = now();
    s2.accel = Some(state.accel);
    for i in 0..4 {
        s2.eeg[i] = GANGLION_EEG_SCALE * state.last_data[i + 4];
    }

    vec![s1, s2]
}
