// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tests for svg.rs

use super::*;

#[test]
fn test_svg_esc() {
    assert_eq!(svg_esc("a&b"), "a&amp;b");
    assert_eq!(svg_esc("<tag>"), "&lt;tag&gt;");
    assert_eq!(svg_esc("plain"), "plain");
}

#[test]
fn test_trunc() {
    assert_eq!(trunc("abcdef", 3), "abc…");
    assert_eq!(trunc("abc", 3), "abc");
    assert_eq!(trunc("a", 3), "a");
}

#[test]
fn test_turbo_hex() {
    let c0 = turbo_hex(0.0);
    let c1 = turbo_hex(1.0);
    assert!(c0.starts_with("#") && c0.len() == 7);
    assert!(c1.starts_with("#") && c1.len() == 7);
    assert_ne!(c0, c1);
}

#[test]
fn test_generate_svg_minimal() {
    let nodes = vec![InteractiveGraphNode {
        id: "n1".into(),
        kind: "query".into(),
        ..Default::default()
    }];
    let edges = vec![];
    let labels = SvgLabels {
        layer_query: "Query".into(),
        layer_text_matches: "Text".into(),
        layer_eeg_neighbors: "EEG".into(),
        layer_found_labels: "Found".into(),
        legend_query: "LegendQ".into(),
        legend_text: "LegendT".into(),
        legend_eeg: "LegendE".into(),
        legend_found: "LegendF".into(),
        generated_by: "Skill".into(),
        ..Default::default()
    };
    let svg = generate_svg(&nodes, &edges, &labels, false);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("Query"));
}

#[test]
fn test_generate_svg_complex() {
    let nodes = vec![
        InteractiveGraphNode {
            id: "q1".into(),
            kind: "query".into(),
            ..Default::default()
        },
        InteractiveGraphNode {
            id: "t1".into(),
            kind: "text_label".into(),
            timestamp_unix: Some(1_700_000_000),
            ..Default::default()
        },
        InteractiveGraphNode {
            id: "e1".into(),
            kind: "eeg_point".into(),
            timestamp_unix: Some(1_700_000_100),
            ..Default::default()
        },
        InteractiveGraphNode {
            id: "f1".into(),
            kind: "found_label".into(),
            parent_id: Some("ep_1".into()),
            ..Default::default()
        },
        InteractiveGraphNode {
            id: "s1".into(),
            kind: "screenshot".into(),
            parent_id: Some("ep_1".into()),
            ..Default::default()
        },
    ];
    let edges = vec![
        InteractiveGraphEdge {
            source: "q1".into(),
            target: "t1".into(),
            kind: "text_sim".into(),
            ..Default::default()
        },
        InteractiveGraphEdge {
            source: "t1".into(),
            target: "e1".into(),
            kind: "eeg_sim".into(),
            ..Default::default()
        },
        InteractiveGraphEdge {
            source: "e1".into(),
            target: "f1".into(),
            kind: "label_prox".into(),
            ..Default::default()
        },
        InteractiveGraphEdge {
            source: "f1".into(),
            target: "s1".into(),
            kind: "screenshot_prox".into(),
            ..Default::default()
        },
    ];
    let labels = SvgLabels {
        layer_query: "Query".into(),
        layer_text_matches: "Text".into(),
        layer_eeg_neighbors: "EEG".into(),
        layer_found_labels: "Found".into(),
        legend_query: "LegendQ".into(),
        legend_text: "LegendT".into(),
        legend_eeg: "LegendE".into(),
        legend_found: "LegendF".into(),
        legend_screenshot: "LegendS".into(),
        generated_by: "Skill".into(),
        ..Default::default()
    };
    let svg = generate_svg(&nodes, &edges, &labels, false);
    // Check that all node kinds and edge kinds are present in the SVG output
    assert!(svg.contains("Query"));
    assert!(svg.contains("Text"));
    assert!(svg.contains("EEG"));
    assert!(svg.contains("Found"));
    assert!(svg.contains("LegendS"));
    assert!(svg.contains("marker"));
    assert!(svg.contains("#8b5cf6")); // query color
    assert!(svg.contains("#3b82f6")); // text_label color
    assert!(svg.contains("#f59e0b")); // eeg_point color
    assert!(svg.contains("#10b981")); // found_label color
    assert!(svg.contains("#ec4899")); // screenshot color
}
