// SPDX-License-Identifier: GPL-3.0-only
//! Device health: what is wrong with the connected EEG device right now, and
//! what the user can do about it.
//!
//! Pure logic over [`StatusResponse`] so the monitor (toasts / UI banner), the
//! LLM chat (a notice on every reply) and tests all see the same verdict.
//!
//! Faults never stop anything. A 64-electrode cap may carry a few dead
//! electrodes for a whole session; the job here is to say so — which channels,
//! for how long, what to adjust — while recording, brain-state metrics,
//! embeddings and chat carry on using the channels that work.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bbtree::unsync::BBTreeMap;
use serde::{Deserialize, Serialize};

use crate::StatusResponse;

/// Battery at or below this is a warning.
pub const BATTERY_LOW_PCT: f32 = 20.0;
/// Battery at or below this is critical.
pub const BATTERY_CRITICAL_PCT: f32 = 10.0;
/// No new samples for this long while "connected" means the data stopped.
pub const DATA_STALL_AFTER: Duration = Duration::from_secs(5);
/// A channel bad for this long is a persistent fault, not a passing glitch.
pub const PERSISTENT_FAULT_AFTER: Duration = Duration::from_secs(5 * 60);
/// A channel must be bad this long before it is reported at all, so a blink
/// or a single bad window doesn't raise an alarm.
pub const BAD_CHANNEL_GRACE: Duration = Duration::from_secs(5);
/// Window for spotting an electrode that keeps cutting out.
pub const INTERMITTENT_WINDOW: Duration = Duration::from_secs(30);
/// Bad this share of the window (without staying bad) = intermittent contact.
pub const INTERMITTENT_BAD_SHARE: f64 = 0.25;
/// History must span this long before judging intermittent contact.
const INTERMITTENT_MIN_SPAN: Duration = Duration::from_secs(15);
/// Fair (usable but noisy) this share of the window → "could be improved".
pub const FAIR_CONTACT_SHARE: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HealthIssue {
    /// Stable code: `battery_low`, `battery_critical`, `no_signal`,
    /// `poor_contact`, `faulty_electrodes`, `no_usable_channels`,
    /// `reference_suspect`, `data_stalled`, `device_error`.
    pub code: String,
    pub severity: Severity,
    /// What is wrong, for the user.
    pub message: String,
    /// What to do about it.
    pub advice: String,
    /// Affected electrodes, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceHealth {
    pub connected: bool,
    pub device_name: Option<String>,
    pub issues: Vec<HealthIssue>,
    /// Electrodes with usable signal.
    pub usable_channels: Vec<String>,
    pub total_channels: usize,
    /// Usable electrodes that brain-state metrics and embeddings actually read
    /// (the pipeline analyses at most `analysis_limit` channels).
    #[serde(default)]
    pub analysed_channels: usize,
    #[serde(default)]
    pub analysis_limit: Option<usize>,
}

impl DeviceHealth {
    pub fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn worst(&self) -> Option<Severity> {
        self.issues.iter().map(|i| i.severity).max()
    }

    /// Identity of the issue set, ignoring numbers that move every reading
    /// (battery %), so "the same problems" compare equal.
    pub fn fingerprint(&self) -> String {
        let mut parts: Vec<String> = self
            .issues
            .iter()
            .map(|i| format!("{}:{}", i.code, i.channels.join(",")))
            .collect();
        parts.sort();
        parts.join("|")
    }

    /// Markdown notice appended to chat replies. `detailed` adds the advice;
    /// otherwise one line per issue, so a fault the user already knows about
    /// is a reminder, not a wall of text.
    pub fn notice_markdown(&self, detailed: bool) -> Option<String> {
        if self.issues.is_empty() {
            return None;
        }
        let name = self.device_name.as_deref().unwrap_or("EEG device");
        let mut out = format!("**Device check — {name}:**");
        for issue in &self.issues {
            let icon = match issue.severity {
                Severity::Critical => "🔴",
                Severity::Warning => "🟠",
                Severity::Info => "ℹ️",
            };
            out.push_str(&format!("\n- {icon} {}", issue.message));
            if detailed && !issue.advice.is_empty() {
                out.push_str(&format!(" {}", issue.advice));
            }
        }
        match self.analysis_limit {
            Some(limit) if self.total_channels > limit => out.push_str(&format!(
                "\n- Brain-state analysis reads the first {limit} of {} electrodes; {} of those are usable.",
                self.total_channels, self.analysed_channels
            )),
            _ if self.total_channels > 0 && self.usable_channels.len() < self.total_channels => {
                out.push_str(&format!(
                    "\n- Analysis is using {} of {} electrodes.",
                    self.usable_channels.len(),
                    self.total_channels
                ));
            }
            _ => {}
        }
        Some(out)
    }

    /// Plain-text context for the model's system prompt.
    pub fn prompt_context(&self) -> Option<String> {
        if self.issues.is_empty() {
            return None;
        }
        let lines: Vec<String> = self.issues.iter().map(|i| format!("- {}", i.message)).collect();
        Some(format!(
            "EEG device status: {}\nUsable electrodes: {} of {} ({}).\n\
             Answer the user's question fully. Where it depends on their EEG, base it on the \
             usable electrodes and say briefly if the problems above make it less reliable. \
             A device notice with fixes is added to your reply automatically; don't repeat it.",
            lines.join("\n"),
            self.usable_channels.len(),
            self.total_channels,
            self.usable_channels.join(", ")
        ))
    }
}

/// Tracks status over time: how long each electrode has been bad and whether
/// samples are still arriving. Feed it every status reading.
#[derive(Debug)]
pub struct HealthTracker {
    bad_since: BBTreeMap<String, Instant>,
    /// Recent (time, bad, fair) readings per electrode, for intermittent and
    /// improvable contact.
    history: BBTreeMap<String, VecDeque<(Instant, bool, bool)>>,
    last_samples: Option<(u64, Instant)>,
    persistent_after: Duration,
    analysis_limit: Option<usize>,
}

impl Default for HealthTracker {
    fn default() -> Self {
        Self {
            bad_since: BBTreeMap::new(),
            history: BBTreeMap::new(),
            last_samples: None,
            persistent_after: PERSISTENT_FAULT_AFTER,
            analysis_limit: None,
        }
    }
}

impl HealthTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// How long an electrode must stay bad to count as a persistent fault
    /// (default [`PERSISTENT_FAULT_AFTER`]; shortened by scenario tests).
    pub fn with_persistent_after(mut self, after: Duration) -> Self {
        self.persistent_after = after.max(BAD_CHANNEL_GRACE);
        self
    }

    /// The pipeline analyses only the first `limit` channels; report that
    /// honestly for larger caps.
    pub fn with_analysis_limit(mut self, limit: usize) -> Self {
        self.analysis_limit = Some(limit);
        self
    }

    pub fn observe(&mut self, status: &StatusResponse, now: Instant) -> DeviceHealth {
        let connected = status.state == "connected";
        if !connected {
            self.bad_since.clear();
            self.history.clear();
            self.last_samples = None;
            return DeviceHealth {
                connected: false,
                device_name: status.device_name.clone(),
                ..Default::default()
            };
        }

        // Data flow.
        let stalled_for = match self.last_samples {
            Some((n, at)) if n == status.sample_count => Some(now.saturating_duration_since(at)),
            _ => {
                self.last_samples = Some((status.sample_count, now));
                None
            }
        };

        // Electrode history.
        let labels = channel_labels(status);
        let mut bad_now = Vec::new();
        for (label, q) in labels.iter().zip(&status.channel_quality) {
            let bad = is_bad(q);
            if bad {
                self.bad_since.entry(label.clone()).or_insert(now);
                bad_now.push((label.clone(), q.clone()));
            } else {
                self.bad_since.remove(label);
            }
            let h = self.history.entry(label.clone()).or_default();
            h.push_back((now, bad, q == "fair"));
            while h
                .front()
                .is_some_and(|(t, _, _)| now.saturating_duration_since(*t) > INTERMITTENT_WINDOW)
            {
                h.pop_front();
            }
        }
        self.bad_since.retain(|l, _| labels.contains(l));
        self.history.retain(|l, _| labels.contains(l));
        let bad_for = |l: &str| self.bad_since.get(l).map(|t| now.saturating_duration_since(*t));

        // Keeps cutting out: dropped out at least twice, bad for a real
        // share of the window, and not currently a settled (continuously bad)
        // fault. "At least twice" is what separates a loose electrode from
        // one that was bad and has just been fixed.
        let intermittent: Vec<String> = labels
            .iter()
            .filter(|l| {
                let Some(h) = self.history.get(*l) else { return false };
                let bad = h.iter().filter(|(_, b, _)| *b).count();
                let dropouts = h
                    .iter()
                    .zip(h.iter().skip(1))
                    .filter(|((_, was_bad, _), (_, is_bad, _))| !*was_bad && *is_bad)
                    .count()
                    + usize::from(h.front().is_some_and(|(_, b, _)| *b));
                h.front()
                    .is_some_and(|(t, _, _)| now.saturating_duration_since(*t) >= INTERMITTENT_MIN_SPAN)
                    && dropouts >= 2
                    && bad as f64 / h.len() as f64 >= INTERMITTENT_BAD_SHARE
                    && !bad_for(l).is_some_and(|d| d >= BAD_CHANNEL_GRACE)
            })
            .cloned()
            .collect();

        // Usable but noisy (hum / muscle / dry sensor) for most of the window:
        // contact could be improved. Analysis keeps using these.
        let improvable: Vec<String> = labels
            .iter()
            .filter(|l| !intermittent.contains(l) && !bad_for(l).is_some_and(|d| d >= BAD_CHANNEL_GRACE))
            .filter(|l| {
                let Some(h) = self.history.get(*l) else { return false };
                let fair = h.iter().filter(|(_, _, f)| *f).count();
                h.front()
                    .is_some_and(|(t, _, _)| now.saturating_duration_since(*t) >= INTERMITTENT_MIN_SPAN)
                    && fair as f64 / h.len() as f64 >= FAIR_CONTACT_SHARE
            })
            .cloned()
            .collect();

        let mut health = assess(status, &labels, &bad_now, bad_for, stalled_for, self.persistent_after);
        let nothing_usable = health.issues.iter().any(|i| i.code == "no_usable_channels");
        if !improvable.is_empty() && !nothing_usable {
            health.issues.push(HealthIssue {
                code: "fair_contact".into(),
                severity: Severity::Info,
                message: format!(
                    "Contact on {} could be better: the signal is noisy (electrical hum or muscle tension).",
                    improvable.join(", ")
                ),
                advice: format!(
                    "{} Relaxing your jaw and forehead also helps.",
                    electrode_advice(&status.device_kind, &improvable)
                ),
                channels: improvable,
            });
        }
        if !intermittent.is_empty() && !nothing_usable {
            health.issues.push(HealthIssue {
                code: "intermittent_contact".into(),
                severity: Severity::Warning,
                message: format!("{} keeps cutting out.", intermittent.join(", ")),
                advice: format!(
                    "{} It is probably loose.",
                    electrode_advice(&status.device_kind, &intermittent)
                ),
                channels: intermittent,
            });
        }
        health.issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
        health.analysis_limit = self.analysis_limit;
        let limit = self.analysis_limit.unwrap_or(usize::MAX);
        health.analysed_channels = labels
            .iter()
            .take(limit)
            .filter(|l| health.usable_channels.contains(l))
            .count();
        health
    }
}

fn is_bad(q: &str) -> bool {
    // `nosignal` is what older daemons sent for `no_signal`.
    matches!(q, "poor" | "no_signal" | "nosignal")
}

fn channel_labels(status: &StatusResponse) -> Vec<String> {
    let n = status.channel_quality.len();
    (0..n)
        .map(|i| {
            status
                .channel_names
                .get(i)
                .cloned()
                .unwrap_or_else(|| format!("Ch{}", i + 1))
        })
        .collect()
}

fn assess(
    status: &StatusResponse,
    labels: &[String],
    bad_now: &[(String, String)],
    bad_for: impl Fn(&str) -> Option<Duration>,
    stalled_for: Option<Duration>,
    persistent_after: Duration,
) -> DeviceHealth {
    let mut issues = Vec::new();
    let kind = status.device_kind.as_str();

    if let Some(err) = status.device_error.as_deref().filter(|e| !e.is_empty()) {
        issues.push(HealthIssue {
            code: "device_error".into(),
            severity: Severity::Warning,
            message: format!("The device reported an error: {err}."),
            advice: "If it persists, turn the device off and on and reconnect.".into(),
            channels: vec![],
        });
    }

    if let Some(d) = stalled_for.filter(|d| *d >= DATA_STALL_AFTER) {
        issues.push(HealthIssue {
            code: "data_stalled".into(),
            severity: Severity::Critical,
            message: format!("No EEG data has arrived for {}s.", d.as_secs()),
            advice: "Keep the device close to the computer and check it is powered on; \
                     it will reconnect on its own once data resumes."
                .into(),
            channels: vec![],
        });
    }

    let batt = status.battery;
    if batt > 0.0 && batt <= BATTERY_CRITICAL_PCT {
        issues.push(HealthIssue {
            code: "battery_critical".into(),
            severity: Severity::Critical,
            message: format!("Battery is critically low ({batt:.0}%)."),
            advice: "Charge it now — the recording stops when it runs out.".into(),
            channels: vec![],
        });
    } else if batt > 0.0 && batt <= BATTERY_LOW_PCT {
        issues.push(HealthIssue {
            code: "battery_low".into(),
            severity: Severity::Warning,
            message: format!("Battery is low ({batt:.0}%)."),
            advice: "Charge it soon to avoid losing the end of this session.".into(),
            channels: vec![],
        });
    }

    // Only electrodes past the grace period count, so a blink doesn't alarm.
    let settled: Vec<&(String, String)> = bad_now
        .iter()
        .filter(|(l, _)| bad_for(l).is_some_and(|d| d >= BAD_CHANNEL_GRACE))
        .collect();
    let total = labels.len();
    let usable: Vec<String> = labels
        .iter()
        .filter(|l| !settled.iter().any(|(b, _)| b == *l))
        .cloned()
        .collect();

    if total > 0 && usable.is_empty() {
        issues.push(HealthIssue {
            code: "no_usable_channels".into(),
            severity: Severity::Critical,
            message: "No electrode has usable signal.".into(),
            advice: no_contact_advice(kind),
            channels: labels.to_vec(),
        });
    } else if !settled.is_empty() {
        type Bad<'a> = Vec<&'a (String, String)>;
        let (persistent, recent): (Bad, Bad) = settled
            .iter()
            .copied()
            .partition(|(l, _)| bad_for(l).is_some_and(|d| d >= persistent_after));

        // Most of a larger montage going bad together points at the shared
        // reference / ground, not at every electrode separately.
        if total >= 8 && settled.len() * 2 > total {
            issues.push(HealthIssue {
                code: "reference_suspect".into(),
                severity: Severity::Warning,
                message: format!("{} of {total} electrodes lost signal at the same time.", settled.len()),
                advice: "Check the reference and ground electrodes (ear clip / mastoid) and the cable \
                         connection first — they affect every channel."
                    .into(),
                channels: settled.iter().map(|(l, _)| l.clone()).collect(),
            });
        } else {
            let no_signal: Vec<String> = recent
                .iter()
                .filter(|(_, q)| q != "poor")
                .map(|(l, _)| l.clone())
                .collect();
            let poor: Vec<String> = recent
                .iter()
                .filter(|(_, q)| q == "poor")
                .map(|(l, _)| l.clone())
                .collect();
            if !no_signal.is_empty() {
                issues.push(HealthIssue {
                    code: "no_signal".into(),
                    severity: Severity::Warning,
                    message: format!("No signal on {}.", no_signal.join(", ")),
                    advice: electrode_advice(kind, &no_signal),
                    channels: no_signal,
                });
            }
            if !poor.is_empty() {
                issues.push(HealthIssue {
                    code: "poor_contact".into(),
                    severity: Severity::Warning,
                    message: format!("Poor contact on {}.", poor.join(", ")),
                    advice: electrode_advice(kind, &poor),
                    channels: poor,
                });
            }
            if !persistent.is_empty() {
                let names: Vec<String> = persistent.iter().map(|(l, _)| l.clone()).collect();
                issues.push(HealthIssue {
                    code: "faulty_electrodes".into(),
                    severity: Severity::Info,
                    message: format!(
                        "{} electrode{} faulty for over {} ({}); left out of brain-state metrics and embeddings.",
                        names.len(),
                        if names.len() == 1 { " has been" } else { "s have been" },
                        human_duration(persistent_after),
                        names.join(", ")
                    ),
                    advice: electrode_advice(kind, &names),
                    channels: names,
                });
            }
        }
    }

    issues.sort_by_key(|i| std::cmp::Reverse(i.severity));
    DeviceHealth {
        connected: true,
        device_name: status.device_name.clone(),
        issues,
        usable_channels: usable,
        total_channels: total,
        analysed_channels: 0,
        analysis_limit: None,
    }
}

fn human_duration(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 && s % 60 == 0 {
        format!("{} min", s / 60)
    } else {
        format!("{s} s")
    }
}

fn no_contact_advice(kind: &str) -> String {
    if kind == "muse" {
        "Put the headband on: forehead sensors on bare skin above the eyebrows, ear sensors \
         tucked behind the ears touching skin."
            .into()
    } else {
        "Make sure the cap or headset is on and the electrodes touch the scalp; check the \
         reference and ground electrodes and the cable."
            .into()
    }
}

/// What to adjust for these electrodes on this kind of device.
pub fn electrode_advice(kind: &str, channels: &[String]) -> String {
    let ear = |c: &str| matches!(c.to_ascii_uppercase().as_str(), "TP9" | "TP10");
    let forehead = |c: &str| {
        let u = c.to_ascii_uppercase();
        u.starts_with("FP") || u.starts_with("AF")
    };
    if kind == "muse" {
        let mut tips = Vec::new();
        if channels.iter().any(|c| ear(c)) {
            tips.push("tuck hair behind your ears and make sure the ear sensors touch skin");
        }
        if channels.iter().any(|c| forehead(c)) {
            tips.push("move hair off your forehead, wipe or slightly moisten the skin and snug the band");
        }
        if tips.is_empty() {
            tips.push("adjust the headband so every sensor touches skin");
        }
        let mut s = tips.join("; ");
        if let Some(first) = s.get(..1) {
            s = first.to_uppercase() + &s[1..];
        }
        return format!("{s}.");
    }
    if channels.iter().all(|c| forehead(c)) {
        return "Clear hair from the forehead and clean the skin under those electrodes.".into();
    }
    "Re-seat those electrodes: part the hair underneath, add gel or saline if the cap uses it, \
     and check the leads are plugged in."
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(kind: &str, labels: &[&str], quality: &[&str], battery: f32, samples: u64) -> StatusResponse {
        StatusResponse {
            state: "connected".into(),
            device_name: Some("Test".into()),
            device_kind: kind.into(),
            channel_names: labels.iter().map(|s| s.to_string()).collect(),
            channel_quality: quality.iter().map(|s| s.to_string()).collect(),
            battery,
            sample_count: samples,
            ..Default::default()
        }
    }

    const MUSE: [&str; 4] = ["TP9", "AF7", "AF8", "TP10"];

    #[test]
    fn healthy_device_has_no_issues_and_no_notice() {
        let mut t = HealthTracker::new();
        let h = t.observe(&status("muse", &MUSE, &["good"; 4], 80.0, 100), Instant::now());
        assert!(h.is_ok());
        assert_eq!(h.usable_channels.len(), 4);
        assert_eq!(h.notice_markdown(true), None);
        assert_eq!(h.prompt_context(), None);
    }

    #[test]
    fn a_bad_electrode_is_reported_only_after_the_grace_period() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        let q = ["good", "no_signal", "good", "good"];
        assert!(t.observe(&status("muse", &MUSE, &q, 80.0, 100), t0).is_ok());
        let h = t.observe(&status("muse", &MUSE, &q, 80.0, 200), t0 + BAD_CHANNEL_GRACE);
        assert_eq!(h.issues.len(), 1);
        assert_eq!(h.issues[0].code, "no_signal");
        assert_eq!(h.issues[0].channels, ["AF7"]);
        assert!(h.issues[0].advice.contains("forehead"), "{}", h.issues[0].advice);
        assert_eq!(h.usable_channels, ["TP9", "AF8", "TP10"]);
        // Recovery clears it.
        let h = t.observe(
            &status("muse", &MUSE, &["good"; 4], 80.0, 300),
            t0 + Duration::from_secs(9),
        );
        assert!(h.is_ok());
    }

    #[test]
    fn legacy_nosignal_label_counts_as_bad() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        let q = ["nosignal", "good", "good", "good"];
        t.observe(&status("muse", &MUSE, &q, 80.0, 1), t0);
        let h = t.observe(&status("muse", &MUSE, &q, 80.0, 2), t0 + BAD_CHANNEL_GRACE);
        assert_eq!(h.issues[0].channels, ["TP9"]);
        assert!(h.issues[0].advice.contains("ears"));
    }

    #[test]
    fn battery_thresholds() {
        let mut t = HealthTracker::new();
        let now = Instant::now();
        assert!(t.observe(&status("muse", &MUSE, &["good"; 4], 21.0, 1), now).is_ok());
        assert_eq!(
            t.observe(&status("muse", &MUSE, &["good"; 4], 20.0, 2), now).issues[0].code,
            "battery_low"
        );
        let h = t.observe(&status("muse", &MUSE, &["good"; 4], 9.0, 3), now);
        assert_eq!(h.issues[0].code, "battery_critical");
        assert_eq!(h.worst(), Some(Severity::Critical));
        // 0% means "unknown" (LSL streams), not empty.
        assert!(t.observe(&status("lsl", &MUSE, &["good"; 4], 0.0, 4), now).is_ok());
    }

    #[test]
    fn nothing_on_the_head_is_critical_with_wear_advice() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        t.observe(&status("muse", &MUSE, &["poor"; 4], 90.0, 1), t0);
        let h = t.observe(&status("muse", &MUSE, &["poor"; 4], 90.0, 2), t0 + BAD_CHANNEL_GRACE);
        assert_eq!(h.issues[0].code, "no_usable_channels");
        assert_eq!(h.worst(), Some(Severity::Critical));
        assert!(h.issues[0].advice.contains("headband"));
        assert!(h.usable_channels.is_empty());
    }

    #[test]
    fn large_montage_with_persistent_faults_notifies_but_keeps_the_rest_usable() {
        let labels: Vec<String> = (1..=64).map(|i| format!("E{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut q = vec!["good"; 64];
        q[10] = "no_signal";
        q[20] = "poor";
        q[30] = "no_signal";
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        t.observe(&status("openbci", &refs, &q, 0.0, 1), t0);
        let h = t.observe(&status("openbci", &refs, &q, 0.0, 2), t0 + PERSISTENT_FAULT_AFTER);
        let faulty = h
            .issues
            .iter()
            .find(|i| i.code == "faulty_electrodes")
            .expect("persistent");
        assert_eq!(faulty.channels, ["E11", "E21", "E31"]);
        assert_eq!(faulty.severity, Severity::Info);
        assert_eq!(h.usable_channels.len(), 61);
        let notice = h.notice_markdown(false).unwrap();
        assert!(notice.contains("61 of 64"), "{notice}");
        assert!(h.prompt_context().unwrap().contains("Answer the user's question fully"));
    }

    #[test]
    fn most_electrodes_failing_together_points_at_the_reference() {
        let labels: Vec<String> = (1..=16).map(|i| format!("E{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut q = vec!["poor"; 16];
        q[0] = "good";
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        t.observe(&status("openbci", &refs, &q, 0.0, 1), t0);
        let h = t.observe(&status("openbci", &refs, &q, 0.0, 2), t0 + BAD_CHANNEL_GRACE);
        assert_eq!(h.issues[0].code, "reference_suspect");
        assert!(h.issues[0].advice.contains("reference"));
    }

    #[test]
    fn stalled_data_is_critical() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        t.observe(&status("muse", &MUSE, &["good"; 4], 80.0, 500), t0);
        let h = t.observe(&status("muse", &MUSE, &["good"; 4], 80.0, 500), t0 + DATA_STALL_AFTER);
        assert_eq!(h.issues[0].code, "data_stalled");
    }

    #[test]
    fn fingerprint_ignores_the_battery_number() {
        let mut t = HealthTracker::new();
        let now = Instant::now();
        let a = t
            .observe(&status("muse", &MUSE, &["good"; 4], 18.0, 1), now)
            .fingerprint();
        let b = t
            .observe(&status("muse", &MUSE, &["good"; 4], 17.0, 2), now)
            .fingerprint();
        assert_eq!(a, b);
        let c = t
            .observe(&status("muse", &MUSE, &["good"; 4], 9.0, 3), now)
            .fingerprint();
        assert_ne!(a, c);
    }

    #[test]
    fn an_electrode_that_keeps_cutting_out_is_flagged() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        // Read every 3 s like the monitor: AF7 bad on every other reading.
        let mut h = DeviceHealth::default();
        for i in 0..10u64 {
            let q = if i % 2 == 0 { "no_signal" } else { "good" };
            h = t.observe(
                &status("muse", &MUSE, &["good", q, "good", "good"], 80.0, i + 1),
                t0 + Duration::from_secs(3 * i),
            );
        }
        let ic = h
            .issues
            .iter()
            .find(|i| i.code == "intermittent_contact")
            .expect("intermittent");
        assert_eq!(ic.channels, ["AF7"]);
        assert!(ic.advice.contains("loose"), "{}", ic.advice);
        // A single blip in 30 s is not intermittent.
        let mut t = HealthTracker::new();
        for i in 0..10u64 {
            let q = if i == 4 { "poor" } else { "good" };
            h = t.observe(
                &status("muse", &MUSE, &[q, "good", "good", "good"], 80.0, i + 1),
                t0 + Duration::from_secs(3 * i),
            );
        }
        assert!(h.is_ok(), "{:?}", h.issues);
        // A fault that was just fixed (bad, then good for good) is not
        // "intermittent", even with plenty of bad readings in the window.
        let mut t = HealthTracker::new();
        for i in 0..10u64 {
            let q = if i < 6 { "poor" } else { "good" };
            h = t.observe(
                &status("muse", &MUSE, &[q, "good", "good", "good"], 80.0, i + 1),
                t0 + Duration::from_secs(3 * i),
            );
        }
        assert!(
            !h.issues.iter().any(|i| i.code == "intermittent_contact"),
            "{:?}",
            h.issues
        );
    }

    #[test]
    fn mostly_fair_contact_gets_an_improvement_hint_but_stays_usable() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        let mut h = DeviceHealth::default();
        for i in 0..12u64 {
            // AF8 fair two readings out of three; the rest good.
            let q = if i % 3 == 0 { "good" } else { "fair" };
            h = t.observe(
                &status("muse", &MUSE, &["good", "good", q, "good"], 80.0, i + 1),
                t0 + Duration::from_secs(2 * i),
            );
        }
        let fc = h
            .issues
            .iter()
            .find(|i| i.code == "fair_contact")
            .expect("fair_contact");
        assert_eq!(fc.channels, ["AF8"]);
        assert_eq!(fc.severity, Severity::Info);
        assert!(fc.advice.contains("forehead"), "{}", fc.advice);
        assert_eq!(h.usable_channels.len(), 4, "fair electrodes stay in the analysis");
    }

    #[test]
    fn analysis_limit_is_reported_for_large_caps() {
        let labels: Vec<String> = (1..=64).map(|i| format!("E{i}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut q = vec!["good"; 64];
        q[2] = "no_signal"; // inside the analysed 32
        q[40] = "no_signal"; // outside
        let mut t = HealthTracker::new().with_analysis_limit(32);
        let t0 = Instant::now();
        t.observe(&status("lsl", &refs, &q, 0.0, 1), t0);
        let h = t.observe(&status("lsl", &refs, &q, 0.0, 2), t0 + BAD_CHANNEL_GRACE);
        assert_eq!(h.usable_channels.len(), 62);
        assert_eq!(h.analysed_channels, 31);
        let notice = h.notice_markdown(false).unwrap();
        assert!(
            notice.contains("first 32 of 64 electrodes; 31 of those are usable"),
            "{notice}"
        );
    }

    #[test]
    fn disconnected_resets_history() {
        let mut t = HealthTracker::new();
        let t0 = Instant::now();
        let q = ["no_signal", "good", "good", "good"];
        t.observe(&status("muse", &MUSE, &q, 80.0, 1), t0);
        let mut off = status("muse", &MUSE, &q, 80.0, 1);
        off.state = "disconnected".into();
        assert!(!t.observe(&off, t0 + Duration::from_secs(1)).connected);
        // The grace period starts over after reconnecting.
        assert!(t
            .observe(&status("muse", &MUSE, &q, 80.0, 2), t0 + Duration::from_secs(6))
            .is_ok());
    }
}
