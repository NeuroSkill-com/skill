# Overview

## Live Streaming
{app} streams derived EEG metrics and device status over a local WebSocket server. Broadcast events include: eeg-bands (~4 Hz — 60+ scores), device-status (~1 Hz — battery, connection state), and label-created. Raw EEG/PPG/IMU samples are not available over the WebSocket API. The service is advertised via Bonjour/mDNS as _skill._tcp so clients can discover it automatically.

## Commands
Clients can send JSON commands over the WebSocket: status (full system snapshot), calibrate (open calibration), label (submit an annotation), search (nearest-neighbour query), sessions (list recordings), compare (A/B metrics + sleep + UMAP), sleep (sleep staging), umap/umap_poll (3D embedding projection). Responses arrive on the same connection as JSON with an "ok" boolean.

# Command Reference

## status
_(none)_

Returns device state, session info, embedding counts (today & all-time), label count, last calibration timestamp, and per-channel signal quality.

## calibrate
_(none)_

Opens the calibration window. Requires a connected, streaming device.

## label
text (string, required); label_start_utc (u64, optional — defaults to now)

Inserts a timestamped label into the label database. Returns the new label_id.

## search
start_utc, end_utc (u64, required); k, ef (u64, optional)

Searches the HNSW embedding index for the k nearest neighbours within the given time range.

## compare
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, required)

Compares two time ranges by returning aggregated band-power metrics (relative powers, relaxation/engagement scores, and FAA) for each. Returns { a: SessionMetrics, b: SessionMetrics }.

## sessions
_(none)_

Lists all embedding sessions discovered from the daily eeg.sqlite databases. Sessions are contiguous recording ranges (gap > 2 min = new session). Returns newest first.

## sleep
start_utc, end_utc (u64, required)

Classifies each embedding epoch in the time range into a sleep stage (Wake/N1/N2/N3/REM) using band-power ratios and returns a hypnogram with per-stage summary.

## umap
a_start_utc, a_end_utc, b_start_utc, b_end_utc (u64, required)

Enqueues a 3D UMAP projection of embeddings from two sessions. Returns a job_id for polling. Non-blocking.

## umap_poll
job_id (string, required)

Polls for the result of a previously enqueued UMAP job. Returns { status: 'pending' | 'done', points?: [...] }.

## say
text: string (required)

Speak text via on-device TTS. Fire-and-forget — returns immediately while audio plays in the background. Initialises the TTS engine on first call.
