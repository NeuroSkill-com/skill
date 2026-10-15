//! Electrode placement, standard montages, and position lookup.
//!
//! ## Montage data
//! Positions are sourced from [MNE-Python]'s `standard_1020.elc` and
//! `standard_1005.elc` montage files (BSD-3-Clause licence).
//!
//! Three pre-defined montage slices are provided:
//! - [`MONTAGE_1020`] — 83 positions (10-20 standard)
//! - [`MONTAGE_1010`] — 176 positions (10-10; adds midpoints between 10-20)
//! - [`MONTAGE_1005`] — 334 positions (10-05; full half-step density)
//!
//! [MNE-Python]: https://mne.tools/stable/generated/mne.channels.make_standard_montage.html

use std::fmt;

// ── Signal type ──────────────────────────────────────────────────────────────

/// Signal category for a single electrode channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalType {
    /// EEG — electroencephalography.
    Eeg,
    /// EMG — electromyography.
    Emg,
    /// EOG — electrooculography (eye movement).
    Eog,
    /// ECG/EKG — electrocardiography.
    Ecg,
    /// Generic analogue reference.
    Reference,
    /// Any other / custom signal.
    Other(String),
}

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalType::Eeg       => write!(f, "EEG"),
            SignalType::Emg       => write!(f, "EMG"),
            SignalType::Eog       => write!(f, "EOG"),
            SignalType::Ecg       => write!(f, "ECG"),
            SignalType::Reference => write!(f, "REF"),
            SignalType::Other(s)  => write!(f, "{s}"),
        }
    }
}

// ── Single electrode description ─────────────────────────────────────────────

/// Description of one electrode (channel) in a cap.
#[derive(Debug, Clone)]
pub struct Electrode {
    /// Label, e.g. `"Cz"`, `"T7"`, `"Left Motor"`.
    pub label: String,
    /// Signal modality.
    pub signal_type: SignalType,
    /// Optional free-text note (anatomical region, impedance info, …).
    pub note: Option<String>,
}

impl Electrode {
    /// Construct an EEG electrode from a label.
    pub fn eeg(label: &str) -> Self {
        Electrode { label: label.to_string(), signal_type: SignalType::Eeg, note: None }
    }

    /// Construct an EMG electrode from a label.
    pub fn emg(label: &str) -> Self {
        Electrode { label: label.to_string(), signal_type: SignalType::Emg, note: None }
    }

    /// Attach a note.
    pub fn with_note(mut self, note: &str) -> Self {
        self.note = Some(note.to_string());
        self
    }

    /// Look up the 3-D head position for this electrode in the 10-05 montage.
    /// Returns `None` for custom / non-standard labels.
    pub fn position(&self) -> Option<ElectrodePosition> {
        position(&self.label)
    }
}

impl fmt::Display for Electrode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label)
    }
}

// ── Electrode layout ─────────────────────────────────────────────────────────

/// Maps channel indices (0-based) to electrode metadata.
///
/// Channels without an explicit assignment default to `"Ch{n+1}"` with
/// signal type `Eeg`.
///
/// # Example
/// ```rust,no_run
/// use openbci::electrode::{ElectrodeLayout, Electrode, positions};
///
/// let layout = ElectrodeLayout::from_labels(&[
///     positions::FP1, positions::FP2,
///     positions::C3,  positions::CZ,
///     positions::C4,  positions::P3,
///     positions::PZ,  positions::P4,
/// ]);
/// println!("{}", layout.label(0));  // "Fp1"
/// println!("{}", layout.label(2));  // "C3"
/// ```
#[derive(Debug, Clone)]
pub struct ElectrodeLayout {
    channels: usize,
    electrodes: Vec<Option<Electrode>>,
}

impl ElectrodeLayout {
    /// Create a layout with `n` unassigned channels.
    pub fn new(channels: usize) -> Self {
        ElectrodeLayout { channels, electrodes: vec![None; channels] }
    }

    /// Create from a slice of label strings.  Signal type defaults to `Eeg`.
    /// Alias resolution is applied (e.g. `"T3"` is stored as `"T7"`).
    pub fn from_labels(labels: &[&str]) -> Self {
        let mut layout = Self::new(labels.len());
        for (i, &lbl) in labels.iter().enumerate() {
            let canonical = resolve_alias(lbl);
            layout.electrodes[i] = Some(Electrode::eeg(canonical));
        }
        layout
    }

    /// Assign an electrode to a channel index (builder style — takes ownership).
    pub fn with_electrode(mut self, channel: usize, electrode: Electrode) -> Self {
        if channel < self.channels {
            self.electrodes[channel] = Some(electrode);
        }
        self
    }

    /// Assign an electrode to a channel index (mutable setter).
    pub fn set_electrode(&mut self, channel: usize, electrode: Electrode) {
        if channel < self.channels {
            self.electrodes[channel] = Some(electrode);
        }
    }

    /// Number of channels.
    pub fn len(&self) -> usize { self.channels }

    /// Returns `true` if there are no channels.
    pub fn is_empty(&self) -> bool { self.channels == 0 }

    /// Return the label for channel `i`, or `"Ch{i+1}"` if unassigned.
    pub fn label(&self, channel: usize) -> String {
        self.electrodes
            .get(channel)
            .and_then(|e| e.as_ref())
            .map(|e| e.label.clone())
            .unwrap_or_else(|| format!("Ch{}", channel + 1))
    }

    /// Return a reference to the electrode at channel `i`, if assigned.
    pub fn get(&self, channel: usize) -> Option<&Electrode> {
        self.electrodes.get(channel).and_then(|e| e.as_ref())
    }

    /// Look up the 3-D position of channel `i` in the 10-05 montage.
    pub fn position(&self, channel: usize) -> Option<ElectrodePosition> {
        self.get(channel).and_then(|e| e.position())
    }

    /// Return an iterator over `(channel_index, label)` pairs.
    pub fn iter_labels(&self) -> impl Iterator<Item = (usize, String)> + '_ {
        (0..self.channels).map(move |i| (i, self.label(i)))
    }

    /// Return an iterator over `(channel_index, &Electrode)` for assigned channels.
    pub fn iter_electrodes(&self) -> impl Iterator<Item = (usize, &Electrode)> {
        self.electrodes.iter().enumerate()
            .filter_map(|(i, opt)| opt.as_ref().map(|e| (i, e)))
    }

    /// Shorthand: iterate over `(channel_index, &Electrode)` for all assigned channels.
    /// Equivalent to [`iter_electrodes`](Self::iter_electrodes).
    pub fn iter(&self) -> impl Iterator<Item = (usize, &Electrode)> {
        self.iter_electrodes()
    }

    /// Return all channel labels in order as a `Vec<String>`.
    /// Unassigned channels appear as `"Ch{n+1}"`.
    pub fn labels(&self) -> Vec<String> {
        (0..self.channels).map(|i| self.label(i)).collect()
    }

    /// Build the 10-20 subset layout: returns only channels whose labels are
    /// present in `MONTAGE_1020`.
    pub fn subset_1020(&self) -> Vec<(usize, &Electrode)> {
        self.iter_electrodes()
            .filter(|(_, e)| position_1020(&e.label).is_some())
            .collect()
    }

    /// Build the 10-10 subset layout: returns only channels whose labels are
    /// present in `MONTAGE_1010`.
    pub fn subset_1010(&self) -> Vec<(usize, &Electrode)> {
        self.iter_electrodes()
            .filter(|(_, e)| position_1010(&e.label).is_some())
            .collect()
    }
}

impl Default for ElectrodeLayout {
    fn default() -> Self { Self::new(0) }
}

// ── Predefined layouts for common OpenBCI configurations ─────────────────────

/// Standard 8-channel Cyton layout (motor cortex + frontal).
pub fn cyton_motor() -> ElectrodeLayout {
    ElectrodeLayout::from_labels(&[
        positions::C3, positions::C4,
        positions::CZ,
        positions::FC3, positions::FC4,
        positions::CP3, positions::CP4,
        positions::FZ,
    ])
}

/// Standard 16-channel Cyton+Daisy layout (full-cap 10-20 subset).
pub fn cyton_daisy_standard() -> ElectrodeLayout {
    ElectrodeLayout::from_labels(&[
        // Cyton  (ch 0–7)
        positions::FP1, positions::FP2,
        positions::F3,  positions::F4,
        positions::C3,  positions::C4,
        positions::P3,  positions::P4,
        // Daisy  (ch 8–15)
        positions::O1,  positions::O2,
        positions::F7,  positions::F8,
        positions::T7,  positions::T8,
        positions::FZ,  positions::PZ,
    ])
}

/// 4-channel Ganglion default layout (frontal + occipital).
pub fn ganglion_default() -> ElectrodeLayout {
    ElectrodeLayout::from_labels(&[
        positions::FP1, positions::FP2,
        positions::O1,  positions::O2,
    ])
}

// ── Generated montage data ────────────────────────────────────────────────────

/// 3D (x, y, z) position in metres from the MNE standard_1020 montage.
/// Coordinate system: X = left→right, Y = back→front, Z = down→up.
/// Source: MNE-Python `standard_1020.elc` / `standard_1005.elc` (BSD-3).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElectrodePosition {
    /// Standard name, e.g. `"Cz"`, `"FCC3h"`.
    pub label: &'static str,
    /// Left-right, metres (left = negative).
    pub x: f64,
    /// Anterior-posterior, metres (front = positive).
    pub y: f64,
    /// Superior-inferior, metres (up = positive).
    pub z: f64,
}

impl ElectrodePosition {
    /// Returns the Euclidean distance from the head origin (in metres).
    pub fn radius(&self) -> f64 {
        (self.x*self.x + self.y*self.y + self.z*self.z).sqrt()
    }
}

/// All 83 electrode positions from the standard **10-20** system.
/// Positions are in metres, referenced to the head origin.
pub static MONTAGE_1020: &[ElectrodePosition] = &[
    ElectrodePosition { label: "Fp1", x: -0.029437, y: 0.083917, z: -0.006990 },
    ElectrodePosition { label: "Fpz", x: 0.000112, y: 0.088247, z: -0.001713 },
    ElectrodePosition { label: "Fp2", x: 0.029872, y: 0.084896, z: -0.007080 },
    ElectrodePosition { label: "AF9", x: -0.048971, y: 0.064087, z: -0.047683 },
    ElectrodePosition { label: "AF7", x: -0.054840, y: 0.068572, z: -0.010590 },
    ElectrodePosition { label: "AF5", x: -0.045431, y: 0.072862, z: 0.005978 },
    ElectrodePosition { label: "AF3", x: -0.033701, y: 0.076837, z: 0.021227 },
    ElectrodePosition { label: "AF1", x: -0.018472, y: 0.079904, z: 0.032752 },
    ElectrodePosition { label: "AFz", x: 0.000231, y: 0.080771, z: 0.035417 },
    ElectrodePosition { label: "AF2", x: 0.019820, y: 0.080302, z: 0.032764 },
    ElectrodePosition { label: "AF4", x: 0.035712, y: 0.077726, z: 0.021956 },
    ElectrodePosition { label: "AF6", x: 0.046584, y: 0.073808, z: 0.006034 },
    ElectrodePosition { label: "AF8", x: 0.055743, y: 0.069657, z: -0.010755 },
    ElectrodePosition { label: "AF10", x: 0.050435, y: 0.063870, z: -0.048005 },
    ElectrodePosition { label: "F9", x: -0.070102, y: 0.041652, z: -0.049952 },
    ElectrodePosition { label: "F7", x: -0.070263, y: 0.042474, z: -0.011420 },
    ElectrodePosition { label: "F5", x: -0.064466, y: 0.048035, z: 0.016921 },
    ElectrodePosition { label: "F3", x: -0.050244, y: 0.053111, z: 0.042192 },
    ElectrodePosition { label: "F1", x: -0.027496, y: 0.056931, z: 0.060342 },
    ElectrodePosition { label: "Fz", x: 0.000312, y: 0.058512, z: 0.066462 },
    ElectrodePosition { label: "F2", x: 0.029514, y: 0.057602, z: 0.059540 },
    ElectrodePosition { label: "F4", x: 0.051836, y: 0.054305, z: 0.040814 },
    ElectrodePosition { label: "F6", x: 0.067914, y: 0.049830, z: 0.016367 },
    ElectrodePosition { label: "F8", x: 0.073043, y: 0.044422, z: -0.012000 },
    ElectrodePosition { label: "F10", x: 0.072114, y: 0.042067, z: -0.050452 },
    ElectrodePosition { label: "FT9", x: -0.084076, y: 0.014567, z: -0.050429 },
    ElectrodePosition { label: "FT7", x: -0.080775, y: 0.014120, z: -0.011135 },
    ElectrodePosition { label: "FC5", x: -0.077215, y: 0.018643, z: 0.024460 },
    ElectrodePosition { label: "FC3", x: -0.060182, y: 0.022716, z: 0.055544 },
    ElectrodePosition { label: "FC1", x: -0.034062, y: 0.026011, z: 0.079987 },
    ElectrodePosition { label: "FCz", x: 0.000376, y: 0.027390, z: 0.088668 },
    ElectrodePosition { label: "FC2", x: 0.034784, y: 0.026438, z: 0.078808 },
    ElectrodePosition { label: "FC4", x: 0.062293, y: 0.023723, z: 0.055630 },
    ElectrodePosition { label: "FC6", x: 0.079534, y: 0.019936, z: 0.024438 },
    ElectrodePosition { label: "FT8", x: 0.081815, y: 0.015417, z: -0.011330 },
    ElectrodePosition { label: "FT10", x: 0.084113, y: 0.014365, z: -0.050538 },
    ElectrodePosition { label: "T9", x: -0.085894, y: -0.015829, z: -0.048283 },
    ElectrodePosition { label: "T7", x: -0.084161, y: -0.016019, z: -0.009346 },
    ElectrodePosition { label: "C5", x: -0.080280, y: -0.013760, z: 0.029160 },
    ElectrodePosition { label: "C3", x: -0.065358, y: -0.011632, z: 0.064358 },
    ElectrodePosition { label: "C1", x: -0.036158, y: -0.009984, z: 0.089752 },
    ElectrodePosition { label: "Cz", x: 0.000401, y: -0.009167, z: 0.100244 },
    ElectrodePosition { label: "C2", x: 0.037672, y: -0.009624, z: 0.088412 },
    ElectrodePosition { label: "C4", x: 0.067118, y: -0.010900, z: 0.063580 },
    ElectrodePosition { label: "C6", x: 0.083456, y: -0.012776, z: 0.029208 },
    ElectrodePosition { label: "T8", x: 0.085080, y: -0.015020, z: -0.009490 },
    ElectrodePosition { label: "T10", x: 0.085560, y: -0.016361, z: -0.048271 },
    ElectrodePosition { label: "TP9", x: -0.085619, y: -0.046515, z: -0.045707 },
    ElectrodePosition { label: "TP7", x: -0.084830, y: -0.046022, z: -0.007056 },
    ElectrodePosition { label: "CP5", x: -0.079592, y: -0.046551, z: 0.030949 },
    ElectrodePosition { label: "CP3", x: -0.063556, y: -0.047009, z: 0.065624 },
    ElectrodePosition { label: "CP1", x: -0.035513, y: -0.047292, z: 0.091315 },
    ElectrodePosition { label: "CPz", x: 0.000386, y: -0.047318, z: 0.099432 },
    ElectrodePosition { label: "CP2", x: 0.038384, y: -0.047073, z: 0.090695 },
    ElectrodePosition { label: "CP4", x: 0.066612, y: -0.046637, z: 0.065580 },
    ElectrodePosition { label: "CP6", x: 0.083322, y: -0.046101, z: 0.031206 },
    ElectrodePosition { label: "TP8", x: 0.085549, y: -0.045545, z: -0.007130 },
    ElectrodePosition { label: "TP10", x: 0.086162, y: -0.047035, z: -0.045869 },
    ElectrodePosition { label: "P9", x: -0.073009, y: -0.073766, z: -0.040998 },
    ElectrodePosition { label: "P7", x: -0.072434, y: -0.073453, z: -0.002487 },
    ElectrodePosition { label: "P5", x: -0.067272, y: -0.076291, z: 0.028382 },
    ElectrodePosition { label: "P3", x: -0.053007, y: -0.078788, z: 0.055940 },
    ElectrodePosition { label: "P1", x: -0.028620, y: -0.080525, z: 0.075436 },
    ElectrodePosition { label: "Pz", x: 0.000325, y: -0.081115, z: 0.082615 },
    ElectrodePosition { label: "P2", x: 0.031920, y: -0.080487, z: 0.076716 },
    ElectrodePosition { label: "P4", x: 0.055667, y: -0.078560, z: 0.056561 },
    ElectrodePosition { label: "P6", x: 0.067888, y: -0.075904, z: 0.028091 },
    ElectrodePosition { label: "P8", x: 0.073056, y: -0.073068, z: -0.002540 },
    ElectrodePosition { label: "P10", x: 0.073895, y: -0.074390, z: -0.041220 },
    ElectrodePosition { label: "PO9", x: -0.054910, y: -0.098045, z: -0.035465 },
    ElectrodePosition { label: "PO7", x: -0.054840, y: -0.097528, z: 0.002792 },
    ElectrodePosition { label: "PO5", x: -0.048424, y: -0.099341, z: 0.021599 },
    ElectrodePosition { label: "PO3", x: -0.036511, y: -0.100853, z: 0.037167 },
    ElectrodePosition { label: "PO1", x: -0.018972, y: -0.101768, z: 0.046536 },
    ElectrodePosition { label: "POz", x: 0.000216, y: -0.102178, z: 0.050608 },
    ElectrodePosition { label: "PO2", x: 0.019878, y: -0.101793, z: 0.046393 },
    ElectrodePosition { label: "PO4", x: 0.036782, y: -0.100849, z: 0.036397 },
    ElectrodePosition { label: "PO6", x: 0.049820, y: -0.099446, z: 0.021727 },
    ElectrodePosition { label: "PO8", x: 0.055667, y: -0.097625, z: 0.002730 },
    ElectrodePosition { label: "PO10", x: 0.054988, y: -0.098091, z: -0.035541 },
    ElectrodePosition { label: "O1", x: -0.029413, y: -0.112449, z: 0.008839 },
    ElectrodePosition { label: "Oz", x: 0.000108, y: -0.114892, z: 0.014657 },
    ElectrodePosition { label: "O2", x: 0.029843, y: -0.112156, z: 0.008800 },
];

/// All 176 electrode positions from the standard **10-10** system
/// (a superset of 10-20; adds midpoints at 10% intervals).
pub static MONTAGE_1010: &[ElectrodePosition] = &[
    ElectrodePosition { label: "Fp1", x: -0.029437, y: 0.083917, z: -0.006990 },
    ElectrodePosition { label: "Fpz", x: 0.000112, y: 0.088247, z: -0.001713 },
    ElectrodePosition { label: "Fp2", x: 0.029872, y: 0.084896, z: -0.007080 },
    ElectrodePosition { label: "AF9", x: -0.048971, y: 0.064087, z: -0.047683 },
    ElectrodePosition { label: "AF7", x: -0.054840, y: 0.068572, z: -0.010590 },
    ElectrodePosition { label: "AF5", x: -0.045431, y: 0.072862, z: 0.005978 },
    ElectrodePosition { label: "AF3", x: -0.033701, y: 0.076837, z: 0.021227 },
    ElectrodePosition { label: "AF1", x: -0.018472, y: 0.079904, z: 0.032752 },
    ElectrodePosition { label: "AFz", x: 0.000231, y: 0.080771, z: 0.035417 },
    ElectrodePosition { label: "AF2", x: 0.019820, y: 0.080302, z: 0.032764 },
    ElectrodePosition { label: "AF4", x: 0.035712, y: 0.077726, z: 0.021956 },
    ElectrodePosition { label: "AF6", x: 0.046584, y: 0.073808, z: 0.006034 },
    ElectrodePosition { label: "AF8", x: 0.055743, y: 0.069657, z: -0.010755 },
    ElectrodePosition { label: "AF10", x: 0.050435, y: 0.063870, z: -0.048005 },
    ElectrodePosition { label: "F9", x: -0.070102, y: 0.041652, z: -0.049952 },
    ElectrodePosition { label: "F7", x: -0.070263, y: 0.042474, z: -0.011420 },
    ElectrodePosition { label: "F5", x: -0.064466, y: 0.048035, z: 0.016921 },
    ElectrodePosition { label: "F3", x: -0.050244, y: 0.053111, z: 0.042192 },
    ElectrodePosition { label: "F1", x: -0.027496, y: 0.056931, z: 0.060342 },
    ElectrodePosition { label: "Fz", x: 0.000312, y: 0.058512, z: 0.066462 },
    ElectrodePosition { label: "F2", x: 0.029514, y: 0.057602, z: 0.059540 },
    ElectrodePosition { label: "F4", x: 0.051836, y: 0.054305, z: 0.040814 },
    ElectrodePosition { label: "F6", x: 0.067914, y: 0.049830, z: 0.016367 },
    ElectrodePosition { label: "F8", x: 0.073043, y: 0.044422, z: -0.012000 },
    ElectrodePosition { label: "F10", x: 0.072114, y: 0.042067, z: -0.050452 },
    ElectrodePosition { label: "FT9", x: -0.084076, y: 0.014567, z: -0.050429 },
    ElectrodePosition { label: "FT7", x: -0.080775, y: 0.014120, z: -0.011135 },
    ElectrodePosition { label: "FC5", x: -0.077215, y: 0.018643, z: 0.024460 },
    ElectrodePosition { label: "FC3", x: -0.060182, y: 0.022716, z: 0.055544 },
    ElectrodePosition { label: "FC1", x: -0.034062, y: 0.026011, z: 0.079987 },
    ElectrodePosition { label: "FCz", x: 0.000376, y: 0.027390, z: 0.088668 },
    ElectrodePosition { label: "FC2", x: 0.034784, y: 0.026438, z: 0.078808 },
    ElectrodePosition { label: "FC4", x: 0.062293, y: 0.023723, z: 0.055630 },
    ElectrodePosition { label: "FC6", x: 0.079534, y: 0.019936, z: 0.024438 },
    ElectrodePosition { label: "FT8", x: 0.081815, y: 0.015417, z: -0.011330 },
    ElectrodePosition { label: "FT10", x: 0.084113, y: 0.014365, z: -0.050538 },
    ElectrodePosition { label: "T9", x: -0.085894, y: -0.015829, z: -0.048283 },
    ElectrodePosition { label: "T7", x: -0.084161, y: -0.016019, z: -0.009346 },
    ElectrodePosition { label: "C5", x: -0.080280, y: -0.013760, z: 0.029160 },
    ElectrodePosition { label: "C3", x: -0.065358, y: -0.011632, z: 0.064358 },
    ElectrodePosition { label: "C1", x: -0.036158, y: -0.009984, z: 0.089752 },
    ElectrodePosition { label: "Cz", x: 0.000401, y: -0.009167, z: 0.100244 },
    ElectrodePosition { label: "C2", x: 0.037672, y: -0.009624, z: 0.088412 },
    ElectrodePosition { label: "C4", x: 0.067118, y: -0.010900, z: 0.063580 },
    ElectrodePosition { label: "C6", x: 0.083456, y: -0.012776, z: 0.029208 },
    ElectrodePosition { label: "T8", x: 0.085080, y: -0.015020, z: -0.009490 },
    ElectrodePosition { label: "T10", x: 0.085560, y: -0.016361, z: -0.048271 },
    ElectrodePosition { label: "TP9", x: -0.085619, y: -0.046515, z: -0.045707 },
    ElectrodePosition { label: "TP7", x: -0.084830, y: -0.046022, z: -0.007056 },
    ElectrodePosition { label: "CP5", x: -0.079592, y: -0.046551, z: 0.030949 },
    ElectrodePosition { label: "CP3", x: -0.063556, y: -0.047009, z: 0.065624 },
    ElectrodePosition { label: "CP1", x: -0.035513, y: -0.047292, z: 0.091315 },
    ElectrodePosition { label: "CPz", x: 0.000386, y: -0.047318, z: 0.099432 },
    ElectrodePosition { label: "CP2", x: 0.038384, y: -0.047073, z: 0.090695 },
    ElectrodePosition { label: "CP4", x: 0.066612, y: -0.046637, z: 0.065580 },
    ElectrodePosition { label: "CP6", x: 0.083322, y: -0.046101, z: 0.031206 },
    ElectrodePosition { label: "TP8", x: 0.085549, y: -0.045545, z: -0.007130 },
    ElectrodePosition { label: "TP10", x: 0.086162, y: -0.047035, z: -0.045869 },
    ElectrodePosition { label: "P9", x: -0.073009, y: -0.073766, z: -0.040998 },
    ElectrodePosition { label: "P7", x: -0.072434, y: -0.073453, z: -0.002487 },
    ElectrodePosition { label: "P5", x: -0.067272, y: -0.076291, z: 0.028382 },
    ElectrodePosition { label: "P3", x: -0.053007, y: -0.078788, z: 0.055940 },
    ElectrodePosition { label: "P1", x: -0.028620, y: -0.080525, z: 0.075436 },
    ElectrodePosition { label: "Pz", x: 0.000325, y: -0.081115, z: 0.082615 },
    ElectrodePosition { label: "P2", x: 0.031920, y: -0.080487, z: 0.076716 },
    ElectrodePosition { label: "P4", x: 0.055667, y: -0.078560, z: 0.056561 },
    ElectrodePosition { label: "P6", x: 0.067888, y: -0.075904, z: 0.028091 },
    ElectrodePosition { label: "P8", x: 0.073056, y: -0.073068, z: -0.002540 },
    ElectrodePosition { label: "P10", x: 0.073895, y: -0.074390, z: -0.041220 },
    ElectrodePosition { label: "PO9", x: -0.054910, y: -0.098045, z: -0.035465 },
    ElectrodePosition { label: "PO7", x: -0.054840, y: -0.097528, z: 0.002792 },
    ElectrodePosition { label: "PO5", x: -0.048424, y: -0.099341, z: 0.021599 },
    ElectrodePosition { label: "PO3", x: -0.036511, y: -0.100853, z: 0.037167 },
    ElectrodePosition { label: "PO1", x: -0.018972, y: -0.101768, z: 0.046536 },
    ElectrodePosition { label: "POz", x: 0.000216, y: -0.102178, z: 0.050608 },
    ElectrodePosition { label: "PO2", x: 0.019878, y: -0.101793, z: 0.046393 },
    ElectrodePosition { label: "PO4", x: 0.036782, y: -0.100849, z: 0.036397 },
    ElectrodePosition { label: "PO6", x: 0.049820, y: -0.099446, z: 0.021727 },
    ElectrodePosition { label: "PO8", x: 0.055667, y: -0.097625, z: 0.002730 },
    ElectrodePosition { label: "PO10", x: 0.054988, y: -0.098091, z: -0.035541 },
    ElectrodePosition { label: "O1", x: -0.029413, y: -0.112449, z: 0.008839 },
    ElectrodePosition { label: "Oz", x: 0.000108, y: -0.114892, z: 0.014657 },
    ElectrodePosition { label: "O2", x: 0.029843, y: -0.112156, z: 0.008800 },
    ElectrodePosition { label: "I1", x: -0.029818, y: -0.114570, z: -0.029216 },
    ElectrodePosition { label: "I2", x: 0.029742, y: -0.114260, z: -0.029256 },
    ElectrodePosition { label: "AFp9", x: -0.036125, y: 0.072380, z: -0.045852 },
    ElectrodePosition { label: "AFp7", x: -0.043512, y: 0.078580, z: -0.009240 },
    ElectrodePosition { label: "AFp5", x: -0.033285, y: 0.081207, z: -0.001140 },
    ElectrodePosition { label: "AFp3", x: -0.022352, y: 0.083562, z: 0.006071 },
    ElectrodePosition { label: "AFp1", x: -0.012242, y: 0.086194, z: 0.014188 },
    ElectrodePosition { label: "AFpz", x: 0.000170, y: 0.087322, z: 0.017442 },
    ElectrodePosition { label: "AFp2", x: 0.013622, y: 0.086758, z: 0.015302 },
    ElectrodePosition { label: "AFp4", x: 0.024101, y: 0.084377, z: 0.007433 },
    ElectrodePosition { label: "AFp6", x: 0.033913, y: 0.081812, z: -0.001035 },
    ElectrodePosition { label: "AFp8", x: 0.043948, y: 0.079296, z: -0.009300 },
    ElectrodePosition { label: "AFp10", x: 0.037712, y: 0.072168, z: -0.046197 },
    ElectrodePosition { label: "AFF9", x: -0.059340, y: 0.052680, z: -0.048770 },
    ElectrodePosition { label: "AFF7", x: -0.063262, y: 0.055992, z: -0.011173 },
    ElectrodePosition { label: "AFF5", x: -0.055820, y: 0.061396, z: 0.011884 },
    ElectrodePosition { label: "AFF3", x: -0.043382, y: 0.066367, z: 0.032811 },
    ElectrodePosition { label: "AFF1", x: -0.023582, y: 0.069917, z: 0.047293 },
    ElectrodePosition { label: "AFFz", x: 0.000276, y: 0.071280, z: 0.052092 },
    ElectrodePosition { label: "AFF2", x: 0.025558, y: 0.070556, z: 0.047827 },
    ElectrodePosition { label: "AFF4", x: 0.045152, y: 0.067275, z: 0.032731 },
    ElectrodePosition { label: "AFF6", x: 0.058000, y: 0.062600, z: 0.011900 },
    ElectrodePosition { label: "AFF8", x: 0.064673, y: 0.057274, z: -0.011460 },
    ElectrodePosition { label: "AFF10", x: 0.060601, y: 0.052267, z: -0.049038 },
    ElectrodePosition { label: "FFT9", x: -0.078484, y: 0.028770, z: -0.050522 },
    ElectrodePosition { label: "FFT7", x: -0.076615, y: 0.028653, z: -0.011508 },
    ElectrodePosition { label: "FFC5", x: -0.071506, y: 0.033926, z: 0.020993 },
    ElectrodePosition { label: "FFC3", x: -0.055940, y: 0.038716, z: 0.049788 },
    ElectrodePosition { label: "FFC1", x: -0.030655, y: 0.042415, z: 0.071040 },
    ElectrodePosition { label: "FFCz", x: 0.000351, y: 0.044074, z: 0.079141 },
    ElectrodePosition { label: "FFC2", x: 0.032645, y: 0.043101, z: 0.070795 },
    ElectrodePosition { label: "FFC4", x: 0.057504, y: 0.039852, z: 0.048811 },
    ElectrodePosition { label: "FFC6", x: 0.074250, y: 0.035500, z: 0.020380 },
    ElectrodePosition { label: "FFT8", x: 0.079034, y: 0.030344, z: -0.011997 },
    ElectrodePosition { label: "FFT10", x: 0.079920, y: 0.028942, z: -0.050914 },
    ElectrodePosition { label: "FTT9", x: -0.087362, y: -0.000515, z: -0.049837 },
    ElectrodePosition { label: "FTT7", x: -0.082668, y: -0.000942, z: -0.010284 },
    ElectrodePosition { label: "FCC5", x: -0.080133, y: 0.002585, z: 0.027312 },
    ElectrodePosition { label: "FCC3", x: -0.064161, y: 0.005831, z: 0.060885 },
    ElectrodePosition { label: "FCC1", x: -0.035749, y: 0.008309, z: 0.085459 },
    ElectrodePosition { label: "FCCz", x: 0.000391, y: 0.009508, z: 0.095560 },
    ElectrodePosition { label: "FCC2", x: 0.036070, y: 0.008652, z: 0.083832 },
    ElectrodePosition { label: "FCC4", x: 0.065164, y: 0.006620, z: 0.060052 },
    ElectrodePosition { label: "FCC6", x: 0.081544, y: 0.003664, z: 0.027201 },
    ElectrodePosition { label: "FTT8", x: 0.083168, y: 0.000182, z: -0.010364 },
    ElectrodePosition { label: "FTT10", x: 0.085393, y: -0.000952, z: -0.049520 },
    ElectrodePosition { label: "TTP9", x: -0.086632, y: -0.031238, z: -0.047178 },
    ElectrodePosition { label: "TTP7", x: -0.085933, y: -0.031093, z: -0.008474 },
    ElectrodePosition { label: "CCP5", x: -0.081543, y: -0.030173, z: 0.030273 },
    ElectrodePosition { label: "CCP3", x: -0.066128, y: -0.029296, z: 0.065898 },
    ElectrodePosition { label: "CCP1", x: -0.036930, y: -0.028570, z: 0.091734 },
    ElectrodePosition { label: "CCPz", x: 0.000396, y: -0.028163, z: 0.101269 },
    ElectrodePosition { label: "CCP2", x: 0.038540, y: -0.028225, z: 0.090976 },
    ElectrodePosition { label: "CCP4", x: 0.068854, y: -0.028640, z: 0.066410 },
    ElectrodePosition { label: "CCP6", x: 0.084553, y: -0.029378, z: 0.030878 },
    ElectrodePosition { label: "TTP8", x: 0.086000, y: -0.030280, z: -0.008435 },
    ElectrodePosition { label: "TTP10", x: 0.086762, y: -0.031731, z: -0.047253 },
    ElectrodePosition { label: "TPP9", x: -0.080715, y: -0.060646, z: -0.043594 },
    ElectrodePosition { label: "TPP7", x: -0.078599, y: -0.059724, z: -0.004758 },
    ElectrodePosition { label: "CPP5", x: -0.073664, y: -0.061923, z: 0.030380 },
    ElectrodePosition { label: "CPP3", x: -0.059411, y: -0.063925, z: 0.062672 },
    ElectrodePosition { label: "CPP1", x: -0.032728, y: -0.065320, z: 0.085944 },
    ElectrodePosition { label: "CPPz", x: 0.000366, y: -0.065750, z: 0.094058 },
    ElectrodePosition { label: "CPP2", x: 0.035892, y: -0.065138, z: 0.085980 },
    ElectrodePosition { label: "CPP4", x: 0.062256, y: -0.063615, z: 0.062719 },
    ElectrodePosition { label: "CPP6", x: 0.076671, y: -0.061548, z: 0.030543 },
    ElectrodePosition { label: "TPP8", x: 0.079319, y: -0.059303, z: -0.004840 },
    ElectrodePosition { label: "TPP10", x: 0.081560, y: -0.061215, z: -0.043800 },
    ElectrodePosition { label: "PPO9", x: -0.064570, y: -0.086432, z: -0.038324 },
    ElectrodePosition { label: "PPO7", x: -0.064583, y: -0.086222, z: 0.000033 },
    ElectrodePosition { label: "PPO5", x: -0.058712, y: -0.088705, z: 0.025193 },
    ElectrodePosition { label: "PPO3", x: -0.046160, y: -0.090888, z: 0.047446 },
    ElectrodePosition { label: "PPO1", x: -0.024648, y: -0.092292, z: 0.062076 },
    ElectrodePosition { label: "PPOz", x: 0.000273, y: -0.092758, z: 0.067342 },
    ElectrodePosition { label: "PPO2", x: 0.026437, y: -0.092295, z: 0.063199 },
    ElectrodePosition { label: "PPO4", x: 0.047144, y: -0.090712, z: 0.047678 },
    ElectrodePosition { label: "PPO6", x: 0.060813, y: -0.088504, z: 0.025662 },
    ElectrodePosition { label: "PPO8", x: 0.065152, y: -0.085943, z: -0.000009 },
    ElectrodePosition { label: "PPO10", x: 0.065038, y: -0.086718, z: -0.038448 },
    ElectrodePosition { label: "POO9", x: -0.043128, y: -0.107516, z: -0.032387 },
    ElectrodePosition { label: "POO7", x: -0.042976, y: -0.106493, z: 0.005773 },
    ElectrodePosition { label: "POO5", x: -0.036234, y: -0.107716, z: 0.017750 },
    ElectrodePosition { label: "POO3", x: -0.025984, y: -0.108616, z: 0.026544 },
    ElectrodePosition { label: "POO1", x: -0.013664, y: -0.109266, z: 0.032856 },
    ElectrodePosition { label: "POOz", x: 0.000168, y: -0.109276, z: 0.032790 },
    ElectrodePosition { label: "POO2", x: 0.013651, y: -0.109106, z: 0.030936 },
    ElectrodePosition { label: "POO4", x: 0.026664, y: -0.108668, z: 0.026415 },
    ElectrodePosition { label: "POO6", x: 0.037701, y: -0.107840, z: 0.018069 },
    ElectrodePosition { label: "POO8", x: 0.043670, y: -0.106599, z: 0.005726 },
    ElectrodePosition { label: "POO10", x: 0.043177, y: -0.107444, z: -0.032463 },
    ElectrodePosition { label: "OI1", x: -0.029391, y: -0.114511, z: -0.010020 },
    ElectrodePosition { label: "OIz", x: 0.000052, y: -0.119343, z: -0.003936 },
    ElectrodePosition { label: "OI2", x: 0.029553, y: -0.113636, z: -0.010051 },
];

/// All 334 electrode positions from the standard **10-05** system
/// (superset of 10-10; adds half-step `h`-suffix positions at 5% intervals).
pub static MONTAGE_1005: &[ElectrodePosition] = &[
    ElectrodePosition { label: "Fp1", x: -0.029437, y: 0.083917, z: -0.006990 },
    ElectrodePosition { label: "Fpz", x: 0.000112, y: 0.088247, z: -0.001713 },
    ElectrodePosition { label: "Fp2", x: 0.029872, y: 0.084896, z: -0.007080 },
    ElectrodePosition { label: "AF9", x: -0.048971, y: 0.064087, z: -0.047683 },
    ElectrodePosition { label: "AF7", x: -0.054840, y: 0.068572, z: -0.010590 },
    ElectrodePosition { label: "AF5", x: -0.045431, y: 0.072862, z: 0.005978 },
    ElectrodePosition { label: "AF3", x: -0.033701, y: 0.076837, z: 0.021227 },
    ElectrodePosition { label: "AF1", x: -0.018472, y: 0.079904, z: 0.032752 },
    ElectrodePosition { label: "AFz", x: 0.000231, y: 0.080771, z: 0.035417 },
    ElectrodePosition { label: "AF2", x: 0.019820, y: 0.080302, z: 0.032764 },
    ElectrodePosition { label: "AF4", x: 0.035712, y: 0.077726, z: 0.021956 },
    ElectrodePosition { label: "AF6", x: 0.046584, y: 0.073808, z: 0.006034 },
    ElectrodePosition { label: "AF8", x: 0.055743, y: 0.069657, z: -0.010755 },
    ElectrodePosition { label: "AF10", x: 0.050435, y: 0.063870, z: -0.048005 },
    ElectrodePosition { label: "F9", x: -0.070102, y: 0.041652, z: -0.049952 },
    ElectrodePosition { label: "F7", x: -0.070263, y: 0.042474, z: -0.011420 },
    ElectrodePosition { label: "F5", x: -0.064466, y: 0.048035, z: 0.016921 },
    ElectrodePosition { label: "F3", x: -0.050244, y: 0.053111, z: 0.042192 },
    ElectrodePosition { label: "F1", x: -0.027496, y: 0.056931, z: 0.060342 },
    ElectrodePosition { label: "Fz", x: 0.000312, y: 0.058512, z: 0.066462 },
    ElectrodePosition { label: "F2", x: 0.029514, y: 0.057602, z: 0.059540 },
    ElectrodePosition { label: "F4", x: 0.051836, y: 0.054305, z: 0.040814 },
    ElectrodePosition { label: "F6", x: 0.067914, y: 0.049830, z: 0.016367 },
    ElectrodePosition { label: "F8", x: 0.073043, y: 0.044422, z: -0.012000 },
    ElectrodePosition { label: "F10", x: 0.072114, y: 0.042067, z: -0.050452 },
    ElectrodePosition { label: "FT9", x: -0.084076, y: 0.014567, z: -0.050429 },
    ElectrodePosition { label: "FT7", x: -0.080775, y: 0.014120, z: -0.011135 },
    ElectrodePosition { label: "FC5", x: -0.077215, y: 0.018643, z: 0.024460 },
    ElectrodePosition { label: "FC3", x: -0.060182, y: 0.022716, z: 0.055544 },
    ElectrodePosition { label: "FC1", x: -0.034062, y: 0.026011, z: 0.079987 },
    ElectrodePosition { label: "FCz", x: 0.000376, y: 0.027390, z: 0.088668 },
    ElectrodePosition { label: "FC2", x: 0.034784, y: 0.026438, z: 0.078808 },
    ElectrodePosition { label: "FC4", x: 0.062293, y: 0.023723, z: 0.055630 },
    ElectrodePosition { label: "FC6", x: 0.079534, y: 0.019936, z: 0.024438 },
    ElectrodePosition { label: "FT8", x: 0.081815, y: 0.015417, z: -0.011330 },
    ElectrodePosition { label: "FT10", x: 0.084113, y: 0.014365, z: -0.050538 },
    ElectrodePosition { label: "T9", x: -0.085894, y: -0.015829, z: -0.048283 },
    ElectrodePosition { label: "T7", x: -0.084161, y: -0.016019, z: -0.009346 },
    ElectrodePosition { label: "C5", x: -0.080280, y: -0.013760, z: 0.029160 },
    ElectrodePosition { label: "C3", x: -0.065358, y: -0.011632, z: 0.064358 },
    ElectrodePosition { label: "C1", x: -0.036158, y: -0.009984, z: 0.089752 },
    ElectrodePosition { label: "Cz", x: 0.000401, y: -0.009167, z: 0.100244 },
    ElectrodePosition { label: "C2", x: 0.037672, y: -0.009624, z: 0.088412 },
    ElectrodePosition { label: "C4", x: 0.067118, y: -0.010900, z: 0.063580 },
    ElectrodePosition { label: "C6", x: 0.083456, y: -0.012776, z: 0.029208 },
    ElectrodePosition { label: "T8", x: 0.085080, y: -0.015020, z: -0.009490 },
    ElectrodePosition { label: "T10", x: 0.085560, y: -0.016361, z: -0.048271 },
    ElectrodePosition { label: "TP9", x: -0.085619, y: -0.046515, z: -0.045707 },
    ElectrodePosition { label: "TP7", x: -0.084830, y: -0.046022, z: -0.007056 },
    ElectrodePosition { label: "CP5", x: -0.079592, y: -0.046551, z: 0.030949 },
    ElectrodePosition { label: "CP3", x: -0.063556, y: -0.047009, z: 0.065624 },
    ElectrodePosition { label: "CP1", x: -0.035513, y: -0.047292, z: 0.091315 },
    ElectrodePosition { label: "CPz", x: 0.000386, y: -0.047318, z: 0.099432 },
    ElectrodePosition { label: "CP2", x: 0.038384, y: -0.047073, z: 0.090695 },
    ElectrodePosition { label: "CP4", x: 0.066612, y: -0.046637, z: 0.065580 },
    ElectrodePosition { label: "CP6", x: 0.083322, y: -0.046101, z: 0.031206 },
    ElectrodePosition { label: "TP8", x: 0.085549, y: -0.045545, z: -0.007130 },
    ElectrodePosition { label: "TP10", x: 0.086162, y: -0.047035, z: -0.045869 },
    ElectrodePosition { label: "P9", x: -0.073009, y: -0.073766, z: -0.040998 },
    ElectrodePosition { label: "P7", x: -0.072434, y: -0.073453, z: -0.002487 },
    ElectrodePosition { label: "P5", x: -0.067272, y: -0.076291, z: 0.028382 },
    ElectrodePosition { label: "P3", x: -0.053007, y: -0.078788, z: 0.055940 },
    ElectrodePosition { label: "P1", x: -0.028620, y: -0.080525, z: 0.075436 },
    ElectrodePosition { label: "Pz", x: 0.000325, y: -0.081115, z: 0.082615 },
    ElectrodePosition { label: "P2", x: 0.031920, y: -0.080487, z: 0.076716 },
    ElectrodePosition { label: "P4", x: 0.055667, y: -0.078560, z: 0.056561 },
    ElectrodePosition { label: "P6", x: 0.067888, y: -0.075904, z: 0.028091 },
    ElectrodePosition { label: "P8", x: 0.073056, y: -0.073068, z: -0.002540 },
    ElectrodePosition { label: "P10", x: 0.073895, y: -0.074390, z: -0.041220 },
    ElectrodePosition { label: "PO9", x: -0.054910, y: -0.098045, z: -0.035465 },
    ElectrodePosition { label: "PO7", x: -0.054840, y: -0.097528, z: 0.002792 },
    ElectrodePosition { label: "PO5", x: -0.048424, y: -0.099341, z: 0.021599 },
    ElectrodePosition { label: "PO3", x: -0.036511, y: -0.100853, z: 0.037167 },
    ElectrodePosition { label: "PO1", x: -0.018972, y: -0.101768, z: 0.046536 },
    ElectrodePosition { label: "POz", x: 0.000216, y: -0.102178, z: 0.050608 },
    ElectrodePosition { label: "PO2", x: 0.019878, y: -0.101793, z: 0.046393 },
    ElectrodePosition { label: "PO4", x: 0.036782, y: -0.100849, z: 0.036397 },
    ElectrodePosition { label: "PO6", x: 0.049820, y: -0.099446, z: 0.021727 },
    ElectrodePosition { label: "PO8", x: 0.055667, y: -0.097625, z: 0.002730 },
    ElectrodePosition { label: "PO10", x: 0.054988, y: -0.098091, z: -0.035541 },
    ElectrodePosition { label: "O1", x: -0.029413, y: -0.112449, z: 0.008839 },
    ElectrodePosition { label: "Oz", x: 0.000108, y: -0.114892, z: 0.014657 },
    ElectrodePosition { label: "O2", x: 0.029843, y: -0.112156, z: 0.008800 },
    ElectrodePosition { label: "I1", x: -0.029818, y: -0.114570, z: -0.029216 },
    ElectrodePosition { label: "I2", x: 0.029742, y: -0.114260, z: -0.029256 },
    ElectrodePosition { label: "AFp9h", x: -0.043290, y: 0.075855, z: -0.028244 },
    ElectrodePosition { label: "AFp7h", x: -0.038552, y: 0.079953, z: -0.004995 },
    ElectrodePosition { label: "AFp5h", x: -0.027986, y: 0.082459, z: 0.002702 },
    ElectrodePosition { label: "AFp3h", x: -0.017195, y: 0.084849, z: 0.010027 },
    ElectrodePosition { label: "AFp1h", x: -0.005932, y: 0.086878, z: 0.016200 },
    ElectrodePosition { label: "AFp2h", x: 0.007105, y: 0.087074, z: 0.016469 },
    ElectrodePosition { label: "AFp4h", x: 0.018923, y: 0.085597, z: 0.011443 },
    ElectrodePosition { label: "AFp6h", x: 0.028644, y: 0.082976, z: 0.002828 },
    ElectrodePosition { label: "AFp8h", x: 0.039320, y: 0.080687, z: -0.004725 },
    ElectrodePosition { label: "AFp10h", x: 0.043822, y: 0.076542, z: -0.028307 },
    ElectrodePosition { label: "AFF9h", x: -0.063254, y: 0.053857, z: -0.030316 },
    ElectrodePosition { label: "AFF7h", x: -0.061351, y: 0.058799, z: 0.000897 },
    ElectrodePosition { label: "AFF5h", x: -0.050800, y: 0.064041, z: 0.023089 },
    ElectrodePosition { label: "AFF3h", x: -0.034316, y: 0.068393, z: 0.041188 },
    ElectrodePosition { label: "AFF1h", x: -0.011436, y: 0.070756, z: 0.050348 },
    ElectrodePosition { label: "AFF2h", x: 0.013479, y: 0.071201, z: 0.051175 },
    ElectrodePosition { label: "AFF4h", x: 0.036183, y: 0.069151, z: 0.041254 },
    ElectrodePosition { label: "AFF6h", x: 0.052397, y: 0.065071, z: 0.022862 },
    ElectrodePosition { label: "AFF8h", x: 0.062915, y: 0.060045, z: 0.000630 },
    ElectrodePosition { label: "AFF10h", x: 0.064334, y: 0.054600, z: -0.030444 },
    ElectrodePosition { label: "FFT9h", x: -0.079067, y: 0.028081, z: -0.031253 },
    ElectrodePosition { label: "FFT7h", x: -0.074500, y: 0.031300, z: 0.004846 },
    ElectrodePosition { label: "FFC5h", x: -0.065238, y: 0.036428, z: 0.036144 },
    ElectrodePosition { label: "FFC3h", x: -0.044410, y: 0.040762, z: 0.061690 },
    ElectrodePosition { label: "FFC1h", x: -0.015424, y: 0.043660, z: 0.077682 },
    ElectrodePosition { label: "FFC2h", x: 0.017592, y: 0.044054, z: 0.077788 },
    ElectrodePosition { label: "FFC4h", x: 0.045853, y: 0.041623, z: 0.060647 },
    ElectrodePosition { label: "FFC6h", x: 0.067128, y: 0.037800, z: 0.035296 },
    ElectrodePosition { label: "FFT8h", x: 0.078053, y: 0.032982, z: 0.004483 },
    ElectrodePosition { label: "FFT10h", x: 0.080097, y: 0.028514, z: -0.031338 },
    ElectrodePosition { label: "FTT9h", x: -0.084125, y: -0.001847, z: -0.029794 },
    ElectrodePosition { label: "FTT7h", x: -0.082355, y: 0.000826, z: 0.008579 },
    ElectrodePosition { label: "FCC5h", x: -0.074692, y: 0.004303, z: 0.045307 },
    ElectrodePosition { label: "FCC3h", x: -0.051051, y: 0.007177, z: 0.074377 },
    ElectrodePosition { label: "FCC1h", x: -0.018219, y: 0.009094, z: 0.092529 },
    ElectrodePosition { label: "FCC2h", x: 0.018787, y: 0.009248, z: 0.091562 },
    ElectrodePosition { label: "FCC4h", x: 0.051885, y: 0.007798, z: 0.073507 },
    ElectrodePosition { label: "FCC6h", x: 0.077002, y: 0.005336, z: 0.045350 },
    ElectrodePosition { label: "FTT8h", x: 0.083888, y: 0.001946, z: 0.008501 },
    ElectrodePosition { label: "FTT10h", x: 0.084123, y: -0.001808, z: -0.029638 },
    ElectrodePosition { label: "TTP9h", x: -0.086973, y: -0.032216, z: -0.027848 },
    ElectrodePosition { label: "TTP7h", x: -0.085565, y: -0.030629, z: 0.011153 },
    ElectrodePosition { label: "CCP5h", x: -0.076407, y: -0.029731, z: 0.049217 },
    ElectrodePosition { label: "CCP3h", x: -0.052928, y: -0.028906, z: 0.080304 },
    ElectrodePosition { label: "CCP1h", x: -0.018354, y: -0.028322, z: 0.098220 },
    ElectrodePosition { label: "CCP2h", x: 0.020220, y: -0.028148, z: 0.098172 },
    ElectrodePosition { label: "CCP4h", x: 0.055114, y: -0.028386, z: 0.080474 },
    ElectrodePosition { label: "CCP6h", x: 0.079006, y: -0.028986, z: 0.049628 },
    ElectrodePosition { label: "TTP8h", x: 0.086000, y: -0.029820, z: 0.011248 },
    ElectrodePosition { label: "TTP10h", x: 0.088625, y: -0.032272, z: -0.028000 },
    ElectrodePosition { label: "TPP9h", x: -0.078160, y: -0.060757, z: -0.023824 },
    ElectrodePosition { label: "TPP7h", x: -0.076680, y: -0.060832, z: 0.012880 },
    ElectrodePosition { label: "CPP5h", x: -0.068115, y: -0.062975, z: 0.047252 },
    ElectrodePosition { label: "CPP3h", x: -0.046914, y: -0.064691, z: 0.075296 },
    ElectrodePosition { label: "CPP1h", x: -0.015820, y: -0.065600, z: 0.091164 },
    ElectrodePosition { label: "CPP2h", x: 0.019420, y: -0.065595, z: 0.092405 },
    ElectrodePosition { label: "CPP4h", x: 0.050674, y: -0.064482, z: 0.076130 },
    ElectrodePosition { label: "CPP6h", x: 0.071096, y: -0.062624, z: 0.047328 },
    ElectrodePosition { label: "TPP8h", x: 0.078520, y: -0.060432, z: 0.012902 },
    ElectrodePosition { label: "TPP10h", x: 0.078903, y: -0.060955, z: -0.023805 },
    ElectrodePosition { label: "PPO9h", x: -0.064597, y: -0.087656, z: -0.019014 },
    ElectrodePosition { label: "PPO7h", x: -0.062959, y: -0.087503, z: 0.012952 },
    ElectrodePosition { label: "PPO5h", x: -0.054010, y: -0.089899, z: 0.037332 },
    ElectrodePosition { label: "PPO3h", x: -0.035887, y: -0.091667, z: 0.055504 },
    ElectrodePosition { label: "PPO1h", x: -0.012047, y: -0.092607, z: 0.065508 },
    ElectrodePosition { label: "PPO2h", x: 0.013923, y: -0.092694, z: 0.066958 },
    ElectrodePosition { label: "PPO4h", x: 0.037799, y: -0.091629, z: 0.056733 },
    ElectrodePosition { label: "PPO6h", x: 0.054609, y: -0.089640, z: 0.037035 },
    ElectrodePosition { label: "PPO8h", x: 0.063112, y: -0.087228, z: 0.012856 },
    ElectrodePosition { label: "PPO10h", x: 0.065014, y: -0.087806, z: -0.018952 },
    ElectrodePosition { label: "POO9h", x: -0.042862, y: -0.108073, z: -0.013151 },
    ElectrodePosition { label: "POO7h", x: -0.040120, y: -0.107129, z: 0.012061 },
    ElectrodePosition { label: "POO5h", x: -0.031951, y: -0.108252, z: 0.023047 },
    ElectrodePosition { label: "POO3h", x: -0.019862, y: -0.108942, z: 0.029760 },
    ElectrodePosition { label: "POO1h", x: -0.006919, y: -0.109260, z: 0.032710 },
    ElectrodePosition { label: "POO2h", x: 0.006804, y: -0.109163, z: 0.031582 },
    ElectrodePosition { label: "POO4h", x: 0.020294, y: -0.108914, z: 0.028944 },
    ElectrodePosition { label: "POO6h", x: 0.032176, y: -0.108252, z: 0.022255 },
    ElectrodePosition { label: "POO8h", x: 0.041098, y: -0.107245, z: 0.012138 },
    ElectrodePosition { label: "POO10h", x: 0.043895, y: -0.109127, z: -0.013170 },
    ElectrodePosition { label: "OI1h", x: -0.014850, y: -0.117987, z: -0.006920 },
    ElectrodePosition { label: "OI2h", x: 0.015095, y: -0.118018, z: -0.006933 },
    ElectrodePosition { label: "Fp1h", x: -0.014811, y: 0.087235, z: -0.004477 },
    ElectrodePosition { label: "Fp2h", x: 0.015162, y: 0.088091, z: -0.004551 },
    ElectrodePosition { label: "AF9h", x: -0.054830, y: 0.066413, z: -0.029704 },
    ElectrodePosition { label: "AF7h", x: -0.051176, y: 0.070836, z: -0.001755 },
    ElectrodePosition { label: "AF5h", x: -0.039641, y: 0.074867, z: 0.013678 },
    ElectrodePosition { label: "AF3h", x: -0.027219, y: 0.078709, z: 0.028375 },
    ElectrodePosition { label: "AF1h", x: -0.009198, y: 0.080605, z: 0.035133 },
    ElectrodePosition { label: "AF2h", x: 0.010482, y: 0.080865, z: 0.035359 },
    ElectrodePosition { label: "AF4h", x: 0.028580, y: 0.079303, z: 0.028470 },
    ElectrodePosition { label: "AF6h", x: 0.040940, y: 0.075740, z: 0.013860 },
    ElectrodePosition { label: "AF8h", x: 0.052029, y: 0.071847, z: -0.001920 },
    ElectrodePosition { label: "AF10h", x: 0.055754, y: 0.067170, z: -0.029824 },
    ElectrodePosition { label: "F9h", x: -0.071508, y: 0.041119, z: -0.030854 },
    ElectrodePosition { label: "F7h", x: -0.068556, y: 0.045284, z: 0.003002 },
    ElectrodePosition { label: "F5h", x: -0.058488, y: 0.050672, z: 0.030192 },
    ElectrodePosition { label: "F3h", x: -0.039980, y: 0.055260, z: 0.052600 },
    ElectrodePosition { label: "F1h", x: -0.013384, y: 0.057902, z: 0.064332 },
    ElectrodePosition { label: "F2h", x: 0.015834, y: 0.058456, z: 0.064992 },
    ElectrodePosition { label: "F4h", x: 0.041794, y: 0.056226, z: 0.051499 },
    ElectrodePosition { label: "F6h", x: 0.060052, y: 0.052086, z: 0.028708 },
    ElectrodePosition { label: "F8h", x: 0.071959, y: 0.047192, z: 0.002475 },
    ElectrodePosition { label: "F10h", x: 0.072798, y: 0.041822, z: -0.031026 },
    ElectrodePosition { label: "FT9h", x: -0.082956, y: 0.013320, z: -0.030808 },
    ElectrodePosition { label: "FT7h", x: -0.080114, y: 0.016390, z: 0.006850 },
    ElectrodePosition { label: "FC5h", x: -0.071210, y: 0.020820, z: 0.041324 },
    ElectrodePosition { label: "FC3h", x: -0.048512, y: 0.024529, z: 0.069136 },
    ElectrodePosition { label: "FC1h", x: -0.017344, y: 0.027024, z: 0.086923 },
    ElectrodePosition { label: "FC2h", x: 0.018418, y: 0.027271, z: 0.086437 },
    ElectrodePosition { label: "FC4h", x: 0.049548, y: 0.025238, z: 0.068430 },
    ElectrodePosition { label: "FC6h", x: 0.073219, y: 0.022007, z: 0.041297 },
    ElectrodePosition { label: "FT8h", x: 0.081580, y: 0.017684, z: 0.006564 },
    ElectrodePosition { label: "FT10h", x: 0.083371, y: 0.013548, z: -0.030749 },
    ElectrodePosition { label: "T9h", x: -0.085132, y: -0.017056, z: -0.028731 },
    ElectrodePosition { label: "T7h", x: -0.082946, y: -0.014883, z: 0.010009 },
    ElectrodePosition { label: "C5h", x: -0.075294, y: -0.012640, z: 0.047904 },
    ElectrodePosition { label: "C3h", x: -0.051581, y: -0.010755, z: 0.078035 },
    ElectrodePosition { label: "C1h", x: -0.018279, y: -0.009432, z: 0.097356 },
    ElectrodePosition { label: "C2h", x: 0.019678, y: -0.009304, z: 0.095706 },
    ElectrodePosition { label: "C4h", x: 0.053806, y: -0.010144, z: 0.077730 },
    ElectrodePosition { label: "C6h", x: 0.078125, y: -0.011735, z: 0.047840 },
    ElectrodePosition { label: "T8h", x: 0.085137, y: -0.013906, z: 0.009890 },
    ElectrodePosition { label: "T10h", x: 0.086100, y: -0.017088, z: -0.028756 },
    ElectrodePosition { label: "TP9h", x: -0.084810, y: -0.047246, z: -0.026220 },
    ElectrodePosition { label: "TP7h", x: -0.082704, y: -0.046298, z: 0.011974 },
    ElectrodePosition { label: "CP5h", x: -0.073301, y: -0.046792, z: 0.049109 },
    ElectrodePosition { label: "CP3h", x: -0.051049, y: -0.047176, z: 0.080016 },
    ElectrodePosition { label: "CP1h", x: -0.017354, y: -0.047342, z: 0.097410 },
    ElectrodePosition { label: "CP2h", x: 0.020680, y: -0.047232, z: 0.098072 },
    ElectrodePosition { label: "CP4h", x: 0.053997, y: -0.046890, z: 0.080077 },
    ElectrodePosition { label: "CP6h", x: 0.076550, y: -0.046373, z: 0.049140 },
    ElectrodePosition { label: "TP8h", x: 0.085200, y: -0.045807, z: 0.012102 },
    ElectrodePosition { label: "TP10h", x: 0.085443, y: -0.047221, z: -0.026176 },
    ElectrodePosition { label: "P9h", x: -0.072177, y: -0.074628, z: -0.021536 },
    ElectrodePosition { label: "P7h", x: -0.070113, y: -0.074868, z: 0.012999 },
    ElectrodePosition { label: "P5h", x: -0.061728, y: -0.077624, z: 0.043028 },
    ElectrodePosition { label: "P3h", x: -0.041673, y: -0.079753, z: 0.066715 },
    ElectrodePosition { label: "P1h", x: -0.013961, y: -0.081003, z: 0.081003 },
    ElectrodePosition { label: "P2h", x: 0.017298, y: -0.080981, z: 0.081641 },
    ElectrodePosition { label: "P4h", x: 0.044748, y: -0.079611, z: 0.067655 },
    ElectrodePosition { label: "P6h", x: 0.063627, y: -0.077302, z: 0.043119 },
    ElectrodePosition { label: "P8h", x: 0.072104, y: -0.074499, z: 0.013025 },
    ElectrodePosition { label: "P10h", x: 0.073282, y: -0.075077, z: -0.021576 },
    ElectrodePosition { label: "PO9h", x: -0.054775, y: -0.098977, z: -0.016193 },
    ElectrodePosition { label: "PO7h", x: -0.051928, y: -0.098444, z: 0.012304 },
    ElectrodePosition { label: "PO5h", x: -0.043342, y: -0.100163, z: 0.030009 },
    ElectrodePosition { label: "PO3h", x: -0.028007, y: -0.101361, z: 0.042379 },
    ElectrodePosition { label: "PO1h", x: -0.009503, y: -0.102060, z: 0.049418 },
    ElectrodePosition { label: "PO2h", x: 0.010236, y: -0.102029, z: 0.048942 },
    ElectrodePosition { label: "PO4h", x: 0.028648, y: -0.101390, z: 0.042138 },
    ElectrodePosition { label: "PO6h", x: 0.044221, y: -0.100219, z: 0.029808 },
    ElectrodePosition { label: "PO8h", x: 0.052839, y: -0.098536, z: 0.012250 },
    ElectrodePosition { label: "PO10h", x: 0.055860, y: -0.099894, z: -0.016208 },
    ElectrodePosition { label: "O1h", x: -0.014805, y: -0.115100, z: 0.011829 },
    ElectrodePosition { label: "O2h", x: 0.015146, y: -0.115191, z: 0.011833 },
    ElectrodePosition { label: "I1h", x: -0.015158, y: -0.118242, z: -0.026048 },
    ElectrodePosition { label: "I2h", x: 0.015129, y: -0.118151, z: -0.026081 },
    ElectrodePosition { label: "AFp9", x: -0.036125, y: 0.072380, z: -0.045852 },
    ElectrodePosition { label: "AFp7", x: -0.043512, y: 0.078580, z: -0.009240 },
    ElectrodePosition { label: "AFp5", x: -0.033285, y: 0.081207, z: -0.001140 },
    ElectrodePosition { label: "AFp3", x: -0.022352, y: 0.083562, z: 0.006071 },
    ElectrodePosition { label: "AFp1", x: -0.012242, y: 0.086194, z: 0.014188 },
    ElectrodePosition { label: "AFpz", x: 0.000170, y: 0.087322, z: 0.017442 },
    ElectrodePosition { label: "AFp2", x: 0.013622, y: 0.086758, z: 0.015302 },
    ElectrodePosition { label: "AFp4", x: 0.024101, y: 0.084377, z: 0.007433 },
    ElectrodePosition { label: "AFp6", x: 0.033913, y: 0.081812, z: -0.001035 },
    ElectrodePosition { label: "AFp8", x: 0.043948, y: 0.079296, z: -0.009300 },
    ElectrodePosition { label: "AFp10", x: 0.037712, y: 0.072168, z: -0.046197 },
    ElectrodePosition { label: "AFF9", x: -0.059340, y: 0.052680, z: -0.048770 },
    ElectrodePosition { label: "AFF7", x: -0.063262, y: 0.055992, z: -0.011173 },
    ElectrodePosition { label: "AFF5", x: -0.055820, y: 0.061396, z: 0.011884 },
    ElectrodePosition { label: "AFF3", x: -0.043382, y: 0.066367, z: 0.032811 },
    ElectrodePosition { label: "AFF1", x: -0.023582, y: 0.069917, z: 0.047293 },
    ElectrodePosition { label: "AFFz", x: 0.000276, y: 0.071280, z: 0.052092 },
    ElectrodePosition { label: "AFF2", x: 0.025558, y: 0.070556, z: 0.047827 },
    ElectrodePosition { label: "AFF4", x: 0.045152, y: 0.067275, z: 0.032731 },
    ElectrodePosition { label: "AFF6", x: 0.058000, y: 0.062600, z: 0.011900 },
    ElectrodePosition { label: "AFF8", x: 0.064673, y: 0.057274, z: -0.011460 },
    ElectrodePosition { label: "AFF10", x: 0.060601, y: 0.052267, z: -0.049038 },
    ElectrodePosition { label: "FFT9", x: -0.078484, y: 0.028770, z: -0.050522 },
    ElectrodePosition { label: "FFT7", x: -0.076615, y: 0.028653, z: -0.011508 },
    ElectrodePosition { label: "FFC5", x: -0.071506, y: 0.033926, z: 0.020993 },
    ElectrodePosition { label: "FFC3", x: -0.055940, y: 0.038716, z: 0.049788 },
    ElectrodePosition { label: "FFC1", x: -0.030655, y: 0.042415, z: 0.071040 },
    ElectrodePosition { label: "FFCz", x: 0.000351, y: 0.044074, z: 0.079141 },
    ElectrodePosition { label: "FFC2", x: 0.032645, y: 0.043101, z: 0.070795 },
    ElectrodePosition { label: "FFC4", x: 0.057504, y: 0.039852, z: 0.048811 },
    ElectrodePosition { label: "FFC6", x: 0.074250, y: 0.035500, z: 0.020380 },
    ElectrodePosition { label: "FFT8", x: 0.079034, y: 0.030344, z: -0.011997 },
    ElectrodePosition { label: "FFT10", x: 0.079920, y: 0.028942, z: -0.050914 },
    ElectrodePosition { label: "FTT9", x: -0.087362, y: -0.000515, z: -0.049837 },
    ElectrodePosition { label: "FTT7", x: -0.082668, y: -0.000942, z: -0.010284 },
    ElectrodePosition { label: "FCC5", x: -0.080133, y: 0.002585, z: 0.027312 },
    ElectrodePosition { label: "FCC3", x: -0.064161, y: 0.005831, z: 0.060885 },
    ElectrodePosition { label: "FCC1", x: -0.035749, y: 0.008309, z: 0.085459 },
    ElectrodePosition { label: "FCCz", x: 0.000391, y: 0.009508, z: 0.095560 },
    ElectrodePosition { label: "FCC2", x: 0.036070, y: 0.008652, z: 0.083832 },
    ElectrodePosition { label: "FCC4", x: 0.065164, y: 0.006620, z: 0.060052 },
    ElectrodePosition { label: "FCC6", x: 0.081544, y: 0.003664, z: 0.027201 },
    ElectrodePosition { label: "FTT8", x: 0.083168, y: 0.000182, z: -0.010364 },
    ElectrodePosition { label: "FTT10", x: 0.085393, y: -0.000952, z: -0.049520 },
    ElectrodePosition { label: "TTP9", x: -0.086632, y: -0.031238, z: -0.047178 },
    ElectrodePosition { label: "TTP7", x: -0.085933, y: -0.031093, z: -0.008474 },
    ElectrodePosition { label: "CCP5", x: -0.081543, y: -0.030173, z: 0.030273 },
    ElectrodePosition { label: "CCP3", x: -0.066128, y: -0.029296, z: 0.065898 },
    ElectrodePosition { label: "CCP1", x: -0.036930, y: -0.028570, z: 0.091734 },
    ElectrodePosition { label: "CCPz", x: 0.000396, y: -0.028163, z: 0.101269 },
    ElectrodePosition { label: "CCP2", x: 0.038540, y: -0.028225, z: 0.090976 },
    ElectrodePosition { label: "CCP4", x: 0.068854, y: -0.028640, z: 0.066410 },
    ElectrodePosition { label: "CCP6", x: 0.084553, y: -0.029378, z: 0.030878 },
    ElectrodePosition { label: "TTP8", x: 0.086000, y: -0.030280, z: -0.008435 },
    ElectrodePosition { label: "TTP10", x: 0.086762, y: -0.031731, z: -0.047253 },
    ElectrodePosition { label: "TPP9", x: -0.080715, y: -0.060646, z: -0.043594 },
    ElectrodePosition { label: "TPP7", x: -0.078599, y: -0.059724, z: -0.004758 },
    ElectrodePosition { label: "CPP5", x: -0.073664, y: -0.061923, z: 0.030380 },
    ElectrodePosition { label: "CPP3", x: -0.059411, y: -0.063925, z: 0.062672 },
    ElectrodePosition { label: "CPP1", x: -0.032728, y: -0.065320, z: 0.085944 },
    ElectrodePosition { label: "CPPz", x: 0.000366, y: -0.065750, z: 0.094058 },
    ElectrodePosition { label: "CPP2", x: 0.035892, y: -0.065138, z: 0.085980 },
    ElectrodePosition { label: "CPP4", x: 0.062256, y: -0.063615, z: 0.062719 },
    ElectrodePosition { label: "CPP6", x: 0.076671, y: -0.061548, z: 0.030543 },
    ElectrodePosition { label: "TPP8", x: 0.079319, y: -0.059303, z: -0.004840 },
    ElectrodePosition { label: "TPP10", x: 0.081560, y: -0.061215, z: -0.043800 },
    ElectrodePosition { label: "PPO9", x: -0.064570, y: -0.086432, z: -0.038324 },
    ElectrodePosition { label: "PPO7", x: -0.064583, y: -0.086222, z: 0.000033 },
    ElectrodePosition { label: "PPO5", x: -0.058712, y: -0.088705, z: 0.025193 },
    ElectrodePosition { label: "PPO3", x: -0.046160, y: -0.090888, z: 0.047446 },
    ElectrodePosition { label: "PPO1", x: -0.024648, y: -0.092292, z: 0.062076 },
    ElectrodePosition { label: "PPOz", x: 0.000273, y: -0.092758, z: 0.067342 },
    ElectrodePosition { label: "PPO2", x: 0.026437, y: -0.092295, z: 0.063199 },
    ElectrodePosition { label: "PPO4", x: 0.047144, y: -0.090712, z: 0.047678 },
    ElectrodePosition { label: "PPO6", x: 0.060813, y: -0.088504, z: 0.025662 },
    ElectrodePosition { label: "PPO8", x: 0.065152, y: -0.085943, z: -0.000009 },
    ElectrodePosition { label: "PPO10", x: 0.065038, y: -0.086718, z: -0.038448 },
    ElectrodePosition { label: "POO9", x: -0.043128, y: -0.107516, z: -0.032387 },
    ElectrodePosition { label: "POO7", x: -0.042976, y: -0.106493, z: 0.005773 },
    ElectrodePosition { label: "POO5", x: -0.036234, y: -0.107716, z: 0.017750 },
    ElectrodePosition { label: "POO3", x: -0.025984, y: -0.108616, z: 0.026544 },
    ElectrodePosition { label: "POO1", x: -0.013664, y: -0.109266, z: 0.032856 },
    ElectrodePosition { label: "POOz", x: 0.000168, y: -0.109276, z: 0.032790 },
    ElectrodePosition { label: "POO2", x: 0.013651, y: -0.109106, z: 0.030936 },
    ElectrodePosition { label: "POO4", x: 0.026664, y: -0.108668, z: 0.026415 },
    ElectrodePosition { label: "POO6", x: 0.037701, y: -0.107840, z: 0.018069 },
    ElectrodePosition { label: "POO8", x: 0.043670, y: -0.106599, z: 0.005726 },
    ElectrodePosition { label: "POO10", x: 0.043177, y: -0.107444, z: -0.032463 },
    ElectrodePosition { label: "OI1", x: -0.029391, y: -0.114511, z: -0.010020 },
    ElectrodePosition { label: "OIz", x: 0.000052, y: -0.119343, z: -0.003936 },
    ElectrodePosition { label: "OI2", x: 0.029553, y: -0.113636, z: -0.010051 },
];

/// Return the 3-D head-surface position for a label from the full 10-05 montage
/// (334 sites), or `None` if the label is not recognised.
///
/// The lookup is case-insensitive and automatically resolves legacy aliases
/// (e.g. `"T3"` → `"T7"`, `"T5"` → `"P7"`, `"M1"` → `"TP9"`).
///
/// # Example
/// ```rust
/// use openbci::electrode::position;
/// let cz = position("Cz").unwrap();
/// assert!(cz.z > 0.09); // Cz is near the top of the head
/// assert!(position("Bogus").is_none());
/// ```
pub fn position(label: &str) -> Option<ElectrodePosition> {
    let canonical = resolve_alias(label);
    MONTAGE_1005.iter().find(|e| e.label.eq_ignore_ascii_case(canonical)).copied()
}

/// Return the 3-D position for a label restricted to the classic **10-20**
/// system (83 sites), or `None` if the label is not in the 10-20 set.
///
/// Useful for filtering layouts to the standard clinical subset.
pub fn position_1020(label: &str) -> Option<ElectrodePosition> {
    let canonical = resolve_alias(label);
    MONTAGE_1020.iter().find(|e| e.label.eq_ignore_ascii_case(canonical)).copied()
}

/// Return the 3-D position for a label restricted to the **10-10** system
/// (176 sites), or `None` if the label is not in the 10-10 set.
pub fn position_1010(label: &str) -> Option<ElectrodePosition> {
    let canonical = resolve_alias(label);
    MONTAGE_1010.iter().find(|e| e.label.eq_ignore_ascii_case(canonical)).copied()
}

/// Map legacy and alternative electrode names to their canonical 10-05 equivalents.
///
/// The older 10-20 nomenclature used temporal labels that differ from the
/// modern 10-05 standard.  This function performs a quick lookup so that
/// callers can freely mix old and new label conventions.
///
/// | Input | Canonical | Note |
/// |-------|-----------|------|
/// | `T3`  | `T7`      | Old left temporal |
/// | `T4`  | `T8`      | Old right temporal |
/// | `T5`  | `P7`      | Old left posterior temporal |
/// | `T6`  | `P8`      | Old right posterior temporal |
/// | `M1`  | `TP9`     | Left mastoid |
/// | `M2`  | `TP10`    | Right mastoid |
/// | `A1`  | `TP9`     | Left ear lobe |
/// | `A2`  | `TP10`    | Right ear lobe |
/// | `O9`  | `I1`      | Occipital extension left |
/// | `O10` | `I2`      | Occipital extension right |
///
/// Any unrecognised input is returned unchanged (pass-through).
pub fn resolve_alias(label: &str) -> &str {
    match label {
        "T3" | "t3" => "T7",
        "T4" | "t4" => "T8",
        "T5" | "t5" => "P7",
        "T6" | "t6" => "P8",
        "M1" | "m1" => "TP9",
        "M2" | "m2" => "TP10",
        "A1" | "a1" => "TP9",
        "A2" | "a2" => "TP10",
        "O9" | "o9" => "I1",
        "O10" | "o10" => "I2",
        other => other,
    }
}

/// Label constants for every electrode in the 10-05 system.
///
/// These are `&str` constants you can pass to [`ElectrodeLayout::from_labels`]
/// or [`position`].
///
/// # Naming conventions
/// - Odd numbers → left hemisphere (e.g. `C3`, `F7`)
/// - Even numbers → right hemisphere (e.g. `C4`, `F8`)
/// - `z` suffix → midline (e.g. `Cz`, `Fz`, `Pz`)
/// - `h` suffix → half-step 10-05 position (e.g. `FCC3h`)
pub mod positions {
    pub const FP1: &str = "Fp1";
    pub const FPZ: &str = "Fpz";
    pub const FP2: &str = "Fp2";
    pub const AF9: &str = "AF9";
    pub const AF7: &str = "AF7";
    pub const AF5: &str = "AF5";
    pub const AF3: &str = "AF3";
    pub const AF1: &str = "AF1";
    pub const AFZ: &str = "AFz";
    pub const AF2: &str = "AF2";
    pub const AF4: &str = "AF4";
    pub const AF6: &str = "AF6";
    pub const AF8: &str = "AF8";
    pub const AF10: &str = "AF10";
    pub const F9: &str = "F9";
    pub const F7: &str = "F7";
    pub const F5: &str = "F5";
    pub const F3: &str = "F3";
    pub const F1: &str = "F1";
    pub const FZ: &str = "Fz";
    pub const F2: &str = "F2";
    pub const F4: &str = "F4";
    pub const F6: &str = "F6";
    pub const F8: &str = "F8";
    pub const F10: &str = "F10";
    pub const FT9: &str = "FT9";
    pub const FT7: &str = "FT7";
    pub const FC5: &str = "FC5";
    pub const FC3: &str = "FC3";
    pub const FC1: &str = "FC1";
    pub const FCZ: &str = "FCz";
    pub const FC2: &str = "FC2";
    pub const FC4: &str = "FC4";
    pub const FC6: &str = "FC6";
    pub const FT8: &str = "FT8";
    pub const FT10: &str = "FT10";
    pub const T9: &str = "T9";
    pub const T7: &str = "T7";
    pub const C5: &str = "C5";
    pub const C3: &str = "C3";
    pub const C1: &str = "C1";
    pub const CZ: &str = "Cz";
    pub const C2: &str = "C2";
    pub const C4: &str = "C4";
    pub const C6: &str = "C6";
    pub const T8: &str = "T8";
    pub const T10: &str = "T10";
    pub const TP9: &str = "TP9";
    pub const TP7: &str = "TP7";
    pub const CP5: &str = "CP5";
    pub const CP3: &str = "CP3";
    pub const CP1: &str = "CP1";
    pub const CPZ: &str = "CPz";
    pub const CP2: &str = "CP2";
    pub const CP4: &str = "CP4";
    pub const CP6: &str = "CP6";
    pub const TP8: &str = "TP8";
    pub const TP10: &str = "TP10";
    pub const P9: &str = "P9";
    pub const P7: &str = "P7";
    pub const P5: &str = "P5";
    pub const P3: &str = "P3";
    pub const P1: &str = "P1";
    pub const PZ: &str = "Pz";
    pub const P2: &str = "P2";
    pub const P4: &str = "P4";
    pub const P6: &str = "P6";
    pub const P8: &str = "P8";
    pub const P10: &str = "P10";
    pub const PO9: &str = "PO9";
    pub const PO7: &str = "PO7";
    pub const PO5: &str = "PO5";
    pub const PO3: &str = "PO3";
    pub const PO1: &str = "PO1";
    pub const POZ: &str = "POz";
    pub const PO2: &str = "PO2";
    pub const PO4: &str = "PO4";
    pub const PO6: &str = "PO6";
    pub const PO8: &str = "PO8";
    pub const PO10: &str = "PO10";
    pub const O1: &str = "O1";
    pub const OZ: &str = "Oz";
    pub const O2: &str = "O2";
    pub const I1: &str = "I1";
    pub const I2: &str = "I2";
    pub const AFP9H: &str = "AFp9h";
    pub const AFP7H: &str = "AFp7h";
    pub const AFP5H: &str = "AFp5h";
    pub const AFP3H: &str = "AFp3h";
    pub const AFP1H: &str = "AFp1h";
    pub const AFP2H: &str = "AFp2h";
    pub const AFP4H: &str = "AFp4h";
    pub const AFP6H: &str = "AFp6h";
    pub const AFP8H: &str = "AFp8h";
    pub const AFP10H: &str = "AFp10h";
    pub const AFF9H: &str = "AFF9h";
    pub const AFF7H: &str = "AFF7h";
    pub const AFF5H: &str = "AFF5h";
    pub const AFF3H: &str = "AFF3h";
    pub const AFF1H: &str = "AFF1h";
    pub const AFF2H: &str = "AFF2h";
    pub const AFF4H: &str = "AFF4h";
    pub const AFF6H: &str = "AFF6h";
    pub const AFF8H: &str = "AFF8h";
    pub const AFF10H: &str = "AFF10h";
    pub const FFT9H: &str = "FFT9h";
    pub const FFT7H: &str = "FFT7h";
    pub const FFC5H: &str = "FFC5h";
    pub const FFC3H: &str = "FFC3h";
    pub const FFC1H: &str = "FFC1h";
    pub const FFC2H: &str = "FFC2h";
    pub const FFC4H: &str = "FFC4h";
    pub const FFC6H: &str = "FFC6h";
    pub const FFT8H: &str = "FFT8h";
    pub const FFT10H: &str = "FFT10h";
    pub const FTT9H: &str = "FTT9h";
    pub const FTT7H: &str = "FTT7h";
    pub const FCC5H: &str = "FCC5h";
    pub const FCC3H: &str = "FCC3h";
    pub const FCC1H: &str = "FCC1h";
    pub const FCC2H: &str = "FCC2h";
    pub const FCC4H: &str = "FCC4h";
    pub const FCC6H: &str = "FCC6h";
    pub const FTT8H: &str = "FTT8h";
    pub const FTT10H: &str = "FTT10h";
    pub const TTP9H: &str = "TTP9h";
    pub const TTP7H: &str = "TTP7h";
    pub const CCP5H: &str = "CCP5h";
    pub const CCP3H: &str = "CCP3h";
    pub const CCP1H: &str = "CCP1h";
    pub const CCP2H: &str = "CCP2h";
    pub const CCP4H: &str = "CCP4h";
    pub const CCP6H: &str = "CCP6h";
    pub const TTP8H: &str = "TTP8h";
    pub const TTP10H: &str = "TTP10h";
    pub const TPP9H: &str = "TPP9h";
    pub const TPP7H: &str = "TPP7h";
    pub const CPP5H: &str = "CPP5h";
    pub const CPP3H: &str = "CPP3h";
    pub const CPP1H: &str = "CPP1h";
    pub const CPP2H: &str = "CPP2h";
    pub const CPP4H: &str = "CPP4h";
    pub const CPP6H: &str = "CPP6h";
    pub const TPP8H: &str = "TPP8h";
    pub const TPP10H: &str = "TPP10h";
    pub const PPO9H: &str = "PPO9h";
    pub const PPO7H: &str = "PPO7h";
    pub const PPO5H: &str = "PPO5h";
    pub const PPO3H: &str = "PPO3h";
    pub const PPO1H: &str = "PPO1h";
    pub const PPO2H: &str = "PPO2h";
    pub const PPO4H: &str = "PPO4h";
    pub const PPO6H: &str = "PPO6h";
    pub const PPO8H: &str = "PPO8h";
    pub const PPO10H: &str = "PPO10h";
    pub const POO9H: &str = "POO9h";
    pub const POO7H: &str = "POO7h";
    pub const POO5H: &str = "POO5h";
    pub const POO3H: &str = "POO3h";
    pub const POO1H: &str = "POO1h";
    pub const POO2H: &str = "POO2h";
    pub const POO4H: &str = "POO4h";
    pub const POO6H: &str = "POO6h";
    pub const POO8H: &str = "POO8h";
    pub const POO10H: &str = "POO10h";
    pub const OI1H: &str = "OI1h";
    pub const OI2H: &str = "OI2h";
    pub const FP1H: &str = "Fp1h";
    pub const FP2H: &str = "Fp2h";
    pub const AF9H: &str = "AF9h";
    pub const AF7H: &str = "AF7h";
    pub const AF5H: &str = "AF5h";
    pub const AF3H: &str = "AF3h";
    pub const AF1H: &str = "AF1h";
    pub const AF2H: &str = "AF2h";
    pub const AF4H: &str = "AF4h";
    pub const AF6H: &str = "AF6h";
    pub const AF8H: &str = "AF8h";
    pub const AF10H: &str = "AF10h";
    pub const F9H: &str = "F9h";
    pub const F7H: &str = "F7h";
    pub const F5H: &str = "F5h";
    pub const F3H: &str = "F3h";
    pub const F1H: &str = "F1h";
    pub const F2H: &str = "F2h";
    pub const F4H: &str = "F4h";
    pub const F6H: &str = "F6h";
    pub const F8H: &str = "F8h";
    pub const F10H: &str = "F10h";
    pub const FT9H: &str = "FT9h";
    pub const FT7H: &str = "FT7h";
    pub const FC5H: &str = "FC5h";
    pub const FC3H: &str = "FC3h";
    pub const FC1H: &str = "FC1h";
    pub const FC2H: &str = "FC2h";
    pub const FC4H: &str = "FC4h";
    pub const FC6H: &str = "FC6h";
    pub const FT8H: &str = "FT8h";
    pub const FT10H: &str = "FT10h";
    pub const T9H: &str = "T9h";
    pub const T7H: &str = "T7h";
    pub const C5H: &str = "C5h";
    pub const C3H: &str = "C3h";
    pub const C1H: &str = "C1h";
    pub const C2H: &str = "C2h";
    pub const C4H: &str = "C4h";
    pub const C6H: &str = "C6h";
    pub const T8H: &str = "T8h";
    pub const T10H: &str = "T10h";
    pub const TP9H: &str = "TP9h";
    pub const TP7H: &str = "TP7h";
    pub const CP5H: &str = "CP5h";
    pub const CP3H: &str = "CP3h";
    pub const CP1H: &str = "CP1h";
    pub const CP2H: &str = "CP2h";
    pub const CP4H: &str = "CP4h";
    pub const CP6H: &str = "CP6h";
    pub const TP8H: &str = "TP8h";
    pub const TP10H: &str = "TP10h";
    pub const P9H: &str = "P9h";
    pub const P7H: &str = "P7h";
    pub const P5H: &str = "P5h";
    pub const P3H: &str = "P3h";
    pub const P1H: &str = "P1h";
    pub const P2H: &str = "P2h";
    pub const P4H: &str = "P4h";
    pub const P6H: &str = "P6h";
    pub const P8H: &str = "P8h";
    pub const P10H: &str = "P10h";
    pub const PO9H: &str = "PO9h";
    pub const PO7H: &str = "PO7h";
    pub const PO5H: &str = "PO5h";
    pub const PO3H: &str = "PO3h";
    pub const PO1H: &str = "PO1h";
    pub const PO2H: &str = "PO2h";
    pub const PO4H: &str = "PO4h";
    pub const PO6H: &str = "PO6h";
    pub const PO8H: &str = "PO8h";
    pub const PO10H: &str = "PO10h";
    pub const O1H: &str = "O1h";
    pub const O2H: &str = "O2h";
    pub const I1H: &str = "I1h";
    pub const I2H: &str = "I2h";
    pub const AFP9: &str = "AFp9";
    pub const AFP7: &str = "AFp7";
    pub const AFP5: &str = "AFp5";
    pub const AFP3: &str = "AFp3";
    pub const AFP1: &str = "AFp1";
    pub const AFPZ: &str = "AFpz";
    pub const AFP2: &str = "AFp2";
    pub const AFP4: &str = "AFp4";
    pub const AFP6: &str = "AFp6";
    pub const AFP8: &str = "AFp8";
    pub const AFP10: &str = "AFp10";
    pub const AFF9: &str = "AFF9";
    pub const AFF7: &str = "AFF7";
    pub const AFF5: &str = "AFF5";
    pub const AFF3: &str = "AFF3";
    pub const AFF1: &str = "AFF1";
    pub const AFFZ: &str = "AFFz";
    pub const AFF2: &str = "AFF2";
    pub const AFF4: &str = "AFF4";
    pub const AFF6: &str = "AFF6";
    pub const AFF8: &str = "AFF8";
    pub const AFF10: &str = "AFF10";
    pub const FFT9: &str = "FFT9";
    pub const FFT7: &str = "FFT7";
    pub const FFC5: &str = "FFC5";
    pub const FFC3: &str = "FFC3";
    pub const FFC1: &str = "FFC1";
    pub const FFCZ: &str = "FFCz";
    pub const FFC2: &str = "FFC2";
    pub const FFC4: &str = "FFC4";
    pub const FFC6: &str = "FFC6";
    pub const FFT8: &str = "FFT8";
    pub const FFT10: &str = "FFT10";
    pub const FTT9: &str = "FTT9";
    pub const FTT7: &str = "FTT7";
    pub const FCC5: &str = "FCC5";
    pub const FCC3: &str = "FCC3";
    pub const FCC1: &str = "FCC1";
    pub const FCCZ: &str = "FCCz";
    pub const FCC2: &str = "FCC2";
    pub const FCC4: &str = "FCC4";
    pub const FCC6: &str = "FCC6";
    pub const FTT8: &str = "FTT8";
    pub const FTT10: &str = "FTT10";
    pub const TTP9: &str = "TTP9";
    pub const TTP7: &str = "TTP7";
    pub const CCP5: &str = "CCP5";
    pub const CCP3: &str = "CCP3";
    pub const CCP1: &str = "CCP1";
    pub const CCPZ: &str = "CCPz";
    pub const CCP2: &str = "CCP2";
    pub const CCP4: &str = "CCP4";
    pub const CCP6: &str = "CCP6";
    pub const TTP8: &str = "TTP8";
    pub const TTP10: &str = "TTP10";
    pub const TPP9: &str = "TPP9";
    pub const TPP7: &str = "TPP7";
    pub const CPP5: &str = "CPP5";
    pub const CPP3: &str = "CPP3";
    pub const CPP1: &str = "CPP1";
    pub const CPPZ: &str = "CPPz";
    pub const CPP2: &str = "CPP2";
    pub const CPP4: &str = "CPP4";
    pub const CPP6: &str = "CPP6";
    pub const TPP8: &str = "TPP8";
    pub const TPP10: &str = "TPP10";
    pub const PPO9: &str = "PPO9";
    pub const PPO7: &str = "PPO7";
    pub const PPO5: &str = "PPO5";
    pub const PPO3: &str = "PPO3";
    pub const PPO1: &str = "PPO1";
    pub const PPOZ: &str = "PPOz";
    pub const PPO2: &str = "PPO2";
    pub const PPO4: &str = "PPO4";
    pub const PPO6: &str = "PPO6";
    pub const PPO8: &str = "PPO8";
    pub const PPO10: &str = "PPO10";
    pub const POO9: &str = "POO9";
    pub const POO7: &str = "POO7";
    pub const POO5: &str = "POO5";
    pub const POO3: &str = "POO3";
    pub const POO1: &str = "POO1";
    pub const POOZ: &str = "POOz";
    pub const POO2: &str = "POO2";
    pub const POO4: &str = "POO4";
    pub const POO6: &str = "POO6";
    pub const POO8: &str = "POO8";
    pub const POO10: &str = "POO10";
    pub const OI1: &str = "OI1";
    pub const OIZ: &str = "OIz";
    pub const OI2: &str = "OI2";

    // ── Legacy aliases ──────────────────────────────────────────────────────
    /// Old name for [`T7`] (`T7`).
    pub const T3: &str = "T7";
    /// Old name for [`T8`] (`T8`).
    pub const T4: &str = "T8";
    /// Old name for [`P7`] (`P7`).
    pub const T5: &str = "P7";
    /// Old name for [`P8`] (`P8`).
    pub const T6: &str = "P8";
    /// Old name for [`TP9`] (`TP9`).
    pub const M1: &str = "TP9";
    /// Old name for [`TP10`] (`TP10`).
    pub const M2: &str = "TP10";
    /// Old name for [`TP9`] (`TP9`).
    pub const A1: &str = "TP9";
    /// Old name for [`TP10`] (`TP10`).
    pub const A2: &str = "TP10";
    /// Old name for [`I1`] (`I1`).
    pub const O9: &str = "I1";
    /// Old name for [`I2`] (`I2`).
    pub const O10: &str = "I2";
}
