// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Disk cache for session metrics — avoids recomputing from CSV on every load.

use std::collections::HashMap;
use std::path::Path;

// metrics_csv_path kept for backward compat if needed; find_metrics_path handles both formats.
use skill_data::util::{unix_to_ts, ts_to_unix};

use super::{
    CsvMetricsResult, SessionMetrics, EpochRow,
    SleepStages, SleepEpoch, SleepSummary,
    load_metrics_csv, find_metrics_path,
};

