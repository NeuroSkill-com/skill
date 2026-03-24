// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Balanced JSON object / array range finders.
//!
//! These scan raw text for top-level `{…}` and `[…]` ranges while correctly
//! handling string escapes, so that JSON blobs embedded in prose can be
//! extracted without a full parser.

/// Find all top-level balanced `{…}` ranges in `content`.
///
/// Returns a vec of `(start, end)` byte offsets (end is exclusive).
pub fn find_balanced_json_objects(content: &str) -> Vec<(usize, usize)> {
    find_balanced(content, b'{', b'}')
}

/// Find all top-level balanced `[…]` ranges in `content`.
///
/// Returns a vec of `(start, end)` byte offsets (end is exclusive).
pub fn find_balanced_json_arrays(content: &str) -> Vec<(usize, usize)> {
    find_balanced(content, b'[', b']')
}

fn find_balanced(content: &str, open: u8, close: u8) -> Vec<(usize, usize)> {
    let bytes = content.as_bytes();
    let mut out = Vec::new();

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut start = None::<usize>;

    for (i, &b) in bytes.iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match b {
            b'"' => in_string = true,
            b if b == open => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b if b == close => {
                if depth == 0 { continue }
                depth -= 1;
                if depth == 0 {
                    if let Some(s) = start.take() {
                        out.push((s, i + 1));
                    }
                }
            }
            _ => {}
        }
    }

    out
}
