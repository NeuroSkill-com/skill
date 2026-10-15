//! Per-channel ADS1299 amplifier configuration.
//!
//! The Cyton (and Daisy expansion, Galea) board uses the Texas Instruments
//! [ADS1299](https://www.ti.com/product/ADS1299) 24-bit ADC.  Each of the
//! eight channels on a single ADS1299 can be configured independently for
//! gain, input multiplexer, bias network inclusion, and SRB reference routing.
//!
//! The OpenBCI firmware accepts channel-setting commands of the form:
//! ```text
//! x (CH) (PWR) (GAIN) (INPUT) (BIAS) (SRB2) (SRB1) X
//! ```
//! Use [`ChannelConfig::to_command`] to generate these strings, and
//! [`GainTracker`] to keep the µV scaling factors up-to-date.

/// Per-channel amplifier gain supported by the ADS1299.
///
/// Higher gain increases sensitivity (smaller signals are detectable) but
/// reduces the input range.  For EEG signals (≤ ±200 µV) 24× is typical.
/// For large EMG signals (±1 mV or more) consider 4× or lower.
///
/// | Variant | Multiplier | Max input range (±Vref/gain) |
/// |---------|------------|------------------------------|
/// | `X1`  | 1×  | ±4.5 V |
/// | `X2`  | 2×  | ±2.25 V |
/// | `X4`  | 4×  | ±1.125 V |
/// | `X6`  | 6×  | ±0.75 V |
/// | `X8`  | 8×  | ±0.5625 V |
/// | `X12` | 12× | ±0.375 V |
/// | `X24` | 24× | ±0.1875 V |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gain {
    X1  = 0,
    X2  = 1,
    X4  = 2,
    X6  = 3,
    X8  = 4,
    X12 = 5,
    X24 = 6,
}

impl Gain {
    /// The numeric multiplier for this gain setting.
    pub fn multiplier(self) -> f64 {
        match self {
            Gain::X1  => 1.0,
            Gain::X2  => 2.0,
            Gain::X4  => 4.0,
            Gain::X6  => 6.0,
            Gain::X8  => 8.0,
            Gain::X12 => 12.0,
            Gain::X24 => 24.0,
        }
    }

    /// The ADS1299 register code (0–6) for this gain setting, used in `x...X` commands.
    pub(crate) fn code(self) -> u8 {
        self as u8
    }
}

impl Default for Gain {
    fn default() -> Self {
        Gain::X24
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// ADS1299 channel input multiplexer (MUX) setting.
///
/// Controls what signal is routed to the channel's PGA input.
/// `Normal` is the standard choice for EEG/EMG recording; the others are
/// primarily used for calibration or diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    /// Normal electrode input — the channel measures the voltage between
    /// its `IN+` and `IN−` pins.
    Normal        = 0,
    /// Both inputs shorted together — measures the amplifier's intrinsic
    /// noise floor.
    Shorted       = 1,
    /// BIAS drive measurement.
    BiasMeas      = 2,
    /// Supply mid-point (MVDD) — useful for supply monitoring.
    Mvdd          = 3,
    /// Internal temperature sensor.
    Temp          = 4,
    /// Internal test signal (square wave at `f_CLK / 2^21`).
    TestSig       = 5,
    /// BIAS_DRP — positive drive of the bias signal.
    BiasDrp       = 6,
    /// BIAS_DRN — negative drive of the bias signal.
    BiasDrn       = 7,
}

impl Default for InputType {
    fn default() -> Self {
        InputType::Normal
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for a single ADS1299 channel.
///
/// Used to generate OpenBCI channel-setting commands of the form:
/// ```text
/// x (CHANNEL) (POWER_DOWN) (GAIN_SET) (INPUT_TYPE_SET) (BIAS_SET) (SRB2_SET) (SRB1_SET) X
/// ```
///
/// # Example
/// ```rust
/// use openbci::channel_config::{ChannelConfig, Gain, InputType};
///
/// // High-gain EEG channel included in bias and SRB2 network
/// let cfg = ChannelConfig::default()
///     .gain(Gain::X24)
///     .input_type(InputType::Normal)
///     .bias(true)
///     .srb2(true)
///     .srb1(false);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelConfig {
    /// Whether the channel is powered on (`true`) or off (`false`).
    pub power: bool,
    pub gain: Gain,
    pub input_type: InputType,
    /// Include this channel in the bias signal.
    pub bias: bool,
    /// Connect SRB2 pin to this channel (common reference).
    pub srb2: bool,
    /// Connect all channels to SRB1 (only used in differential mode).
    pub srb1: bool,
}

impl Default for ChannelConfig {
    /// Default Cyton settings: powered on, 24× gain, normal input, bias+SRB2 enabled.
    fn default() -> Self {
        Self {
            power:      true,
            gain:       Gain::X24,
            input_type: InputType::Normal,
            bias:       true,
            srb2:       true,
            srb1:       false,
        }
    }
}

impl ChannelConfig {
    /// Set the amplifier gain.  Affects the µV scale factor used at decode time.
    pub fn gain(mut self, g: Gain) -> Self         { self.gain = g; self }
    /// Set the input multiplexer mode.  Use [`InputType::Normal`] for live EEG/EMG.
    pub fn input_type(mut self, t: InputType) -> Self { self.input_type = t; self }
    /// Power the channel on (`true`, default) or off (`false`).
    ///
    /// A powered-off channel stops consuming current and outputs 0.
    pub fn power(mut self, on: bool) -> Self        { self.power = on; self }
    /// Include (`true`) or exclude (`false`) this channel in the bias drive.
    ///
    /// Channels in the bias network are used to drive the body's common-mode
    /// voltage toward mid-supply, reducing 50/60 Hz interference.
    pub fn bias(mut self, b: bool) -> Self          { self.bias = b; self }
    /// Connect (`true`) or disconnect (`false`) SRB2 as this channel's reference.
    ///
    /// SRB2 is the per-channel reference input.  Typically enabled for EEG so
    /// all channels share the same driven reference electrode.
    pub fn srb2(mut self, s: bool) -> Self          { self.srb2 = s; self }
    /// Connect all channels to SRB1 simultaneously (`true`) for differential mode.
    ///
    /// Rarely needed — only used in specific bipolar measurement configurations.
    /// Default is `false`.
    pub fn srb1(mut self, s: bool) -> Self          { self.srb1 = s; self }

    /// Build the OpenBCI channel-setting command string.
    ///
    /// `channel_letter` — the board's letter for this channel, e.g. `'1'`–`'8'`
    /// for Cyton channels 1–8, or `'Q'`/`'W'`/`'E'`/`'R'`/`'T'`/`'Y'`/`'U'`/`'I'`
    /// for the Daisy channels.
    ///
    /// Returns a 9-character string like `"x1060110X"`.
    pub fn to_command(&self, channel_letter: char) -> String {
        format!(
            "x{}{}{}{}{}{}{}X",
            channel_letter,
            if self.power { '0' } else { '1' }, // 0=powered on, 1=off
            self.gain.code(),
            self.input_type as u8,
            if self.bias  { '1' } else { '0' },
            if self.srb2  { '1' } else { '0' },
            if self.srb1  { '1' } else { '0' },
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// OpenBCI channel letters used in `x...X` configuration commands.
///
/// Index 0–7 are the Cyton board channels (`'1'`–`'8'`).
/// Index 8–15 are the Daisy expansion channels (`'Q'`/`'W'`/`'E'`/`'R'`/`'T'`/`'Y'`/`'U'`/`'I'`).
///
/// Used by [`ChannelConfig::to_command`] and [`GainTracker::apply_command`].
pub const CHANNEL_LETTERS: [char; 16] = [
    '1','2','3','4','5','6','7','8',      // Cyton   (ADS1299 #1)
    'Q','W','E','R','T','Y','U','I',      // Daisy   (ADS1299 #2)
];

/// Gain multipliers indexed by the ADS1299 gain code (0–6).
///
/// `GAIN_VALUES[gain_code]` gives the numeric multiplier.
pub const GAIN_VALUES: [f64; 7] = [1.0, 2.0, 4.0, 6.0, 8.0, 12.0, 24.0];

/// Decode a gain code (0–6) back to its numeric multiplier.
///
/// Returns `1.0` for any code outside the valid range.
pub fn gain_from_code(code: u8) -> f64 {
    GAIN_VALUES.get(code as usize).copied().unwrap_or(1.0)
}

// ─────────────────────────────────────────────────────────────────────────────

/// Tracks the ADS1299 gain setting for each channel.
///
/// Because the gain affects the µV scale factor, the driver must know the
/// current gain of every channel at decode time.  `GainTracker` parses
/// `x...X` command strings as they are sent and updates its internal table.
///
/// Boards initialise this with the factory-default gain (24× for Cyton).
#[derive(Debug, Clone)]
pub struct GainTracker {
    gains: Vec<f64>,
}

impl GainTracker {
    /// Create a new `GainTracker` with an explicit per-channel gain table.
    ///
    /// `default_gains` should have one entry per channel, ordered by channel
    /// index.
    pub fn new(default_gains: Vec<f64>) -> Self {
        Self { gains: default_gains }
    }

    /// Return the current gain multiplier for channel `channel_idx` (0-based).
    ///
    /// Returns `1.0` if `channel_idx` is out of range.
    pub fn gain_for(&self, channel_idx: usize) -> f64 {
        self.gains.get(channel_idx).copied().unwrap_or(1.0)
    }

    /// Parse `cmd` for `x...X` channel-setting sequences and update the
    /// internal gain table for any channels they reference.
    ///
    /// Also handles the `"d"` (defaults) command, which resets all gains to 24×.
    ///
    /// Always returns `true` (kept for forward-compatibility).
    pub fn apply_command(&mut self, cmd: &str) -> bool {
        let bytes = cmd.as_bytes();
        let mut i = 0;
        while i + 8 < bytes.len() {
            if bytes[i] == b'x' && bytes[i + 8] == b'X' {
                let letter    = bytes[i + 1] as char;
                let gain_code = bytes[i + 3].wrapping_sub(b'0');
                if let Some(ch_idx) = CHANNEL_LETTERS.iter().position(|&c| c == letter) {
                    if (gain_code as usize) < GAIN_VALUES.len() {
                        self.gains[ch_idx] = GAIN_VALUES[gain_code as usize];
                    }
                }
                i += 9;
            } else {
                i += 1;
            }
        }
        if cmd == "d" {
            for g in &mut self.gains {
                *g = 24.0;
            }
        }
        true
    }

    /// Return a slice of all current gain multipliers, ordered by channel index.
    pub fn gains(&self) -> &[f64] {
        &self.gains
    }
}
