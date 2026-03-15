// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Shared utilities.

use std::sync::MutexGuard;

/// Extension trait for `std::sync::Mutex` that recovers from poison.
pub trait MutexExt<T> {
    /// Acquire the lock, recovering the guard even if the mutex is poisoned.
    fn lock_or_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    #[inline]
    fn lock_or_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poison| {
            eprintln!("[skill-data] Mutex was poisoned — recovering");
            poison.into_inner()
        })
    }
}
