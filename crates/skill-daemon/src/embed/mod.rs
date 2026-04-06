// SPDX-License-Identifier: GPL-3.0-only
//! EEG epoch accumulator → embedding encoder → per-day HNSW + SQLite storage.
//!
//! Runs in a background thread spawned by the session runner.  Receives
//! epoch buffers (5s × N channels) via a bounded channel, encodes them with
//! the configured backend (ZUNA, LUNA, NeuroRVQ, …), and stores the resulting
//! embedding vectors in per-day SQLite + HNSW files.

mod accumulator;
mod day_store;
mod worker;

pub(crate) use accumulator::EpochAccumulator;
pub(crate) use worker::EmbedWorkerHandle;
