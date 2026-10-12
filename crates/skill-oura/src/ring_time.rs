// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Ring deciseconds → wall clock.
//!
//! A history event is stamped with the ring's own counter in **deciseconds**,
//! not Unix time, and that counter **restarts from near zero on every reboot**.
//! Turning one into a date therefore needs anchors: `time_sync` (`0x42`) and
//! `rtc_beacon` (`0x85`) events carry a `unix_time`, and everything else is
//! interpolated from the nearest one inside the same boot.
//!
//! Some timestamps cannot be dated at all, and this module says so rather than
//! guessing — a wrong date in the health database is worse than a missing one:
//!
//! * **No anchor in the boot** that holds the timestamp. The only time available
//!   is when we downloaded it, which is not when it happened.
//! * **An accelerated counter.** A fresh ring's first days ran the counter
//!   erratically (weeks of deciseconds inside an hour), so two anchors disagree
//!   about its rate and nothing between them has a calendar day.
//! * **An ambiguous reboot stall.** The counter lost time between two anchors
//!   (the ring sat off the finger) and more than one `ring_start` falls in the
//!   gap, so the lost time cannot be attributed to a particular reboot.
//!
//! This is a port of `tools/epoch_time.py` from [open_oura], which upstream also
//! mirrors in `oura-summary/src/ring_time.rs` and in `EventStore.RingClock` on
//! iOS.  The constants and the branch order are kept identical on purpose: they
//! were derived from real multi-boot captures, and this is not a place to
//! improvise.
//!
//! [open_oura]: https://github.com/Th0rgal/open_oura

use oura_protocol::events::RingEvent;

/// How far below a boot's lowest timestamp a value may still belong to it.
///
/// Six hours: events arrive out of counter order within one capture second, so a
/// boot's range has to tolerate a backwards step without being split in two.
const EPOCH_RESET_SLACK_DS: i64 = 6 * 3600 * 10;

/// How far past the moment of capture a computed date may land before it is
/// treated as impossible.  Nothing the ring recorded happened after we read it.
const FUTURE_SLACK_S: i64 = 6 * 3600;

/// Absolute slack when checking that two anchors of one boot agree on the
/// counter's rate.
const ANCHOR_AGREEMENT_S: f64 = 30.0 * 60.0;

/// Relative slack for the same check, for long gaps where drift accumulates.
const ANCHOR_AGREEMENT_FRACTION: f64 = 0.02;

/// `time_sync` — a `unix_time` the phone pushed to the ring.
const TAG_TIME_SYNC: u8 = 0x42;
/// `rtc_beacon` — the ring's own 1-second wall-clock anchor.
const TAG_RTC_BEACON: u8 = 0x85;
/// `ring_start` — a boot record, i.e. the counter restarting.
const TAG_RING_START: u8 = 0x41;

/// One `(counter, wall-clock)` pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Anchor {
    ds: i64,
    unix: i64,
}

/// One boot of the ring: a contiguous run of the counter.
#[derive(Clone, Debug, Default)]
struct Epoch {
    min_ds: i64,
    max_ds: i64,
    /// Capture time of the highest timestamp seen — the fallback clock.
    fallback_unix: i64,
    capture_min: i64,
    capture_max: i64,
    anchors: Vec<Anchor>,
    /// Counter values at which a `ring_start` was recorded.
    boots: Vec<i64>,
}

/// Why a timestamp has no calendar date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Undated {
    /// The boot holding it recorded no `time_sync` or `rtc_beacon`.
    MissingAnchor,
    /// Its boot's anchors disagree about the counter's rate.
    AcceleratedCounter,
    /// The counter stalled across more than one reboot; the lost time cannot be
    /// attributed to one of them.
    AmbiguousRebootStall,
}

/// How the anchors on either side of a timestamp relate.
enum Bracket {
    /// The counter kept wall-clock rate between them (or there is nothing to compare).
    Consistent,
    /// The counter lost time between them — the ring was off.  The later
    /// anchor's offset applies from the stall onwards.
    Stalled { before: Anchor, after: Anchor },
    /// The counter ran *faster* than wall time; nothing between them is datable.
    Erratic,
}

/// Dates ring timestamps against the boots and anchors in a batch of events.
#[derive(Clone, Debug, Default)]
pub struct RingClock {
    epochs: Vec<Epoch>,
}

impl RingClock {
    /// Build the boot/anchor model from events **in capture order**.
    ///
    /// Order matters and must not be "fixed" by sorting on the timestamp:
    /// thousands of events share one capture second, and sorting them by counter
    /// would erase the backwards jump that marks a reboot.
    pub fn build(events: &[(RingEvent, i64)]) -> Self {
        let mut epochs: Vec<Epoch> = Vec::new();

        for (ev, captured_unix) in events {
            let ds = i64::from(ev.timestamp);
            let cu = *captured_unix;

            let continues_last = epochs.last().is_some_and(|e| ds >= e.max_ds - EPOCH_RESET_SLACK_DS);

            if continues_last {
                // `last()` is Some: `continues_last` is false for an empty list.
                if let Some(e) = epochs.last_mut() {
                    if ds >= e.max_ds {
                        e.max_ds = ds;
                        e.fallback_unix = cu;
                    }
                    e.min_ds = e.min_ds.min(ds);
                    e.capture_min = e.capture_min.min(cu);
                    e.capture_max = e.capture_max.max(cu);
                }
            } else {
                epochs.push(Epoch {
                    min_ds: ds,
                    max_ds: ds,
                    fallback_unix: cu,
                    capture_min: cu,
                    capture_max: cu,
                    anchors: Vec::new(),
                    boots: Vec::new(),
                });
            }

            let Some(current) = epochs.last_mut() else {
                continue;
            };
            if ev.tag == TAG_RING_START {
                current.boots.push(ds);
            }
            if matches!(ev.tag, TAG_TIME_SYNC | TAG_RTC_BEACON) {
                if let Some(unix) = ev
                    .decoded
                    .as_ref()
                    .and_then(|d| d.get("unix_time"))
                    .and_then(serde_json::Value::as_i64)
                {
                    current.anchors.push(Anchor { ds, unix });
                }
            }
        }

        Self { epochs }
    }

    /// Whether any boot was observed at all.
    pub fn is_empty(&self) -> bool {
        self.epochs.is_empty()
    }

    /// How many anchors were found across all boots.
    ///
    /// Zero means nothing can be dated, which is worth telling the user: it
    /// usually means the ring has not been near the official app (the source of
    /// `time_sync`) and is too old to emit `rtc_beacon`.
    pub fn anchor_count(&self) -> usize {
        self.epochs.iter().map(|e| e.anchors.len()).sum()
    }

    /// The boot a timestamp most likely belongs to.
    ///
    /// Chosen by closeness in *capture* time, not counter value: two boots can
    /// cover the same counter range, and the download that produced the event is
    /// the tiebreaker.
    fn epoch_for(&self, ds: i64, captured_unix: Option<i64>) -> Option<&Epoch> {
        let candidates: Vec<&Epoch> = self
            .epochs
            .iter()
            .filter(|e| (e.min_ds - EPOCH_RESET_SLACK_DS..=e.max_ds + EPOCH_RESET_SLACK_DS).contains(&ds))
            .collect();

        match (captured_unix, candidates.is_empty()) {
            (Some(cu), false) => candidates.into_iter().min_by_key(|e| capture_distance(e, cu)),
            // Without a capture time, prefer the tightest boot range.
            (None, false) => candidates.into_iter().min_by_key(|e| e.max_ds - e.min_ds),
            // Outside every boot's range: the most recent boot extrapolates.
            (_, true) => self.epochs.last(),
        }
    }

    /// Date a ring timestamp, or explain why it cannot be dated.
    pub fn undated_reason(&self, ds: i64, captured_unix: i64) -> Option<Undated> {
        let candidates_exist = self
            .epochs
            .iter()
            .any(|e| (e.min_ds - EPOCH_RESET_SLACK_DS..=e.max_ds + EPOCH_RESET_SLACK_DS).contains(&ds));
        if !candidates_exist {
            return Some(Undated::MissingAnchor);
        }
        let Some(e) = self.epoch_for(ds, Some(captured_unix)) else {
            return Some(Undated::MissingAnchor);
        };
        if e.anchors.is_empty() {
            return Some(Undated::MissingAnchor);
        }

        match bracket(&e.anchors, ds) {
            Bracket::Erratic => Some(Undated::AcceleratedCounter),
            Bracket::Stalled { before, after } => {
                let boots: Vec<i64> = e
                    .boots
                    .iter()
                    .copied()
                    .filter(|b| before.ds < *b && *b <= after.ds)
                    .collect();
                let ambiguous = boots
                    .iter()
                    .min()
                    .zip(boots.iter().max())
                    .is_some_and(|(lo, hi)| *lo <= ds && ds < *hi);
                ambiguous.then_some(Undated::AmbiguousRebootStall)
            }
            Bracket::Consistent => {
                let nearest = nearest_anchor(&e.anchors, ds)?;
                if predicted(nearest, ds) <= (captured_unix + FUTURE_SLACK_S) as f64 {
                    return None;
                }
                // The nearest anchor puts this in the future, but another boot's
                // anchor may still place it plausibly.
                let borrowable = self.epochs.iter().any(|epoch| {
                    (epoch.min_ds - EPOCH_RESET_SLACK_DS..=epoch.max_ds + EPOCH_RESET_SLACK_DS).contains(&ds)
                        && epoch
                            .anchors
                            .iter()
                            .any(|a| predicted(*a, ds) <= (captured_unix + FUTURE_SLACK_S) as f64)
                });
                (!borrowable).then_some(Undated::AcceleratedCounter)
            }
        }
    }

    /// Wall-clock seconds for a ring timestamp, dated or not.
    ///
    /// Prefer [`RingClock::dated_unix_s`] when the result is going to be stored:
    /// this falls back to extrapolating from the capture time, which is only ever
    /// an approximation of when the ring actually recorded the sample.
    pub fn unix_s(&self, ds: i64, captured_unix: Option<i64>) -> Option<f64> {
        let e = self.epoch_for(ds, captured_unix)?;
        let future_limit = captured_unix.map(|cu| (cu + FUTURE_SLACK_S) as f64);

        if !e.anchors.is_empty() {
            let nearest = nearest_anchor(&e.anchors, ds)?;
            let predicted_s = predicted(nearest, ds);

            match bracket(&e.anchors, ds) {
                Bracket::Stalled { before, after } => {
                    let late = after.unix as f64 - (after.ds - ds) as f64 / 10.0;
                    let early = before.unix as f64 + (ds - before.ds) as f64 / 10.0;
                    let boots: Vec<i64> = e
                        .boots
                        .iter()
                        .copied()
                        .filter(|b| before.ds < *b && *b <= after.ds)
                        .collect();
                    if let (Some(lo), Some(hi)) = (boots.iter().min(), boots.iter().max()) {
                        if *lo <= ds && ds < *hi {
                            // Lost time cannot be assigned to one of several reboots.
                            return None;
                        }
                        return Some(if ds >= *hi { late } else { early });
                    }
                    return Some(match future_limit {
                        Some(limit) if late > limit => early,
                        _ => late,
                    });
                }
                Bracket::Consistent => {
                    if future_limit.is_none_or(|limit| predicted_s <= limit) {
                        return Some(predicted_s);
                    }
                }
                // Undatable, but still the best estimate available; callers that
                // care check `undated_reason` first.
                Bracket::Erratic => return Some(predicted_s),
            }

            if let Some(limit) = future_limit {
                // Only borrow another boot's clock when this counter value could
                // belong to it: a rebooted ring restarts near zero and must not
                // be projected through an older boot that only ran at higher counts.
                let plausible = self
                    .epochs
                    .iter()
                    .filter(|epoch| {
                        (epoch.min_ds - EPOCH_RESET_SLACK_DS..=epoch.max_ds + EPOCH_RESET_SLACK_DS).contains(&ds)
                    })
                    .flat_map(|epoch| epoch.anchors.iter())
                    .map(|a| predicted(*a, ds))
                    .filter(|p| *p <= limit)
                    .fold(f64::NEG_INFINITY, f64::max);
                if plausible.is_finite() {
                    return Some(plausible);
                }
            }
        }

        let fallback = e.fallback_unix as f64 - (e.max_ds - ds) as f64 / 10.0;
        Some(match future_limit {
            Some(limit) => fallback.min(limit),
            None => fallback,
        })
    }

    /// Wall-clock seconds, but only when the timestamp really is datable.
    ///
    /// This is the one to use before writing to the health database.
    pub fn dated_unix_s(&self, ds: i64, captured_unix: i64) -> Option<f64> {
        if self.undated_reason(ds, captured_unix).is_some() {
            return None;
        }
        self.unix_s(ds, Some(captured_unix))
    }
}

/// How far a capture time falls outside a boot's capture window (0 when inside).
fn capture_distance(e: &Epoch, captured_unix: i64) -> i64 {
    if captured_unix < e.capture_min {
        e.capture_min - captured_unix
    } else if captured_unix > e.capture_max {
        captured_unix - e.capture_max
    } else {
        0
    }
}

fn predicted(anchor: Anchor, ds: i64) -> f64 {
    anchor.unix as f64 + (ds - anchor.ds) as f64 / 10.0
}

fn nearest_anchor(anchors: &[Anchor], ds: i64) -> Option<Anchor> {
    anchors.iter().copied().min_by_key(|a| (a.ds - ds).abs())
}

/// Compare the counter's rate against wall time between the anchors bracketing `ds`.
fn bracket(anchors: &[Anchor], ds: i64) -> Bracket {
    let mut sorted: Vec<Anchor> = anchors.to_vec();
    sorted.sort_by_key(|a| (a.ds, a.unix));

    let idx = sorted.iter().position(|a| a.ds >= ds).unwrap_or(sorted.len());
    if idx == 0 || idx >= sorted.len() {
        // Outside the anchored range: the nearest anchor extrapolates as usual.
        return Bracket::Consistent;
    }
    let (before, after) = (sorted[idx - 1], sorted[idx]);
    if after.ds == ds {
        return Bracket::Consistent;
    }

    let wall_s = (after.unix - before.unix) as f64;
    let counter_s = (after.ds - before.ds) as f64 / 10.0;
    let tolerance = ANCHOR_AGREEMENT_S.max(counter_s * ANCHOR_AGREEMENT_FRACTION);

    if wall_s < counter_s - tolerance {
        Bracket::Erratic
    } else if wall_s > counter_s + tolerance {
        Bracket::Stalled { before, after }
    } else {
        Bracket::Consistent
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Build an event with a tag, a counter value and an optional decoded body.
    fn ev(tag: u8, ds: u32, decoded: Option<serde_json::Value>) -> RingEvent {
        RingEvent {
            tag,
            name: oura_protocol::events::event_name(tag),
            timestamp: ds,
            body: Vec::new(),
            decoded,
        }
    }

    fn anchor_ev(ds: u32, unix: i64) -> RingEvent {
        ev(TAG_TIME_SYNC, ds, Some(serde_json::json!({ "unix_time": unix })))
    }

    /// A sample 100 s of counter after an anchor is 100 s of wall clock after it.
    #[test]
    fn interpolates_from_a_single_anchor() {
        let events = vec![
            (anchor_ev(10_000, 1_700_000_000), 1_700_000_100),
            (ev(0x46, 11_000, None), 1_700_000_100),
        ];
        let clock = RingClock::build(&events);

        assert_eq!(clock.anchor_count(), 1);
        let dated = clock.dated_unix_s(11_000, 1_700_000_100).unwrap();
        assert!((dated - 1_700_000_100.0).abs() < 0.001, "got {dated}");
    }

    /// Before the anchor, too — the counter runs backwards just as evenly.
    #[test]
    fn extrapolates_backwards_from_an_anchor() {
        let events = vec![(anchor_ev(10_000, 1_700_000_000), 1_700_000_000)];
        let clock = RingClock::build(&events);
        let dated = clock.dated_unix_s(9_000, 1_700_000_000).unwrap();
        assert!((dated - 1_699_999_900.0).abs() < 0.001, "got {dated}");
    }

    /// No anchor anywhere: nothing is datable, and we must say so rather than
    /// handing back the download time dressed up as a measurement time.
    #[test]
    fn a_boot_without_an_anchor_is_undated() {
        let events = vec![(ev(0x46, 500, None), 1_700_000_000)];
        let clock = RingClock::build(&events);

        assert_eq!(clock.undated_reason(500, 1_700_000_000), Some(Undated::MissingAnchor));
        assert!(clock.dated_unix_s(500, 1_700_000_000).is_none());
        // The estimate still exists for callers that only want a rough ordering.
        assert!(clock.unix_s(500, Some(1_700_000_000)).is_some());
    }

    /// A reboot restarts the counter near zero.  That backwards jump must open a
    /// new boot, not collapse into the previous one.
    #[test]
    fn a_reboot_starts_a_new_epoch() {
        let events = vec![
            (anchor_ev(10_000_000, 1_700_000_000), 1_700_000_000),
            // Counter restarts: more than the slack below the previous max.
            (ev(TAG_RING_START, 10, None), 1_700_100_000),
            (anchor_ev(100, 1_700_100_000), 1_700_100_000),
        ];
        let clock = RingClock::build(&events);
        assert_eq!(clock.epochs.len(), 2, "reboot must open a second epoch");

        // The post-reboot sample is dated from the post-reboot anchor, not
        // projected through the old boot's much larger counter.
        let dated = clock.dated_unix_s(200, 1_700_100_000).unwrap();
        assert!((dated - 1_700_100_010.0).abs() < 0.001, "got {dated}");
    }

    /// Two anchors of one boot that disagree about the counter's rate — the
    /// counter ran faster than wall time — make everything between them undatable.
    #[test]
    fn an_accelerated_counter_is_undated() {
        // 1 000 000 ds = 100 000 s of counter, but only 100 s of wall clock.
        let events = vec![
            (anchor_ev(0, 1_700_000_000), 1_700_000_000),
            (ev(0x46, 500_000, None), 1_700_000_100),
            (anchor_ev(1_000_000, 1_700_000_100), 1_700_000_100),
        ];
        let clock = RingClock::build(&events);
        assert_eq!(
            clock.undated_reason(500_000, 1_700_000_100),
            Some(Undated::AcceleratedCounter)
        );
        assert!(clock.dated_unix_s(500_000, 1_700_000_100).is_none());
    }

    /// The ring sat off the finger: wall clock advanced far more than the counter.
    /// With no reboot in the gap the later anchor's offset applies, and the
    /// sample is still datable.
    #[test]
    fn a_stall_without_a_reboot_still_dates() {
        // 1000 ds = 100 s of counter across 10 000 s of wall clock.
        let events = vec![
            (anchor_ev(0, 1_700_000_000), 1_700_000_000),
            (ev(0x46, 500, None), 1_700_010_000),
            (anchor_ev(1_000, 1_700_010_000), 1_700_010_000),
        ];
        let clock = RingClock::build(&events);
        assert_eq!(clock.undated_reason(500, 1_700_010_000), None);
        // Dated off the later anchor: 50 s of counter before it.
        let dated = clock.dated_unix_s(500, 1_700_010_000).unwrap();
        assert!((dated - 1_700_009_950.0).abs() < 0.001, "got {dated}");
    }

    /// Several reboots inside a stall: the lost time cannot be attributed to any
    /// one of them, so a timestamp between the first and last is undatable.
    #[test]
    fn a_stall_spanning_several_reboots_is_ambiguous() {
        let events = vec![
            (anchor_ev(0, 1_700_000_000), 1_700_000_000),
            (ev(TAG_RING_START, 200, None), 1_700_010_000),
            (ev(0x46, 400, None), 1_700_010_000),
            (ev(TAG_RING_START, 600, None), 1_700_010_000),
            (anchor_ev(1_000, 1_700_010_000), 1_700_010_000),
        ];
        let clock = RingClock::build(&events);
        assert_eq!(
            clock.undated_reason(400, 1_700_010_000),
            Some(Undated::AmbiguousRebootStall)
        );
        assert!(clock.dated_unix_s(400, 1_700_010_000).is_none());
    }

    /// A date later than the download cannot be right — nothing the ring
    /// recorded happened after we read it.
    #[test]
    fn a_date_past_the_capture_time_is_rejected() {
        let events = vec![(anchor_ev(0, 1_700_000_000), 1_700_000_000)];
        let clock = RingClock::build(&events);
        // 10 000 000 ds = ~11.6 days after the anchor, captured immediately.
        assert!(clock.dated_unix_s(10_000_000, 1_700_000_000).is_none());
    }

    #[test]
    fn an_empty_batch_has_no_clock() {
        let clock = RingClock::build(&[]);
        assert!(clock.is_empty());
        assert_eq!(clock.anchor_count(), 0);
        assert!(clock.unix_s(1_000, Some(1_700_000_000)).is_none());
        assert!(clock.dated_unix_s(1_000, 1_700_000_000).is_none());
    }
}
