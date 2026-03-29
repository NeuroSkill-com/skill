// SPDX-License-Identifier: GPL-3.0-only
//! Tests for DOT graph generation from interactive search results.

use skill_commands::graph::dot::{dot_esc, generate_dot};
use skill_commands::{InteractiveGraphEdge, InteractiveGraphNode};

fn make_node(id: &str, kind: &str, text: Option<&str>) -> InteractiveGraphNode {
    InteractiveGraphNode {
        id: id.into(),
        kind: kind.into(),
        text: text.map(String::from),
        distance: 0.1,
        ..Default::default()
    }
}

fn make_edge(from: &str, to: &str, kind: &str, dist: f32) -> InteractiveGraphEdge {
    InteractiveGraphEdge {
        from_id: from.into(),
        to_id: to.into(),
        distance: dist,
        kind: kind.into(),
    }
}

// ── dot_esc ──────────────────────────────────────────────────────────────────

#[test]
fn dot_esc_escapes_quotes() {
    assert_eq!(dot_esc(r#"hello "world""#), r#"hello \"world\""#);
}

#[test]
fn dot_esc_escapes_backslashes() {
    assert_eq!(dot_esc(r"a\b"), r"a\\b");
}

#[test]
fn dot_esc_plain_text_unchanged() {
    assert_eq!(dot_esc("hello world"), "hello world");
}

// ── generate_dot ─────────────────────────────────────────────────────────────

#[test]
fn generate_dot_empty_graph() {
    let dot = generate_dot(&[], &[]);
    assert!(dot.contains("digraph"));
    assert!(dot.contains("}"));
}

#[test]
fn generate_dot_single_node() {
    let nodes = vec![make_node("q1", "query", Some("test query"))];
    let dot = generate_dot(&nodes, &[]);
    assert!(dot.contains("q1"));
}

#[test]
fn generate_dot_with_edges() {
    let nodes = vec![
        make_node("q1", "query", Some("search")),
        make_node("l1", "found_label", Some("meditation")),
    ];
    let edges = vec![make_edge("q1", "l1", "text_sim", 0.2)];
    let dot = generate_dot(&nodes, &edges);
    assert!(dot.contains("q1"));
    assert!(dot.contains("l1"));
    assert!(dot.contains("->"));
}

#[test]
fn generate_dot_is_valid_structure() {
    let nodes = vec![
        make_node("a", "query", Some("hello")),
        make_node("b", "found_label", Some("world")),
    ];
    let edges = vec![make_edge("a", "b", "text_sim", 0.5)];
    let dot = generate_dot(&nodes, &edges);

    // Should start with digraph and end with }
    assert!(dot.trim().starts_with("digraph"));
    assert!(dot.trim().ends_with('}'));
}
