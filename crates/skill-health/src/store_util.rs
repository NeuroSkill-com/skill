// SPDX-License-Identifier: GPL-3.0-only
// Mutex helper for HealthStore

use std::sync::{Mutex, MutexGuard, PoisonError};

pub fn lock_or_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
