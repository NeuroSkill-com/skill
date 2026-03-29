// SPDX-License-Identifier: GPL-3.0-only
//! Tests for tool output truncation.

use skill_tools::exec::truncate_text;

#[test]
fn truncate_short_string_unchanged() {
    assert_eq!(truncate_text("hello", 100), "hello");
}

#[test]
fn truncate_at_limit() {
    assert_eq!(truncate_text("abcde", 3), "abc");
}

#[test]
fn truncate_empty() {
    assert_eq!(truncate_text("", 10), "");
}

#[test]
fn truncate_multibyte_chars() {
    // Emoji: each takes 1 char but multiple bytes
    let s = "🎉🎊🎈🎁";
    let result = truncate_text(s, 2);
    assert_eq!(result, "🎉🎊");
}

#[test]
fn truncate_zero_limit() {
    assert_eq!(truncate_text("anything", 0), "");
}
