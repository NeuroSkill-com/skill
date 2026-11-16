// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Montages: what the channels of a recording are — their labels, what kind
//! of signal each carries, where its electrode sits, which ones are bad.
//!
//! A device says little about this (an amplifier knows its inputs as `CH1` …
//! `CH32`; where the cap puts them is the experimenter's business), so a
//! montage comes from the person:
//!
//! * **by default** — a library of named montages in the settings, and rules
//!   that pick one for a session from its device kind, device name and channel
//!   count ([`MontageSettings::default_for`]). The labels of a default montage
//!   are what the session records and analyses under.
//! * **after the fact** — any recording can be given a montage later, changed,
//!   or have it taken away again ([`set_session_montage`]). Nothing recorded is
//!   rewritten: the montage lives in a file of its own beside the recording
//!   (`exg_<ts>.montage`, JSON) together with the channel names as recorded,
//!   and readers lay it over the recording ([`effective_channels`]).
//!
//! ## Writing a montage down
//!
//! Everything that takes a montage takes a [`MontageSpec`], which is whichever
//! of these is at hand:
//!
//! ```json
//! "Enobio 32 cap"                          // a name from the library
//! "Fp1 Fp2 F7 F3 Fz F4 F8"                 // labels in channel order
//! ["Fp1", "Fp2", "F7"]                     // the same as a list
//! {"CH1": "Fp1", "CH32": "EOG", "3": "Cz"} // some channels, by recorded name or number
//! {"name": "...", "channels": [{"source": "CH32", "label": "HEOG", "kind": "eog"}], "reference": "CMS"}
//! ```
//!
//! A montage need not cover every channel (the others keep their recorded
//! names) and may name channels a recording does not have (they are reported,
//! not an error): one montage can serve caps of several sizes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What a channel carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelKind {
    Eeg,
    Eog,
    Ecg,
    Emg,
    /// A reference or ground electrode recorded as a channel.
    Ref,
    /// Trigger / stimulus codes.
    Trigger,
    /// Anything else (auxiliary inputs, respiration belts, ...).
    Misc,
}

impl ChannelKind {
    /// The kind a label suggests, for channels whose montage does not say.
    pub fn from_label(label: &str) -> Self {
        let l = label.to_ascii_lowercase();
        let has = |words: &[&str]| words.iter().any(|w| l.contains(w));
        if has(&["eog"]) {
            Self::Eog
        } else if has(&["ecg", "ekg"]) {
            Self::Ecg
        } else if has(&["emg"]) {
            Self::Emg
        } else if has(&["trig", "stim", "status"]) {
            Self::Trigger
        } else if l == "ref" || l == "gnd" || has(&["drl", "cms"]) {
            Self::Ref
        } else if l.starts_with("aux") || has(&["resp", "gsr", "temp"]) {
            Self::Misc
        } else {
            Self::Eeg
        }
    }

    /// Parse `eeg`, `EOG`, ... (as written in a text montage).
    pub fn parse(word: &str) -> Option<Self> {
        serde_json::from_value(serde_json::Value::String(word.to_ascii_lowercase())).ok()
    }
}

/// One channel of a montage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MontageChannel {
    /// Which recorded channel this is: its recorded name (`CH7`) or its number
    /// counted from 1 (`7`). Absent: the channel at this entry's own place in
    /// the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The label it gets (`Cz`).
    pub label: String,
    /// What it carries; absent: guessed from the label ([`ChannelKind::from_label`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChannelKind>,
    /// Electrode position `[x, y, z]`, in the montage's own coordinate system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<[f64; 3]>,
    /// Known to be unusable (bridged, off the scalp, broken lead).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bad: bool,
}

/// A montage; see the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Montage {
    /// Its name in the library; empty for one written out in place.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub channels: Vec<MontageChannel>,
    /// The reference the signals were recorded against (`CMS`, `Cz`, `linked mastoids`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground: Option<String>,
    /// The coordinate system and unit of the positions (`CapTrak, mm`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A montage as people write it; see the module docs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MontageSpec {
    /// A name from the library, or labels separated by spaces, commas or
    /// line breaks (`CH3=Cz` and `Cz:eog` are understood too).
    Text(String),
    /// Labels in channel order.
    Labels(Vec<String>),
    /// Recorded name or number → label, for some channels.
    Map(BTreeMap<String, String>),
    /// Written out in full.
    Full(Montage),
}

impl Montage {
    /// The montage a spec stands for. A text that is the name of a montage in
    /// `library` (case-insensitive) is that montage; any other text is read
    /// as labels.
    pub fn from_spec(spec: MontageSpec, library: &[Montage]) -> Result<Montage, String> {
        let montage = match spec {
            MontageSpec::Full(m) => m,
            MontageSpec::Labels(labels) => Montage {
                channels: labels.iter().filter_map(|l| channel_from_text(l)).collect(),
                ..Montage::default()
            },
            MontageSpec::Map(map) => Montage {
                channels: map
                    .into_iter()
                    .map(|(source, label)| MontageChannel {
                        source: Some(source),
                        ..channel_from_text(&label).unwrap_or_default()
                    })
                    .collect(),
                ..Montage::default()
            },
            MontageSpec::Text(text) => {
                let text = text.trim();
                if let Some(named) = library
                    .iter()
                    .find(|m| !m.name.is_empty() && m.name.eq_ignore_ascii_case(text))
                {
                    return Ok(named.clone());
                }
                // JSON pasted into a text field
                if text.starts_with(['{', '[']) {
                    let spec: MontageSpec = serde_json::from_str(text).map_err(|e| format!("not a montage: {e}"))?;
                    if matches!(spec, MontageSpec::Text(_)) {
                        return Err("not a montage".into());
                    }
                    return Montage::from_spec(spec, library);
                }
                Montage {
                    channels: text
                        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
                        .filter_map(channel_from_text)
                        .collect(),
                    ..Montage::default()
                }
            }
        };
        if montage.channels.is_empty() {
            return Err("the montage names no channels".into());
        }
        if let Some(c) = montage.channels.iter().find(|c| c.label.trim().is_empty()) {
            return Err(format!(
                "a channel{} has no label",
                c.source.as_ref().map(|s| format!(" ({s})")).unwrap_or_default()
            ));
        }
        Ok(montage)
    }

    /// Lay the montage over the channels of a recording.
    pub fn resolve(&self, recorded: &[String]) -> ResolvedMontage {
        let mut channels: Vec<ResolvedChannel> = recorded
            .iter()
            .enumerate()
            .map(|(index, name)| ResolvedChannel {
                index,
                recorded: name.clone(),
                label: name.clone(),
                kind: ChannelKind::from_label(name),
                position: None,
                bad: false,
                from_montage: false,
            })
            .collect();
        let mut warnings = Vec::new();
        for (place, c) in self.channels.iter().enumerate() {
            let index = match c.source.as_deref().map(str::trim) {
                None | Some("") => Some(place).filter(|&p| p < recorded.len()),
                Some(source) => recorded
                    .iter()
                    .position(|r| r.eq_ignore_ascii_case(source))
                    .or_else(|| {
                        source
                            .parse::<usize>()
                            .ok()
                            .filter(|&n| n >= 1 && n <= recorded.len())
                            .map(|n| n - 1)
                    }),
            };
            let Some(index) = index else {
                warnings.push(match &c.source {
                    Some(s) => format!("'{}' is for channel '{s}', which this recording does not have", c.label),
                    None => format!(
                        "'{}' is channel {} of the montage; this recording has {}",
                        c.label,
                        place + 1,
                        recorded.len()
                    ),
                });
                continue;
            };
            let slot = &mut channels[index];
            if slot.from_montage {
                warnings.push(format!(
                    "channel {} ('{}') is named twice: '{}' and '{}'",
                    index + 1,
                    slot.recorded,
                    slot.label,
                    c.label
                ));
            }
            slot.label = c.label.trim().to_string();
            slot.kind = c.kind.unwrap_or_else(|| ChannelKind::from_label(&c.label));
            slot.position = c.position;
            slot.bad = c.bad;
            slot.from_montage = true;
        }
        for (i, c) in channels.iter().enumerate() {
            if channels[..i].iter().any(|o| o.label.eq_ignore_ascii_case(&c.label)) {
                warnings.push(format!("two channels are called '{}'", c.label));
            }
        }
        let covered = channels.iter().filter(|c| c.from_montage).count();
        ResolvedMontage {
            name: self.name.clone(),
            channels,
            covered,
            reference: self.reference.clone(),
            ground: self.ground.clone(),
            coordinates: self.coordinates.clone(),
            warnings,
        }
    }
}

/// One entry of a text montage: `Cz`, `CH3=Cz`, `Cz:eog`, `CH3=HEOG:eog`, a
/// trailing `!` for a bad channel. `None` for an empty piece.
fn channel_from_text(piece: &str) -> Option<MontageChannel> {
    let piece = piece.trim();
    if piece.is_empty() {
        return None;
    }
    let (source, rest) = match piece.split_once('=') {
        Some((s, r)) => (Some(s.trim().to_string()), r.trim()),
        None => (None, piece),
    };
    let (rest, bad) = match rest.strip_suffix('!') {
        Some(r) => (r, true),
        None => (rest, false),
    };
    let (label, kind) = match rest.rsplit_once(':') {
        Some((l, k)) if ChannelKind::parse(k.trim()).is_some() => (l.trim(), ChannelKind::parse(k.trim())),
        _ => (rest.trim(), None),
    };
    Some(MontageChannel {
        source,
        label: label.to_string(),
        kind,
        position: None,
        bad,
    })
}

/// A channel of a recording with its montage laid over it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedChannel {
    /// Its place in the recording, from 0.
    pub index: usize,
    /// Its name as recorded.
    pub recorded: String,
    /// Its label: the montage's, or the recorded name.
    pub label: String,
    pub kind: ChannelKind,
    pub position: Option<[f64; 3]>,
    pub bad: bool,
    /// The montage says something about this channel.
    pub from_montage: bool,
}

/// A montage laid over a recording.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedMontage {
    pub name: String,
    pub channels: Vec<ResolvedChannel>,
    /// Channels the montage covers.
    pub covered: usize,
    pub reference: Option<String>,
    pub ground: Option<String>,
    pub coordinates: Option<String>,
    /// What did not fit: montage entries without a channel, channels named
    /// twice, labels used twice. Worth showing; none of it stops anything.
    pub warnings: Vec<String>,
}

impl ResolvedMontage {
    pub fn labels(&self) -> Vec<String> {
        self.channels.iter().map(|c| c.label.clone()).collect()
    }
}

// ── Defaults (settings) ───────────────────────────────────────────────────────

/// When a montage of the library is a session's default. Every condition
/// that is given must hold; a rule without conditions holds for any session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MontageRule {
    /// Name of the montage in the library.
    pub montage: String,
    /// The session's device kind (`lsl`, `openbci`, `muse`, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_kind: Option<String>,
    /// The device or stream name; `*` stands for any run of characters
    /// (`EN32*`, `*-EEG`). Case-insensitive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
    /// The number of channels the device delivers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_count: Option<usize>,
}

impl MontageRule {
    fn holds(&self, device_kind: &str, device_name: &str, channel_count: usize) -> bool {
        self.device_kind
            .as_deref()
            .is_none_or(|k| k.eq_ignore_ascii_case(device_kind))
            && self
                .device_name
                .as_deref()
                .is_none_or(|p| glob(&p.to_lowercase(), &device_name.to_lowercase()))
            && self.channel_count.is_none_or(|n| n == channel_count)
    }
}

/// `*` wildcard match.
fn glob(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((head, rest)) => {
            let Some(tail) = text.strip_prefix(head) else {
                return false;
            };
            if rest.is_empty() {
                return true;
            }
            (0..=tail.len())
                .filter(|&i| tail.is_char_boundary(i))
                .any(|i| glob(rest, &tail[i..]))
        }
    }
}

/// The montages of the settings: a library, and which of them is whose default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MontageSettings {
    #[serde(default)]
    pub library: Vec<Montage>,
    /// Tried in order; the first rule that holds decides.
    #[serde(default)]
    pub defaults: Vec<MontageRule>,
}

impl MontageSettings {
    /// The default montage of a session on this device, with the rule that
    /// chose it.
    pub fn default_for(
        &self,
        device_kind: &str,
        device_name: &str,
        channel_count: usize,
    ) -> Option<(&Montage, &MontageRule)> {
        self.defaults
            .iter()
            .filter(|r| r.holds(device_kind, device_name, channel_count))
            .find_map(|r| {
                self.library
                    .iter()
                    .find(|m| m.name.eq_ignore_ascii_case(&r.montage))
                    .map(|m| (m, r))
            })
    }

    /// What is wrong with these settings, in words: rules that name no
    /// montage of the library, montages without a name or with a name used
    /// twice. Empty when all is well.
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (i, m) in self.library.iter().enumerate() {
            if m.name.trim().is_empty() {
                out.push(format!("montage {} of the library has no name", i + 1));
            } else if self.library[..i].iter().any(|o| o.name.eq_ignore_ascii_case(&m.name)) {
                out.push(format!("two montages are called '{}'", m.name));
            }
            if m.channels.is_empty() {
                out.push(format!("montage '{}' names no channels", m.name));
            }
        }
        for r in &self.defaults {
            if !self.library.iter().any(|m| m.name.eq_ignore_ascii_case(&r.montage)) {
                out.push(format!(
                    "a default names the montage '{}', which is not in the library",
                    r.montage
                ));
            }
        }
        out
    }
}

// ── A recording's own montage ─────────────────────────────────────────────────

/// Where a recording's montage came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MontageOrigin {
    /// Chosen by a default rule when the session started.
    Default,
    /// Given to this recording by hand.
    Session,
}

/// The file beside a recording that holds its montage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMontage {
    pub montage: Montage,
    /// The channel names the device delivered, in order — what the montage's
    /// `source` entries refer to, whatever labels the recording was written
    /// under.
    pub recorded_channel_names: Vec<String>,
    pub origin: MontageOrigin,
    /// When it was set (Unix seconds).
    pub set_utc: u64,
}

/// `exg_<ts>.montage` (JSON) beside the recording `exg_<ts>.csv` / `.parquet`. Not a
/// `.json` name: every `exg_*.json` in a day's folder is taken for a session's sidecar,
/// also by versions of the app that know nothing of montages.
pub fn montage_path(recording: &Path) -> PathBuf {
    recording.with_extension("montage")
}

/// The montage file of a recording, if it has one that reads.
pub fn read_session_montage(recording: &Path) -> Option<SessionMontage> {
    serde_json::from_str(&std::fs::read_to_string(montage_path(recording)).ok()?).ok()
}

/// The channel names a recording was written under, from its sidecar.
fn sidecar_channel_names(recording: &Path) -> Vec<String> {
    std::fs::read_to_string(recording.with_extension("json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| serde_json::from_value(v.get("channel_names")?.clone()).ok())
        .unwrap_or_default()
}

/// Give a recording a montage (or replace the one it has). Nothing recorded
/// is touched. Returns the montage laid over the recording's channels.
///
/// `recorded` are the device's channel names when the caller knows them (a
/// session that is starting); otherwise they are taken from the montage file
/// the recording already has, or from its sidecar.
pub fn set_session_montage(
    recording: &Path,
    montage: Montage,
    origin: MontageOrigin,
    recorded: Option<Vec<String>>,
) -> std::io::Result<ResolvedMontage> {
    let recorded = recorded
        .or_else(|| read_session_montage(recording).map(|m| m.recorded_channel_names))
        .unwrap_or_else(|| sidecar_channel_names(recording));
    if recorded.is_empty() {
        return Err(std::io::Error::other(
            "the recording's channels are not known (it has no readable sidecar)",
        ));
    }
    let resolved = montage.resolve(&recorded);
    let file = SessionMontage {
        montage,
        recorded_channel_names: recorded,
        origin,
        set_utc: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let path = montage_path(recording);
    let tmp = path.with_extension("montage.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(&tmp, &path)?;
    Ok(resolved)
}

/// Take a recording's montage away: its channels are what was recorded
/// again. `false` when it had none.
pub fn clear_session_montage(recording: &Path) -> std::io::Result<bool> {
    match std::fs::remove_file(montage_path(recording)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// The channels of a recording as they are to be read: its montage laid over
/// what the device delivered, or — without a montage — the names it was
/// recorded under. `None` when the recording has neither montage nor sidecar.
pub fn effective_channels(recording: &Path) -> Option<(ResolvedMontage, Option<MontageOrigin>)> {
    if let Some(m) = read_session_montage(recording) {
        return Some((m.montage.resolve(&m.recorded_channel_names), Some(m.origin)));
    }
    let recorded = sidecar_channel_names(recording);
    (!recorded.is_empty()).then(|| (Montage::default().resolve(&recorded), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (1..=n).map(|k| format!("CH{k}")).collect()
    }

    fn spec(json: &str) -> MontageSpec {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_montage_can_be_written_in_whichever_form_is_at_hand() {
        let library = vec![Montage {
            name: "Cap A".into(),
            channels: vec![MontageChannel {
                label: "Oz".into(),
                ..Default::default()
            }],
            ..Default::default()
        }];
        let labels = |m: &Montage| m.resolve(&names(4)).labels();

        // labels in order: text, with any separators, or a list
        for s in [
            r#""Fp1 Fp2, F7;F3""#,
            r#""Fp1\nFp2\nF7\nF3""#,
            r#"["Fp1","Fp2","F7","F3"]"#,
        ] {
            let m = Montage::from_spec(spec(s), &library).unwrap();
            assert_eq!(labels(&m), ["Fp1", "Fp2", "F7", "F3"], "{s}");
        }
        // some channels, by recorded name or by number; the rest keep their names
        let m = Montage::from_spec(spec(r#"{"CH4": "HEOG", "2": "Cz"}"#), &library).unwrap();
        let r = m.resolve(&names(4));
        assert_eq!(r.labels(), ["CH1", "Cz", "CH3", "HEOG"]);
        assert_eq!(
            (r.covered, r.channels[3].kind, r.channels[1].kind),
            (2, ChannelKind::Eog, ChannelKind::Eeg)
        );
        // the same in text, with a kind and a bad channel
        let m = Montage::from_spec(spec(r#""CH4=X1:emg CH2=Cz!""#), &library).unwrap();
        let r = m.resolve(&names(4));
        assert_eq!(r.labels(), ["CH1", "Cz", "CH3", "X1"]);
        assert_eq!(
            (r.channels[3].kind, r.channels[1].bad, r.channels[3].bad),
            (ChannelKind::Emg, true, false)
        );
        // a name from the library, in any case
        assert_eq!(Montage::from_spec(spec(r#""cap a""#), &library).unwrap(), library[0]);
        // written out in full, also when it was pasted as text
        let full =
            r#"{"name":"mine","channels":[{"source":"CH3","label":"Pz","position":[0.0,-0.7,0.7]}],"reference":"CMS"}"#;
        for s in [full.to_string(), serde_json::to_string(full).unwrap()] {
            let m = Montage::from_spec(spec(&s), &library).unwrap();
            let r = m.resolve(&names(4));
            assert_eq!(r.labels(), ["CH1", "CH2", "Pz", "CH4"]);
            assert_eq!(r.channels[2].position, Some([0.0, -0.7, 0.7]));
            assert_eq!(r.reference.as_deref(), Some("CMS"));
        }
        // nothing to go by
        assert!(Montage::from_spec(spec(r#""  ""#), &library).is_err());
        assert!(Montage::from_spec(spec("[]"), &library).is_err());
    }

    #[test]
    fn what_does_not_fit_is_said_and_stops_nothing() {
        let m = Montage::from_spec(spec(r#""Fp1 Fp2 F7 F3 Fz""#), &[]).unwrap();
        // a montage for a bigger cap on a smaller recording
        let r = m.resolve(&names(3));
        assert_eq!(r.labels(), ["Fp1", "Fp2", "F7"]);
        assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
        // and for a smaller cap on a bigger one: the rest keep their names
        let r = m.resolve(&names(7));
        assert_eq!(r.labels(), ["Fp1", "Fp2", "F7", "F3", "Fz", "CH6", "CH7"]);
        assert!(r.warnings.is_empty());
        // a channel the recording does not have, one named twice, a label used twice
        let m = Montage::from_spec(spec(r#"{"CH9": "Oz", "CH1": "Cz", "1": "Pz", "CH2": "cz"}"#), &[]).unwrap();
        let r = m.resolve(&names(3));
        assert_eq!(r.warnings.len(), 3, "{:?}", r.warnings);
        assert_eq!(r.channels.len(), 3);
    }

    #[test]
    fn the_first_rule_that_holds_gives_the_default() {
        let montage = |name: &str, first: &str| Montage {
            name: name.into(),
            channels: vec![MontageChannel {
                label: first.into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let rule = |m: &str, kind: Option<&str>, name: Option<&str>, n: Option<usize>| MontageRule {
            montage: m.into(),
            device_kind: kind.map(Into::into),
            device_name: name.map(Into::into),
            channel_count: n,
        };
        let settings = MontageSettings {
            library: vec![
                montage("EN32 cap", "Fp1"),
                montage("EN8 strip", "F3"),
                montage("Any LSL", "A1"),
            ],
            defaults: vec![
                rule("EN32 cap", Some("lsl"), Some("en32*"), Some(32)),
                rule("EN8 strip", None, Some("*-EEG"), Some(8)),
                rule("Gone", Some("lsl"), None, None),
                rule("Any LSL", Some("LSL"), None, None),
            ],
        };
        let pick = |kind: &str, name: &str, n: usize| settings.default_for(kind, name, n).map(|(m, _)| m.name.as_str());
        assert_eq!(pick("lsl", "EN32-EEG", 32), Some("EN32 cap"));
        assert_eq!(pick("lsl", "EN8-EEG", 8), Some("EN8 strip"));
        assert_eq!(pick("openbci", "Lab-EEG", 8), Some("EN8 strip"));
        // a rule whose montage is gone is passed over
        assert_eq!(pick("lsl", "Other", 16), Some("Any LSL"));
        assert_eq!(pick("muse", "Muse-1234", 4), None);
        assert_eq!(settings.problems().len(), 1);
        assert!(glob("a*c*e", "abcde") && glob("*", "") && !glob("a*c", "abd") && glob("é*", "éa"));
    }

    #[test]
    fn a_recording_gets_a_montage_after_the_fact_and_loses_it_again() {
        let dir = tempfile::tempdir().unwrap();
        let rec = dir.path().join("exg_1700000000.parquet");
        // no sidecar: its channels are not known
        let m = Montage::from_spec(spec(r#""Fp1 Fp2 Cz""#), &[]).unwrap();
        assert!(effective_channels(&rec).is_none());
        assert!(set_session_montage(&rec, m.clone(), MontageOrigin::Session, None).is_err());

        std::fs::write(
            rec.with_extension("json"),
            r#"{"channel_names": ["CH1","CH2","CH3","CH4"]}"#,
        )
        .unwrap();
        let (plain, origin) = effective_channels(&rec).unwrap();
        assert_eq!((plain.labels(), origin, plain.covered), (names(4), None, 0));

        let r = set_session_montage(&rec, m, MontageOrigin::Session, None).unwrap();
        assert_eq!(r.labels(), ["Fp1", "Fp2", "Cz", "CH4"]);
        assert_eq!(montage_path(&rec).file_name().unwrap(), "exg_1700000000.montage");
        let (again, origin) = effective_channels(&rec).unwrap();
        assert_eq!((again, origin), (r, Some(MontageOrigin::Session)));

        // A second one replaces the first, and still refers to the channels as the device
        // delivered them — also when the sidecar has other names by now (a session that
        // was recorded under its default montage's labels).
        std::fs::write(
            rec.with_extension("json"),
            r#"{"channel_names": ["Fp1","Fp2","Cz","CH4"]}"#,
        )
        .unwrap();
        let m = Montage::from_spec(spec(r#"{"CH4": "Oz", "CH1": "AF7"}"#), &[]).unwrap();
        let r = set_session_montage(&rec, m, MontageOrigin::Session, None).unwrap();
        assert_eq!(r.labels(), ["AF7", "CH2", "CH3", "Oz"]);

        assert!(clear_session_montage(&rec).unwrap());
        assert!(!clear_session_montage(&rec).unwrap());
        assert_eq!(
            effective_channels(&rec).unwrap().0.labels(),
            ["Fp1", "Fp2", "Cz", "CH4"]
        );
    }
}
