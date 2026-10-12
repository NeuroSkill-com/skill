// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Draining the ring's history-event stream.
//!
//! This is the *fetch* half of the cloud-free history sync: it opens a link,
//! authenticates, and pulls stored events from a cursor. Turning those events
//! into health records is `skill-oura`'s job (`skill_oura::events_to_payload`),
//! which is pure and unit-tested; nothing here interprets a body.
//!
//! ## The cursor
//!
//! The ring is a ring buffer with a read position measured in its own
//! deciseconds. Draining from a stored cursor is what makes a sync incremental
//! rather than re-downloading months of events every time, and the cursor is
//! only advanced once the events before it have been handed back to the caller
//! successfully — a dropped link loses the tail of a batch, not the progress.
//!
//! ## One link at a time
//!
//! A ring accepts a single peripheral connection. A drain therefore cannot run
//! while a live session holds the ring, and the caller is expected to check
//! that before calling (the daemon refuses with an actionable error). Opening a
//! second transport to the same device would have both subscribed to the same
//! notifications, and since the protocol matches a response by being the next
//! packet with the right tag, the two would steal each other's answers.

use std::time::Duration;

use oura_protocol::events::RingEvent;
use tracing::{debug, info, warn};

use super::transport::WebBluetoothTransport;
use super::OuraConfig;
use oura_link::OuraClient;

/// What one drain produced.
#[derive(Debug, Default)]
pub struct HistorySync {
    /// Events in **capture order**, each paired with the Unix second it was
    /// read at. The order is load-bearing: the clock model in `skill-oura`
    /// detects reboots from a backwards step in the counter, which sorting by
    /// timestamp would erase.
    pub events: Vec<(RingEvent, i64)>,
    /// Where the next sync should resume from.
    pub next_cursor: u32,
    /// How many events the ring reported sending.
    pub events_synced: u32,
    /// Whether `max_events` cut the drain short, so more remain on the ring.
    pub truncated: bool,
}

/// Drain history events from `cursor`, collecting at most `max_events`.
///
/// `max_events` bounds memory: a ring that has not been synced in months holds
/// hundreds of thousands of events, and the caller converts the whole batch in
/// one go. Hitting the bound sets [`HistorySync::truncated`] and still returns a
/// usable cursor, so the next call picks up where this one stopped.
pub async fn drain_history(cfg: &OuraConfig, cursor: u32, max_events: usize) -> anyhow::Result<HistorySync> {
    let key = cfg.auth_key.ok_or_else(|| {
        anyhow::anyhow!(
            "Oura ring auth key not configured. History events are app-auth gated; set the \
             ring's key in Settings → Devices → Device API (32 hex characters)."
        )
    })?;

    let transport =
        WebBluetoothTransport::connect(&cfg.name_contains, cfg.device_id.as_deref(), cfg.scan_timeout).await?;
    let client = OuraClient::new(transport);

    client.authenticate(&key).await?;

    // The ring dates its own history from its clock, so aligning it first is
    // what lets anything recorded from here on be placed on a calendar. Events
    // already stored keep whatever clock they were written with.
    if let Err(e) = client.sync_time_app().await {
        debug!(error = %e, "Oura time sync declined before drain");
    }
    // Ask the ring to postprocess sleep so the hypnogram and bedtime events for
    // last night exist before we read them.
    if let Err(e) = client.check_sleep_analysis(false).await {
        debug!(error = %e, "Oura sleep postprocess declined");
    }

    let mut events: Vec<(RingEvent, i64)> = Vec::new();
    let mut truncated = false;

    let outcome = client
        .drain_events(
            cursor,
            |ev| {
                if events.len() >= max_events {
                    truncated = true;
                    // Returning false aborts the drain without acknowledging the
                    // batch, so the cursor stays where it was and nothing is lost.
                    return false;
                }
                events.push((ev.clone(), now_unix_secs()));
                true
            },
            |progress| {
                debug!(
                    cursor = progress.next_cursor,
                    bytes_left = progress.bytes_left,
                    synced = progress.events_synced,
                    "Oura drain progress"
                );
                true
            },
        )
        .await;

    match outcome {
        Ok(outcome) => {
            info!(
                events = events.len(),
                synced = outcome.events_synced,
                next_cursor = outcome.next_cursor,
                "Oura history drain complete"
            );
            client.transport().disconnect().await;
            Ok(HistorySync {
                events,
                next_cursor: outcome.next_cursor,
                events_synced: outcome.events_synced,
                truncated,
            })
        }
        // A drain that stopped because we hit `max_events` is a success with
        // more to come, not a failure — we keep what arrived and resume later.
        Err(e) if truncated => {
            warn!(error = %e, events = events.len(), "Oura drain stopped at the event cap");
            client.transport().disconnect().await;
            Ok(HistorySync {
                events,
                next_cursor: cursor,
                events_synced: 0,
                truncated: true,
            })
        }
        Err(e) => {
            client.transport().disconnect().await;
            Err(e.into())
        }
    }
}

fn now_unix_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Default cap on events pulled in one sync.
///
/// Roughly a few days of a worn ring. Large enough that a routine daily sync is
/// never truncated, small enough that a months-long backlog is drained over
/// several calls instead of in one multi-hundred-megabyte allocation.
pub const DEFAULT_MAX_EVENTS: usize = 50_000;

/// How long to wait for a ring before giving up on a history sync.
///
/// Longer than a live connect's window: a sync is a deliberate, user-initiated
/// action, and a ring that is charging or has just been taken off advertises
/// less often.
pub const SYNC_SCAN_TIMEOUT: Duration = Duration::from_secs(15);
