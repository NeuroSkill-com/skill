// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Property-based tests for tool-call parsing and JSON scanning.

#[cfg(test)]
mod tests {
    use crate::parse::extract::extract_tool_calls;
    use crate::parse::json_scan::{find_balanced_json_arrays, find_balanced_json_objects};
    use proptest::prelude::*;

    // ── JSON scanner properties ──────────────────────────────────────────────

    proptest! {
        /// Every range returned by `find_balanced_json_objects` must start with
        /// `{` and end with `}`, and the slice must be valid UTF-8.
        #[test]
        fn json_objects_have_correct_delimiters(s in ".*") {
            for (start, end) in find_balanced_json_objects(&s) {
                let slice = &s[start..end];
                prop_assert!(slice.starts_with('{'), "expected '{{' at start, got {:?}", &slice[..1.min(slice.len())]);
                prop_assert!(slice.ends_with('}'), "expected '}}' at end, got {:?}", &slice[slice.len().saturating_sub(1)..]);
            }
        }

        /// Every range returned by `find_balanced_json_arrays` must start with
        /// `[` and end with `]`.
        #[test]
        fn json_arrays_have_correct_delimiters(s in ".*") {
            for (start, end) in find_balanced_json_arrays(&s) {
                let slice = &s[start..end];
                prop_assert!(slice.starts_with('['));
                prop_assert!(slice.ends_with(']'));
            }
        }

        /// Ranges must not overlap and must be in order.
        #[test]
        fn json_object_ranges_are_non_overlapping(s in ".*") {
            let ranges = find_balanced_json_objects(&s);
            for window in ranges.windows(2) {
                prop_assert!(
                    window[0].1 <= window[1].0,
                    "overlapping ranges: {:?} and {:?}", window[0], window[1]
                );
            }
        }

        /// Ranges must fit within the input string.
        #[test]
        fn json_object_ranges_are_within_bounds(s in ".*") {
            let len = s.len();
            for (start, end) in find_balanced_json_objects(&s) {
                prop_assert!(start < len);
                prop_assert!(end <= len);
                prop_assert!(start < end);
            }
        }
    }

    // ── Tool-call extraction properties ──────────────────────────────────────

    proptest! {
        /// extract_tool_calls must never panic on arbitrary input.
        #[test]
        fn extract_tool_calls_never_panics(s in "\\PC{0,2000}") {
            let _ = extract_tool_calls(&s);
        }

        /// Tool calls extracted from valid [TOOL_CALL] blocks must have a name.
        #[test]
        fn extracted_tool_calls_have_names(
            name in "[a-z_]{1,20}",
            args in "\\{[^}]{0,100}\\}"
        ) {
            let input = format!("[TOOL_CALL]{{\"name\":\"{name}\",\"arguments\":{args}}}[/TOOL_CALL]");
            let calls = extract_tool_calls(&input);
            for call in &calls {
                prop_assert!(!call.function.name.is_empty());
            }
        }

        /// A valid JSON tool call wrapped in delimiters must be extracted.
        #[test]
        fn delimited_json_tool_call_is_extracted(
            name in "[a-z_]{1,15}"
        ) {
            let input = format!(
                "[TOOL_CALL]{{\"name\":\"{name}\",\"arguments\":{{\"key\":\"value\"}}}}[/TOOL_CALL]"
            );
            let calls = extract_tool_calls(&input);
            // If the name is an alias, expect 'skill' after redirection
            let expected = match name.as_str() {
                "dnd" | "focus" | "tts" | "calendar" | "vision" | "eeg" | "exg" | "oura" | "location" | "router" | "settings" | "constants" | "history" | "jobs" | "gpu" | "headless" | "health" | "label_index" | "llm" | "screenshot" | "tools" | "tray" | "autostart" | "devices" | "commands" => "skill",
                _ => name.as_str(),
            };
            prop_assert!(
                calls.iter().any(|c| c.function.name == expected),
                "expected tool call with name={expected} in {:?}", calls
            );
        }

        /// Llama XML format must be parsed when well-formed.
        #[test]
        fn llama_xml_tool_call_parsed(
            name in "[a-z_]{1,15}",
            value in "[a-zA-Z0-9 ]{0,50}"
        ) {
            let input = format!("<function={name}><parameter=key>{value}</parameter></function>");
            let calls = extract_tool_calls(&input);
            let expected = match name.as_str() {
                "dnd" | "focus" | "tts" | "calendar" | "vision" | "eeg" | "exg" | "oura" | "location" | "router" | "settings" | "constants" | "history" | "jobs" | "gpu" | "headless" | "health" | "label_index" | "llm" | "screenshot" | "tools" | "tray" | "autostart" | "devices" | "commands" => "skill",
                _ => name.as_str(),
            };
            prop_assert!(
                calls.iter().any(|c| c.function.name == expected),
                "expected tool call with name={expected} in {:?}", calls
            );
        }

        /// Random noise between valid tool calls must not prevent extraction.
        #[test]
        fn noise_does_not_break_extraction(
            noise1 in "[a-zA-Z0-9 ]{0,100}",
            noise2 in "[a-zA-Z0-9 ]{0,100}"
        ) {
            let input = format!(
                "{noise1}[TOOL_CALL]{{\"name\":\"bash\",\"arguments\":{{\"command\":\"ls\"}}}}[/TOOL_CALL]{noise2}"
            );
            let calls = extract_tool_calls(&input);
            prop_assert!(calls.iter().any(|c| c.function.name == "bash"));
        }
    }
}
