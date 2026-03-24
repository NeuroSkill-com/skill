// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! 3-D perspective SVG graph generation for interactive search results.

use crate::{InteractiveGraphNode, InteractiveGraphEdge, fmt_unix_utc};
use super::svg::{SvgLabels, svg_esc, trunc};

// ── 3-D perspective SVG ────────────────────────────────────────────────────

/// Project 3-D normalised coordinates `(x, y, z)` in [-1, 1] onto 2-D screen
/// space using a simple perspective projection with depth cues.
///
/// Returns `(screen_x, screen_y, scale)` where `scale` encodes depth — farther
/// nodes are smaller and more transparent.
fn project_3d(x: f32, y: f32, z: f32, w: f64, h: f64) -> (f64, f64, f64) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    // Isometric-like projection with mild depth foreshortening
    let depth = 1.0 + (z as f64) * 0.35;  // z ∈ [-1,1] → [0.65, 1.35]
    let scale = 0.55 + depth * 0.35;      // depth → [0.78, 1.02]
    let sx = cx + (x as f64) * (w * 0.38) / depth + (z as f64) * (w * 0.08);
    let sy = cy + (y as f64) * (h * 0.35) / depth - (z as f64) * (h * 0.10);
    (sx, sy, scale)
}

/// Render a 3-D perspective SVG of the interactive search graph.
///
/// All nodes that have `proj_x` / `proj_y` / `proj_z` are placed in 3-D
/// space via PCA and perspective-projected.  Nodes without projections are
/// arranged in a header row.
pub fn generate_svg_3d(
    nodes:  &[InteractiveGraphNode],
    edges:  &[InteractiveGraphEdge],
    labels: &SvgLabels,
) -> String {
    const W: f64 = 900.0;
    const H: f64 = 700.0;
    const MARGIN: f64 = 60.0;
    const NW: f64 = 130.0;
    const NH: f64 = 32.0;
    const QR: f64 = 22.0;

    let mut o = String::with_capacity(64 * 1024);
    let wi = W as i64;
    let hi = H as i64;

    o.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{wi}" height="{hi}" viewBox="0 0 {wi} {hi}" font-family="Helvetica Neue,Helvetica,Arial,sans-serif">
  <rect width="{wi}" height="{hi}" fill="#0f1117"/>
  <defs>
    <radialGradient id="bg3d" cx="50%" cy="40%" r="60%">
      <stop offset="0%" stop-color="#1a1d2e"/>
      <stop offset="100%" stop-color="#0f1117"/>
    </radialGradient>
"##));

    // Markers
    for (id, col) in [("m3v","#8b5cf6"),("m3a","#f59e0b"),("m3e","#10b981"),("m3s","#ec4899"),("m3g","#666")] {
        o.push_str(&format!(
            "    <marker id=\"{id}\" markerWidth=\"6\" markerHeight=\"4\" refX=\"5\" refY=\"2\" orient=\"auto\" markerUnits=\"strokeWidth\">\
             <path d=\"M0,0 L6,2 L0,4 Z\" fill=\"{col}\"/></marker>\n"));
    }
    o.push_str("  </defs>\n");
    o.push_str(&format!("  <rect width=\"{wi}\" height=\"{hi}\" fill=\"url(#bg3d)\"/>\n"));

    // Grid floor lines (3D-projected)
    for i in -4..=4 {
        let t = i as f32 / 4.0;
        let (x1, y1, _) = project_3d(-1.0, 1.0, t, W, H);
        let (x2, y2, _) = project_3d(1.0,  1.0, t, W, H);
        o.push_str(&format!(
            "  <line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
             stroke=\"#ffffff\" stroke-opacity=\"0.04\" stroke-width=\"0.5\"/>\n"));
        let (x1, y1, _) = project_3d(t, 1.0, -1.0, W, H);
        let (x2, y2, _) = project_3d(t, 1.0, 1.0,  W, H);
        o.push_str(&format!(
            "  <line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
             stroke=\"#ffffff\" stroke-opacity=\"0.04\" stroke-width=\"0.5\"/>\n"));
    }

    // Position map — 3D projected
    let mut pos: std::collections::HashMap<String, (f64, f64, f64)> = Default::default();

    // Nodes WITH 3D projections
    let has_proj: Vec<&InteractiveGraphNode> = nodes.iter()
        .filter(|n| n.proj_x.is_some() && n.proj_y.is_some() && n.proj_z.is_some())
        .collect();

    // Nodes WITHOUT projections — place in a header row
    let no_proj: Vec<&InteractiveGraphNode> = nodes.iter()
        .filter(|n| n.proj_x.is_none() || n.proj_y.is_none() || n.proj_z.is_none())
        .collect();

    // Place projected nodes
    for nd in &has_proj {
        // All three projections are guaranteed `Some` by the filter above.
        let (Some(px), Some(py), Some(pz)) = (nd.proj_x, nd.proj_y, nd.proj_z) else {
            continue;
        };
        let (sx, sy, scale) = project_3d(px, py, pz, W, H);
        pos.insert(nd.id.clone(), (sx, sy, scale));
    }

    // Place unprojected nodes in a top row
    let np_count = no_proj.len().max(1);
    let np_spacing = (W - MARGIN * 2.0) / np_count as f64;
    for (i, nd) in no_proj.iter().enumerate() {
        let sx = MARGIN + np_spacing * (i as f64 + 0.5);
        let sy = MARGIN + 20.0;
        pos.insert(nd.id.clone(), (sx, sy, 1.0));
    }

    // Sort by depth (scale) — render far nodes first
    let mut render_order: Vec<&InteractiveGraphNode> = nodes.iter().collect();
    render_order.sort_by(|a, b| {
        let sa = pos.get(&a.id).map_or(1.0, |p| p.2);
        let sb = pos.get(&b.id).map_or(1.0, |p| p.2);
        sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
    });

    // Edges first (behind nodes)
    let edge_marker = |kind: &str| -> (&str, &str, &str) {
        match kind {
            "text_sim"        => ("#8b5cf6", "",    "m3v"),
            "eeg_bridge"      => ("#f59e0b", "5,3", "m3a"),
            "eeg_sim"         => ("#f59e0b", "2,3", "m3a"),
            "label_prox"      => ("#10b981", "",    "m3e"),
            "screenshot_prox" => ("#ec4899", "5,3", "m3s"),
            "ocr_sim"         => ("#ec4899", "2,3", "m3s"),
            _                 => ("#666666", "",    "m3g"),
        }
    };

    for e in edges {
        let (Some(&(x1, y1, s1)), Some(&(x2, y2, s2))) = (pos.get(&e.from_id), pos.get(&e.to_id))
            else { continue };
        let avg_scale = (s1 + s2) / 2.0;
        let opacity = (0.25 * avg_scale).clamp(0.1, 0.6);
        let pw = 1.2 * avg_scale;
        let (col, dash, mid) = edge_marker(&e.kind);
        let da = if dash.is_empty() { String::new() }
                 else { format!(" stroke-dasharray=\"{dash}\"") };
        o.push_str(&format!(
            "  <line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" \
             stroke=\"{col}\" stroke-width=\"{pw:.2}\" opacity=\"{opacity:.2}\"{da} \
             marker-end=\"url(#{mid})\"/>\n"));
    }

    // Nodes
    let node_color = |kind: &str| -> &str {
        match kind {
            "query"       => "#8b5cf6",
            "text_label"  => "#3b82f6",
            "eeg_point"   => "#f59e0b",
            "found_label" => "#10b981",
            "screenshot"  => "#ec4899",
            _             => "#888888",
        }
    };

    for nd in &render_order {
        let Some(&(cx, cy, scale)) = pos.get(&nd.id) else { continue };
        let fill = node_color(&nd.kind);
        let opacity = (0.55 + scale * 0.45).clamp(0.3, 1.0);
        let fs = (9.0 * scale).clamp(6.0, 11.0);

        // Drop shadow (depth cue)
        let shadow_blur = (3.0 * scale).clamp(1.0, 5.0);
        let shadow_opa  = (0.3 * scale).clamp(0.05, 0.4);

        match nd.kind.as_str() {
            "query" => {
                let r = QR * scale;
                o.push_str(&format!(
                    "  <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{:.1}\" fill=\"{fill}\" \
                     fill-opacity=\"0.15\" stroke=\"{fill}\" stroke-width=\"{:.1}\" opacity=\"{opacity:.2}\"/>\n",
                    r + 6.0 * scale, 2.0 * scale));
                o.push_str(&format!(
                    "  <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{r:.1}\" fill=\"{fill}\" \
                     fill-opacity=\"{opacity:.2}\"/>\n"));
                let label = trunc(nd.text.as_deref().unwrap_or("query"), 16);
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{cy:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"{fs:.1}\" font-weight=\"700\" \
                     fill=\"white\" opacity=\"{opacity:.2}\">{}</text>\n",
                    svg_esc(&label)));
            }
            "eeg_point" => {
                let s = 11.0 * scale;
                o.push_str(&format!(
                    "  <polygon points=\"{cx:.1},{:.1} {:.1},{cy:.1} {cx:.1},{:.1} {:.1},{cy:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"{opacity:.2}\"/>\n",
                    cy - s, cx + s * 1.35, cy + s, cx - s * 1.35));
                let time_str = nd.timestamp_unix.map(|ts| {
                    let h = (ts % 86400) / 3600;
                    let m = (ts % 3600)  / 60;
                    format!("{h:02}:{m:02}")
                }).unwrap_or_default();
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{cy:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"{:.1}\" font-weight=\"600\" \
                     fill=\"white\" opacity=\"{opacity:.2}\">{}</text>\n",
                    fs * 0.8, svg_esc(&time_str)));
            }
            "screenshot" => {
                let rw = (NW + 8.0) * scale;
                let rh = (NH + 8.0) * scale;
                // Shadow
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{rw:.1}\" height=\"{rh:.1}\" rx=\"{:.1}\" \
                     fill=\"black\" fill-opacity=\"{shadow_opa:.2}\" filter=\"blur({shadow_blur:.1}px)\"/>\n",
                    cx - rw / 2.0 + 2.0, cy - rh / 2.0 + 2.0, 6.0 * scale));
                // Main rect
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{rw:.1}\" height=\"{rh:.1}\" rx=\"{:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"{opacity:.2}\" stroke=\"{fill}\" \
                     stroke-width=\"{:.1}\" stroke-opacity=\"0.3\"/>\n",
                    cx - rw / 2.0, cy - rh / 2.0, 6.0 * scale, 1.2 * scale));
                let title = nd.window_title.as_deref()
                    .or(nd.app_name.as_deref())
                    .unwrap_or("screenshot");
                let primary = trunc(title, 16);
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"{fs:.1}\" font-weight=\"600\" \
                     fill=\"white\" opacity=\"{opacity:.2}\">{}</text>\n",
                    if nd.timestamp_unix.is_some() { cy - 5.0 * scale } else { cy },
                    svg_esc(&primary)));
                if let Some(ts) = nd.timestamp_unix {
                    o.push_str(&format!(
                        "  <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                         dominant-baseline=\"middle\" font-size=\"{:.1}\" fill=\"white\" \
                         opacity=\"{:.2}\">{}</text>\n",
                        cy + 7.0 * scale, fs * 0.75, opacity * 0.7, svg_esc(&fmt_unix_utc(ts))));
                }
            }
            _ => {
                // text_label / found_label — rounded rect
                let rw = NW * scale;
                let rh = NH * scale;
                let rx = if nd.kind == "found_label" { rh / 2.0 } else { 5.0 * scale };
                // Shadow
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{rw:.1}\" height=\"{rh:.1}\" rx=\"{rx:.1}\" \
                     fill=\"black\" fill-opacity=\"{shadow_opa:.2}\" filter=\"blur({shadow_blur:.1}px)\"/>\n",
                    cx - rw / 2.0 + 2.0, cy - rh / 2.0 + 2.0));
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{rw:.1}\" height=\"{rh:.1}\" rx=\"{rx:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"{opacity:.2}\"/>\n",
                    cx - rw / 2.0, cy - rh / 2.0));
                let primary = trunc(nd.text.as_deref().unwrap_or(""), 18);
                let has_sub = nd.timestamp_unix.is_some()
                    && matches!(nd.kind.as_str(), "text_label" | "found_label");
                let ty = if has_sub { cy - 5.0 * scale } else { cy };
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{ty:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"{fs:.1}\" font-weight=\"600\" \
                     fill=\"white\" opacity=\"{opacity:.2}\">{}</text>\n",
                    svg_esc(&primary)));
                if has_sub {
                    if let Some(ts) = nd.timestamp_unix {
                        o.push_str(&format!(
                            "  <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                             dominant-baseline=\"middle\" font-size=\"{:.1}\" fill=\"white\" \
                             opacity=\"{:.2}\">{}</text>\n",
                            cy + 7.0 * scale, fs * 0.75, opacity * 0.7, svg_esc(&fmt_unix_utc(ts))));
                    }
                }
            }
        }
    }

    // Legend
    let legend_y = H - 32.0;
    let legend_items: Vec<(&str, &str)> = {
        let mut v = vec![
            ("#8b5cf6", labels.legend_query.as_str()),
            ("#3b82f6", labels.legend_text.as_str()),
            ("#f59e0b", labels.legend_eeg.as_str()),
            ("#10b981", labels.legend_found.as_str()),
        ];
        if nodes.iter().any(|n| n.kind == "screenshot") {
            v.push(("#ec4899", labels.legend_screenshot.as_str()));
        }
        v
    };
    let lw = 78.0_f64;
    let lx0 = (W - lw * legend_items.len() as f64) / 2.0;
    for (i, (col, lbl)) in legend_items.iter().enumerate() {
        let x = lx0 + i as f64 * lw;
        o.push_str(&format!(
            "  <circle cx=\"{:.1}\" cy=\"{legend_y:.1}\" r=\"4\" fill=\"{col}\" opacity=\"0.8\"/>\n\
             <text x=\"{:.1}\" y=\"{legend_y:.1}\" dominant-baseline=\"middle\" \
             font-size=\"8\" fill=\"#999\">{}</text>\n",
            x + 4.0, x + 12.0, svg_esc(lbl)));
    }

    // Title
    o.push_str(&format!(
        "  <text x=\"{:.1}\" y=\"22\" text-anchor=\"middle\" font-size=\"11\" \
         fill=\"#666\" font-weight=\"500\">3D Embedding Space</text>\n",
        W / 2.0));

    // Footer
    o.push_str(&format!(
        "  <text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"7\" \
         fill=\"#555\">{}</text>\n",
        W / 2.0, H - 10.0, svg_esc(&labels.generated_by)));

    o.push_str("</svg>\n");
    o
}

