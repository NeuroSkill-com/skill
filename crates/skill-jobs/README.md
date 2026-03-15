# skill-jobs

Sequential job queue for expensive compute tasks.

## Overview

A single-threaded, FIFO job queue designed for long-running GPU or CPU-bound work (model downloads, embedding rebuilds, UMAP projections, etc.). Jobs run one at a time on a dedicated background thread. Callers receive a `JobTicket` and can poll for progress or final results.

## Key types

| Type | Description |
|---|---|
| `JobQueue` | Thread-safe queue (`Arc`-wrapped). `submit()` enqueues work and returns a ticket. |
| `JobTicket` | Handle containing the job ID for subsequent polling |
| `JobProgress` | Progress snapshot: estimated/elapsed milliseconds, optional status message |
| `JobPollResult` | `Pending(JobProgress)` / `Done(Value)` / `NotFound` |

## Key methods

| Method | Description |
|---|---|
| `JobQueue::new()` | Create a new queue (spawns the worker thread) |
| `submit(estimated_ms, work)` | Enqueue a closure; returns `JobTicket` |
| `submit_with_id(estimated_ms, work)` | Same, but the closure receives its own job ID |
| `poll(job_id)` | Check job status without blocking |
| `progress_map()` | Access the shared progress map for in-flight updates |
| `stats()` | JSON snapshot of queue depth and in-flight count |

## Dependencies

- `serde` / `serde_json` — serialization of progress and results
