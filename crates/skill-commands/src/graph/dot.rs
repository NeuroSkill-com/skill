// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! DOT (Graphviz) graph generation for interactive search results.

use crate::{InteractiveGraphNode, InteractiveGraphEdge, fmt_unix_utc};

/// Escape a string for use inside a DOT double-quoted label.
pub fn dot_esc(s: &str) -> String {
    s.chars().flat_map(|c| match c {
        '"'  => vec!['\\', '"'],
        '\\' => vec!['\\', '\\'],
        '\n' | '\r' => vec![],
        _    => vec![c],
    }).collect()
}

/// Build a DOT label string for a node (may contain `\n` for graphviz newlines).
pub fn dot_node_label(n: &InteractiveGraphNode) -> String {
    match n.kind.as_str() {
        "query" => dot_esc(n.text.as_deref().unwrap_or("query")),
        "text_label" => {
            let text = dot_esc(n.text.as_deref().unwrap_or("?"));
            match n.timestamp_unix {
                Some(ts) => format!("{text}\\n{}", fmt_unix_utc(ts)),
                None     => text,
            }
        }
        "eeg_point" => match n.timestamp_unix {
            Some(ts) => fmt_unix_utc(ts),
            None     => n.id.clone(),
        },
        "found_label" => {
            let text = dot_esc(n.text.as_deref().unwrap_or("?"));
            match n.timestamp_unix {
                Some(ts) => format!("{text}\\n{}", fmt_unix_utc(ts)),
                None     => text,
            }
        }
        "screenshot" => {
            let title = n.window_title.as_deref()
                .or(n.app_name.as_deref())
                .unwrap_or("screenshot");
            let title = dot_esc(title);
            match n.timestamp_unix {
                Some(ts) => format!("{title}\\n{}", fmt_unix_utc(ts)),
                None     => title,
            }
        }
        _ => dot_esc(n.text.as_deref().unwrap_or(&n.id)),
    }
}

/// Build a short edge label.
pub fn dot_edge_label(
    e:      &InteractiveGraphEdge,
    ts_map: &std::collections::HashMap<String, u64>,
) -> String {
    match e.kind.as_str() {
        "text_sim" => {
            let pct = ((1.0 - e.distance) * 100.0).clamp(0.0, 100.0);
            format!("{pct:.0}%")
        }
        "eeg_bridge" | "eeg_sim" => format!("d={:.3}", e.distance),
        "label_prox" => {
            if let (Some(&a), Some(&b)) = (ts_map.get(&e.from_id), ts_map.get(&e.to_id)) {
                let diff_m = (a as i64 - b as i64).unsigned_abs() / 60;
                format!("{diff_m}min")
            } else {
                format!("{:.2}", e.distance)
            }
        }
        "screenshot_prox" => {
            if let (Some(&a), Some(&b)) = (ts_map.get(&e.from_id), ts_map.get(&e.to_id)) {
                let diff_m = (a as i64 - b as i64).unsigned_abs() / 60;
                format!("{diff_m}min")
            } else {
                format!("{:.2}", e.distance)
            }
        }
        "ocr_sim" => {
            let pct = ((1.0 - e.distance) * 100.0).clamp(0.0, 100.0);
            format!("{pct:.0}%")
        }
        _ => String::new(),
    }
}

/// Render `nodes` + `edges` as a Graphviz DOT string.
pub fn generate_dot(nodes: &[InteractiveGraphNode], edges: &[InteractiveGraphEdge]) -> String {
    let mut o = String::with_capacity(8 * 1024);

    o.push_str("digraph interactive_search {\n");
    o.push_str("  graph [rankdir=TB, bgcolor=\"white\", fontname=\"Helvetica\",\n");
    o.push_str("         splines=curved, pad=0.5, nodesep=0.55, ranksep=1.1];\n");
    o.push_str("  node  [fontname=\"Helvetica\", fontsize=10,\n");
    o.push_str("         style=\"filled,rounded\", penwidth=0, margin=\"0.18,0.10\"];\n");
    o.push_str("  edge  [fontname=\"Helvetica\", fontsize=8, arrowsize=0.75];\n\n");

    let ts_map: std::collections::HashMap<String, u64> = nodes.iter()
        .filter_map(|n| n.timestamp_unix.map(|ts| (n.id.clone(), ts)))
        .collect();

    let ids_of = |kind: &str| -> String {
        nodes.iter()
            .filter(|n| n.kind == kind)
            .map(|n| format!("\"{}\"", n.id))
            .collect::<Vec<_>>()
            .join(" ")
    };

    let query_row  = ids_of("query");
    let text_row   = ids_of("text_label");
    let eeg_row    = ids_of("eeg_point");
    let found_row  = ids_of("found_label");
    let ss_row     = ids_of("screenshot");

    if !query_row.is_empty()  { o.push_str(&format!("  {{ rank=source; {query_row} }}\n")); }
    if !text_row.is_empty()   { o.push_str(&format!("  {{ rank=same;   {text_row} }}\n")); }
    if !eeg_row.is_empty()    { o.push_str(&format!("  {{ rank=same;   {eeg_row} }}\n")); }
    if !found_row.is_empty()  { o.push_str(&format!("  {{ rank=same;   {found_row} }}\n")); }
    if !ss_row.is_empty()     { o.push_str(&format!("  {{ rank=sink;   {ss_row} }}\n")); }
    o.push('\n');

    for n in nodes {
        let (shape, fill, fc) = match n.kind.as_str() {
            "query"       => ("doublecircle", "#8b5cf6", "white"),
            "text_label"  => ("box",          "#3b82f6", "white"),
            "eeg_point"   => ("diamond",      "#f59e0b", "white"),
            "found_label" => ("ellipse",      "#10b981", "white"),
            "screenshot"  => ("note",         "#ec4899", "white"),
            _             => ("box",          "#888888", "white"),
        };
        let lbl   = dot_node_label(n);
        let title = n.text.as_deref().unwrap_or(&n.id);
        o.push_str(&format!(
            "  \"{id}\" [label=\"{lbl}\", shape={shape}, \
             fillcolor=\"{fill}\", fontcolor=\"{fc}\", \
             tooltip=\"{tip}\"];\n",
            id    = n.id,
            tip   = dot_esc(title),
        ));
    }
    o.push('\n');

    for e in edges {
        let (color, style, pw) = match e.kind.as_str() {
            "text_sim"        => ("#8b5cf6", "solid",  2.0_f32),
            "eeg_bridge"      => ("#f59e0b", "dashed", 1.5_f32),
            "eeg_sim"         => ("#f59e0b", "dotted", 1.5_f32),
            "label_prox"      => ("#10b981", "solid",  1.5_f32),
            "screenshot_prox" => ("#ec4899", "dashed", 1.5_f32),
            "ocr_sim"         => ("#ec4899", "dotted", 1.5_f32),
            _                 => ("#888888", "solid",  1.0_f32),
        };
        let lbl = dot_edge_label(e, &ts_map);
        o.push_str(&format!(
            "  \"{from}\" -> \"{to}\" \
             [color=\"{color}\", style={style}, penwidth={pw:.1}, label=\"{lbl}\"];\n",
            from = e.from_id,
            to   = e.to_id,
        ));
    }

    o.push_str("}\n");
    o
}
