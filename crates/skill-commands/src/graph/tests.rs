// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Tests for graph generation sub-modules.

use super::dot::*;
use super::svg::*;
use super::svg_3d::*;
use crate::{InteractiveGraphNode, InteractiveGraphEdge};

    #[test]
    fn dot_esc_quotes() {
        assert_eq!(dot_esc(r#"say "hello""#), r#"say \"hello\""#);
    }

    #[test]
    fn dot_esc_backslash() {
        assert_eq!(dot_esc(r"C:\path"), r"C:\\path");
    }

    #[test]
    fn dot_esc_newlines_stripped() {
        assert_eq!(dot_esc("line1\nline2\r"), "line1line2");
    }

    #[test]
    fn dot_esc_plain_text() {
        assert_eq!(dot_esc("hello world"), "hello world");
    }

    #[test]
    fn svg_esc_ampersand() {
        assert_eq!(svg_esc("A & B"), "A &amp; B");
    }

    #[test]
    fn svg_esc_angle_brackets() {
        assert_eq!(svg_esc("<b>bold</b>"), "&lt;b&gt;bold&lt;/b&gt;");
    }

    #[test]
    fn trunc_short_unchanged() {
        assert_eq!(trunc("hi", 5), "hi");
    }

    #[test]
    fn trunc_exact_length() {
        assert_eq!(trunc("abcde", 5), "abcde");
    }

    #[test]
    fn trunc_clips_with_ellipsis() {
        assert_eq!(trunc("abcdef", 5), "abcde…");
    }

    #[test]
    fn turbo_hex_black_at_zero() {
        let hex = turbo_hex(0.0);
        assert_eq!(hex.len(), 7); // #rrggbb
        assert!(hex.starts_with('#'));
    }

    #[test]
    fn turbo_hex_clamps() {
        let lo = turbo_hex(-1.0);
        let hi = turbo_hex(2.0);
        assert_eq!(lo, turbo_hex(0.0));
        assert_eq!(hi, turbo_hex(1.0));
    }

    #[test]
    fn generate_dot_empty() {
        let dot = generate_dot(&[], &[]);
        assert!(dot.contains("digraph"));
        assert!(dot.contains('}'));
    }

    #[test]
    fn generate_dot_single_node() {
        let nodes = vec![InteractiveGraphNode {
            id: "n1".into(),
            kind: "query".into(),
            text: Some("focus".into()),
            ..InteractiveGraphNode::default()
        }];
        let dot = generate_dot(&nodes, &[]);
        assert!(dot.contains("focus"));
    }

    #[test]
    fn generate_dot_screenshot_node() {
        let nodes = vec![
            InteractiveGraphNode {
                id: "ep_1".into(),
                kind: "eeg_point".into(),
                timestamp_unix: Some(1700000000),
                ..InteractiveGraphNode::default()
            },
            InteractiveGraphNode {
                id: "ss_1".into(),
                kind: "screenshot".into(),
                window_title: Some("VS Code — main.rs".into()),
                timestamp_unix: Some(1700000005),
                parent_id: Some("ep_1".into()),
                filename: Some("20231114/20231114120005.webp".into()),
                ..InteractiveGraphNode::default()
            },
        ];
        let edges = vec![InteractiveGraphEdge {
            from_id: "ep_1".into(),
            to_id: "ss_1".into(),
            distance: 0.1,
            kind: "screenshot_prox".into(),
        }];
        let dot = generate_dot(&nodes, &edges);
        assert!(dot.contains("ss_1"));
        assert!(dot.contains("note")); // screenshot shape
        assert!(dot.contains("#ec4899")); // screenshot color
    }

    #[test]
    fn generate_svg_3d_smoke() {
        let nodes = vec![
            InteractiveGraphNode {
                id: "q".into(),
                kind: "query".into(),
                text: Some("test".into()),
                proj_x: Some(0.0),
                proj_y: Some(0.0),
                proj_z: Some(0.0),
                ..InteractiveGraphNode::default()
            },
            InteractiveGraphNode {
                id: "ss_1".into(),
                kind: "screenshot".into(),
                window_title: Some("Browser".into()),
                proj_x: Some(0.5),
                proj_y: Some(-0.3),
                proj_z: Some(0.7),
                ..InteractiveGraphNode::default()
            },
        ];
        let labels = SvgLabels {
            layer_query: "QUERY".into(),
            layer_text_matches: "TEXT".into(),
            layer_eeg_neighbors: "EEG".into(),
            layer_found_labels: "FOUND".into(),
            layer_screenshots: "SCREENSHOTS".into(),
            legend_query: "Query".into(),
            legend_text: "Text".into(),
            legend_eeg: "EEG".into(),
            legend_found: "Found".into(),
            legend_screenshot: "Screenshot".into(),
            generated_by: "Test".into(),
        };
        let svg = generate_svg_3d(&nodes, &[], &labels);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("3D Embedding Space"));
        assert!(svg.contains("Screenshot")); // legend
    }

    #[test]
    fn pca_3d_basic() {
        use crate::pca_3d;
        let embs = vec![
            vec![1.0, 0.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0, 0.0],
            vec![0.0, 0.0, 1.0, 0.0],
            vec![0.0, 0.0, 0.0, 1.0],
        ];
        let result = pca_3d(&embs);
        assert_eq!(result.len(), 4);
        for (x, y, z) in &result {
            assert!(*x >= -1.01 && *x <= 1.01, "x={x}");
            assert!(*y >= -1.01 && *y <= 1.01, "y={y}");
            assert!(*z >= -1.01 && *z <= 1.01, "z={z}");
        }
        // Points should be distinct (orthogonal inputs)
        let (x0, y0, z0) = result[0];
        let (x1, y1, z1) = result[1];
        let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2) + (z1 - z0).powi(2)).sqrt();
        assert!(dist > 0.1, "too close: {dist}");
    }

    #[test]
    fn pca_3d_single() {
        use crate::pca_3d;
        let result = pca_3d(&[vec![1.0, 2.0, 3.0]]);
        assert_eq!(result, vec![(0.0, 0.0, 0.0)]);
    }

    #[test]
    fn pca_3d_empty() {
        use crate::pca_3d;
        let result = pca_3d(&[]);
        assert!(result.is_empty());
    }
