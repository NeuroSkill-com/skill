// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! DOT and SVG graph generation for interactive search results.

use super::{InteractiveGraphNode, InteractiveGraphEdge, fmt_unix_utc};

// ── DOT generation helpers ─────────────────────────────────────────────────

// `fmt_unix_utc` is re-exported from `skill_data::util` at the top of this file.

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

// ── SVG generation ─────────────────────────────────────────────────────────

/// Escape a string for SVG/XML text content.
fn svg_esc(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
}

/// Truncate to at most `n` Unicode chars, appending `…` if clipped.
fn trunc(s: &str, n: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(n).collect();
    if chars.next().is_some() { format!("{head}…") } else { head }
}

/// Turbo colormap: t ∈ [0,1] → `#rrggbb` (matches the JS component).
fn turbo_hex(t: f64) -> String {
    let c = t.clamp(0.0, 1.0);
    let r = (0.13572138 + c*(4.61539260 + c*(-42.66032258 + c*(132.13108234 + c*(-152.54893924 + c*59.28637943))))).clamp(0.0,1.0);
    let g = (0.09140261 + c*(2.19418839 + c*(4.84296658   + c*(-14.18503333 + c*(4.27729857   + c*2.82956604))))).clamp(0.0,1.0);
    let b = (0.10667330 + c*(12.64194608+ c*(-60.58204836 + c*(110.36276771 + c*(-89.90310912 + c*27.34824973))))).clamp(0.0,1.0);
    format!("#{:02x}{:02x}{:02x}", (r*255.0) as u8, (g*255.0) as u8, (b*255.0) as u8)
}

/// Localised strings embedded into the SVG export.
/// Every field is plain text (already translated by the frontend).
#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvgLabels {
    pub layer_query:        String,
    pub layer_text_matches: String,
    pub layer_eeg_neighbors:String,
    pub layer_found_labels: String,
    /// Layer header for screenshots discovered via temporal/OCR proximity.
    #[serde(default = "default_layer_screenshots")]
    pub layer_screenshots:  String,
    pub legend_query:       String,
    pub legend_text:        String,
    pub legend_eeg:         String,
    pub legend_found:       String,
    /// Legend entry for screenshot nodes.
    #[serde(default = "default_legend_screenshot")]
    pub legend_screenshot:  String,
    /// Already interpolated: "Generated by Skill"
    pub generated_by:       String,
}

fn default_layer_screenshots() -> String { "SCREENSHOTS".into() }
fn default_legend_screenshot() -> String { "Screenshot".into() }

/// Iteratively separate overlapping label ellipses in the SVG scatter area.
fn separate_labels_svg(
    pos:    &mut [(f64, f64)],
    w:      f64,
    h:      f64,
    cx_min: f64,
    cx_max: f64,
    cy_min: f64,
    cy_max: f64,
) {
    let min_x = w + 8.0;
    let min_y = h + 8.0;

    for _ in 0..80 {
        let mut changed = false;
        for i in 0..pos.len() {
            for j in (i + 1)..pos.len() {
                let dx = pos[j].0 - pos[i].0;
                let dy = pos[j].1 - pos[i].1;
                let ox = min_x - dx.abs();
                let oy = min_y - dy.abs();
                if ox <= 0.0 || oy <= 0.0 { continue; }
                changed = true;
                if ox < oy {
                    let push = ox * 0.5 + 1.0;
                    let sign = if dx >= 0.0 { 1.0 } else { -1.0 };
                    pos[i].0 -= push * sign;
                    pos[j].0 += push * sign;
                } else {
                    let push = oy * 0.5 + 1.0;
                    let sign = if dy >= 0.0 { 1.0 } else { -1.0 };
                    pos[i].1 -= push * sign;
                    pos[j].1 += push * sign;
                }
            }
        }
        for p in pos.iter_mut() {
            p.0 = p.0.clamp(cx_min, cx_max);
            p.1 = p.1.clamp(cy_min, cy_max);
        }
        if !changed { break; }
    }
}

/// Render an SVG of the interactive search graph.
pub fn generate_svg(
    nodes:   &[InteractiveGraphNode],
    edges:   &[InteractiveGraphEdge],
    labels:  &SvgLabels,
    use_pca: bool,
) -> String {
    // ── Layout constants ──────────────────────────────────────────────────
    const NW:           f64 = 140.0;
    const NH:           f64 = 34.0;
    const QR:           f64 = 24.0;
    const TOP:          f64 = 60.0;
    const SIDE:         f64 = 40.0;
    const DAY_LBL_W:    f64 = 50.0;
    const HOUR_LBL_H:   f64 = 16.0;
    const BAND_GAP:     f64 = 10.0;
    const BAND_PAD:     f64 = 10.0;
    const TL_COL_GAP:   f64 = 8.0;
    const TL_ROW_GAP:   f64 = 6.0;
    const TL_CELL_PAD:  f64 = 5.0;
    const EEG_CELL_W:   f64 = 54.0;
    const EEG_CELL_H:   f64 = 36.0;
    const EEG_S:        f64 = 11.0;
    const FL_COL_GAP:   f64 = 10.0;
    const FL_ROW_GAP:   f64 = 6.0;
    const FL_HDR_H:     f64 = 14.0;

    let kind_order = ["query", "text_label", "eeg_point", "found_label", "screenshot"];
    let layers: Vec<Vec<&InteractiveGraphNode>> = kind_order.iter()
        .map(|k| nodes.iter().filter(|n| n.kind == *k).collect())
        .collect();

    let ts_dhm = |ts: u64| -> (String, u32, u32) {
        let dt   = fmt_unix_utc(ts);
        let date = dt[..10].to_string();
        let h    = ((ts % 86400) / 3600) as u32;
        let m    = ((ts % 3600)  / 60)   as u32;
        (date, h, m)
    };

    // ── Text-matches grid analysis ────────────────────────────────────────
    let has_tl = !layers[1].is_empty();
    let tl_info: Vec<(String, u32, u32)> = layers[1].iter()
        .map(|nd| ts_dhm(nd.timestamp_unix.unwrap_or(0)))
        .collect();
    let mut tl_days:  Vec<String> = tl_info.iter().map(|(d,_,_)| d.clone()).collect();
    tl_days.sort_unstable(); tl_days.dedup();
    let mut tl_hours: Vec<u32> = tl_info.iter().map(|(_,h,_)| *h).collect();
    tl_hours.sort_unstable(); tl_hours.dedup();
    let n_tl_days  = tl_days.len().max(1);
    let n_tl_hours = tl_hours.len().max(1);
    let tl_day_idx:  std::collections::HashMap<&str, usize> =
        tl_days.iter().enumerate().map(|(i, d)| (d.as_str(), i)).collect();
    let tl_hour_idx: std::collections::HashMap<u32, usize>  =
        tl_hours.iter().enumerate().map(|(i, &h)| (h, i)).collect();
    let max_tl_stack: usize = {
        let mut counts: std::collections::HashMap<(usize, usize), usize> = Default::default();
        for (date, hour, _) in &tl_info {
            *counts.entry((tl_day_idx[date.as_str()], tl_hour_idx[hour])).or_insert(0) += 1;
        }
        counts.values().copied().max().unwrap_or(1)
    };
    let tl_col_w  = NW + TL_COL_GAP;
    let tl_cell_h = TL_CELL_PAD * 2.0
        + max_tl_stack as f64 * NH
        + max_tl_stack.saturating_sub(1) as f64 * TL_ROW_GAP;
    let tl_grid_w = n_tl_hours as f64 * tl_col_w - TL_COL_GAP;
    let tl_grid_h = n_tl_days  as f64 * tl_cell_h;

    // ── EEG grid analysis ─────────────────────────────────────────────────
    let has_eeg = !layers[2].is_empty();
    let eeg_info: Vec<(String, u32, u32)> = layers[2].iter()
        .map(|nd| ts_dhm(nd.timestamp_unix.unwrap_or(0)))
        .collect();
    let mut eeg_days:  Vec<String> = eeg_info.iter().map(|(d,_,_)| d.clone()).collect();
    eeg_days.sort_unstable(); eeg_days.dedup();
    let mut eeg_hours: Vec<u32> = eeg_info.iter().map(|(_,h,_)| *h).collect();
    eeg_hours.sort_unstable(); eeg_hours.dedup();
    let n_eeg_days  = eeg_days.len().max(1);
    let n_eeg_hours = eeg_hours.len().max(1);
    let eeg_grid_w  = n_eeg_hours as f64 * EEG_CELL_W;

    // ── Found-label cluster analysis ─────────────────────────────────────
    let has_fl = !layers[3].is_empty();
    let mut fl_parents: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        layers[3].iter()
            .filter_map(|nd| nd.parent_id.as_deref())
            .filter(|p| seen.insert(*p))
            .map(|p| p.to_string())
            .collect()
    };
    fl_parents.sort_by_key(|p| {
        p.strip_prefix("ep_").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
    });
    let mut fl_by_parent: std::collections::HashMap<String, Vec<&InteractiveGraphNode>> =
        Default::default();
    for nd in &layers[3] {
        if let Some(pid) = nd.parent_id.as_deref() {
            fl_by_parent.entry(pid.to_string()).or_default().push(nd);
        }
    }
    let n_fl_cols    = fl_parents.len().max(1);
    let max_fl_stack = fl_parents.iter()
        .map(|p| fl_by_parent.get(p).map_or(0, |v| v.len()))
        .max().unwrap_or(1);
    let fl_col_w  = NW + FL_COL_GAP;
    let fl_row_h  = NH + FL_ROW_GAP;

    let fl_has_proj = use_pca && layers[3].iter().any(|nd| nd.proj_x.is_some());

    let n_fl = layers[3].len().max(1);
    let fl_scatter_cols = ((n_fl as f64).sqrt().ceil() as usize).max(2);
    let fl_scatter_rows = ((n_fl as f64 / fl_scatter_cols as f64).ceil() as usize).max(1);
    let fl_scatter_w = ((fl_scatter_cols as f64) * (NW + 12.0)).max(380.0);
    let fl_scatter_h = ((fl_scatter_rows as f64) * (NH + 14.0)).max(150.0);

    let (fl_grid_w, fl_grid_h) = if fl_has_proj {
        (fl_scatter_w, fl_scatter_h)
    } else {
        (
            n_fl_cols as f64 * fl_col_w - FL_COL_GAP,
            FL_HDR_H + max_fl_stack as f64 * fl_row_h - FL_ROW_GAP,
        )
    };

    // ── Screenshot grid analysis ──────────────────────────────────────────
    let has_ss = !layers[4].is_empty();
    let ss_col_w  = NW + FL_COL_GAP;
    let ss_row_h  = NH + FL_ROW_GAP + 4.0;
    let mut ss_parents: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        layers[4].iter()
            .filter_map(|nd| nd.parent_id.as_deref())
            .filter(|p| seen.insert(*p))
            .map(|p| p.to_string())
            .collect()
    };
    ss_parents.sort_by_key(|p| {
        p.strip_prefix("ep_").and_then(|s| s.parse::<u64>().ok()).unwrap_or(0)
    });
    let mut ss_by_parent: std::collections::HashMap<String, Vec<&InteractiveGraphNode>> =
        Default::default();
    for nd in &layers[4] {
        if let Some(pid) = nd.parent_id.as_deref() {
            ss_by_parent.entry(pid.to_string()).or_default().push(nd);
        }
    }
    let n_ss_cols    = ss_parents.len().max(1);
    let max_ss_stack = ss_parents.iter()
        .map(|p| ss_by_parent.get(p).map_or(0, |v| v.len()))
        .max().unwrap_or(1);
    let ss_grid_w = n_ss_cols as f64 * ss_col_w - FL_COL_GAP;
    let ss_grid_h = FL_HDR_H + max_ss_stack as f64 * ss_row_h - FL_ROW_GAP;

    // ── SVG width ─────────────────────────────────────────────────────────
    let tl_total_w  = DAY_LBL_W + tl_grid_w + SIDE * 2.0;
    let eeg_total_w = DAY_LBL_W + eeg_grid_w + SIDE * 2.0;
    let fl_total_w  = fl_grid_w  + SIDE * 2.0;
    let ss_total_w  = ss_grid_w  + SIDE * 2.0;
    let svg_w = (QR * 2.0 + SIDE * 2.0)
        .max(if has_tl  { tl_total_w  } else { 0.0 })
        .max(if has_eeg { eeg_total_w } else { 0.0 })
        .max(if has_fl  { fl_total_w  } else { 0.0 })
        .max(if has_ss  { ss_total_w  } else { 0.0 });

    // ── Y positions ───────────────────────────────────────────────────────
    let query_y     = TOP;
    let tl_band_top = query_y + (QR + 8.0) + BAND_GAP;
    let tl_grid_top = tl_band_top + BAND_PAD + HOUR_LBL_H;
    let tl_band_bot = tl_grid_top + tl_grid_h + BAND_PAD;
    let eeg_band_top = if has_tl { tl_band_bot + BAND_GAP }
                       else       { tl_band_top };
    let eeg_grid_top = eeg_band_top + BAND_PAD + HOUR_LBL_H;
    let eeg_band_bot = eeg_grid_top + n_eeg_days as f64 * EEG_CELL_H + BAND_PAD;
    let fl_band_top = if has_eeg      { eeg_band_bot + BAND_GAP }
                      else if has_tl  { tl_band_bot  + BAND_GAP }
                      else            { tl_band_top };
    let fl_grid_top = fl_band_top + BAND_PAD;
    let fl_band_bot = fl_grid_top + fl_grid_h + BAND_PAD;
    let ss_band_top = if has_fl       { fl_band_bot  + BAND_GAP }
                      else if has_eeg { eeg_band_bot + BAND_GAP }
                      else if has_tl  { tl_band_bot  + BAND_GAP }
                      else            { tl_band_top };
    let ss_grid_top = ss_band_top + BAND_PAD;
    let ss_band_bot = ss_grid_top + ss_grid_h + BAND_PAD;
    let svg_h = (if has_ss { ss_band_bot } else { fl_band_bot }) + 56.0;

    // ── Centre positions ──────────────────────────────────────────────────
    let mut pos: std::collections::HashMap<String, (f64, f64)> = Default::default();

    for nd in &layers[0] {
        pos.insert(nd.id.clone(), (svg_w / 2.0, query_y));
    }

    if has_tl {
        let block_w  = DAY_LBL_W + tl_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        let mut cell_slots: std::collections::HashMap<(usize, usize), usize> = Default::default();
        for (nd, (date, hour, _)) in layers[1].iter().zip(tl_info.iter()) {
            let col  = tl_hour_idx[hour];
            let row  = tl_day_idx[date.as_str()];
            let slot = *cell_slots.entry((row, col)).or_insert(0);
            cell_slots.entry((row, col)).and_modify(|s| *s += 1);
            let cx = cells_x0 + col as f64 * tl_col_w + NW / 2.0;
            let cy = tl_grid_top + row as f64 * tl_cell_h
                   + TL_CELL_PAD + slot as f64 * (NH + TL_ROW_GAP) + NH / 2.0;
            pos.insert(nd.id.clone(), (cx, cy));
        }
    }

    if has_eeg {
        let day_idx:  std::collections::HashMap<&str, usize> =
            eeg_days.iter().enumerate().map(|(i, d)| (d.as_str(), i)).collect();
        let hour_idx: std::collections::HashMap<u32, usize>  =
            eeg_hours.iter().enumerate().map(|(i, &h)| (h, i)).collect();
        let block_w  = DAY_LBL_W + eeg_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        for (nd, (date, hour, min)) in layers[2].iter().zip(eeg_info.iter()) {
            let col = hour_idx[hour];
            let row = day_idx[date.as_str()];
            let cell_cx = cells_x0 + col as f64 * EEG_CELL_W + EEG_CELL_W / 2.0;
            let cell_cy = eeg_grid_top + row as f64 * EEG_CELL_H + EEG_CELL_H / 2.0;
            let jitter  = (*min as f64 / 59.0 - 0.5) * (EEG_CELL_W - EEG_S * 3.0).max(0.0);
            pos.insert(nd.id.clone(), (cell_cx + jitter, cell_cy));
        }
    }

    if has_fl {
        if fl_has_proj {
            let pairs: Vec<(f32, f32)> = layers[3].iter()
                .map(|nd| (nd.proj_x.unwrap_or(0.0), nd.proj_y.unwrap_or(0.0)))
                .collect();
            let px_min = pairs.iter().map(|&(x, _)| x).fold(f32::MAX, f32::min);
            let px_max = pairs.iter().map(|&(x, _)| x).fold(f32::MIN, f32::max);
            let py_min = pairs.iter().map(|&(_, y)| y).fold(f32::MAX, f32::min);
            let py_max = pairs.iter().map(|&(_, y)| y).fold(f32::MIN, f32::max);
            let px_range = ((px_max - px_min) as f64).max(0.01);
            let py_range = ((py_max - py_min) as f64).max(0.01);
            let margin_x = NW / 2.0 + 6.0;
            let margin_y = NH / 2.0 + 6.0;
            let usable_w  = fl_scatter_w - margin_x * 2.0;
            let usable_h  = fl_scatter_h - margin_y * 2.0;
            let scatter_x0 = (svg_w - fl_scatter_w) / 2.0 + margin_x;
            let scatter_y0 = fl_grid_top + margin_y;
            let cx_min = scatter_x0;
            let cx_max = scatter_x0 + usable_w;
            let cy_min = scatter_y0;
            let cy_max = scatter_y0 + usable_h;

            let mut raw_pos: Vec<(f64, f64)> = pairs.iter().map(|&(px, py)| {
                let cx = scatter_x0 + (px - px_min) as f64 / px_range * usable_w;
                let cy = scatter_y0 + (py - py_min) as f64 / py_range * usable_h;
                (cx, cy)
            }).collect();

            separate_labels_svg(&mut raw_pos, NW, NH, cx_min, cx_max, cy_min, cy_max);

            for (nd, &(cx, cy)) in layers[3].iter().zip(raw_pos.iter()) {
                pos.insert(nd.id.clone(), (cx, cy));
            }
        } else {
            let x0 = (svg_w - fl_grid_w) / 2.0 + NW / 2.0;
            for (ci, parent_id) in fl_parents.iter().enumerate() {
                let cx = x0 + ci as f64 * fl_col_w;
                if let Some(group) = fl_by_parent.get(parent_id) {
                    for (ri, nd) in group.iter().enumerate() {
                        let cy = fl_grid_top + FL_HDR_H + ri as f64 * fl_row_h + NH / 2.0;
                        pos.insert(nd.id.clone(), (cx, cy));
                    }
                }
            }
        }
    }

    // ── Screenshot node positions ─────────────────────────────────────────
    if has_ss {
        let x0 = (svg_w - ss_grid_w) / 2.0 + NW / 2.0;
        for (ci, parent_id) in ss_parents.iter().enumerate() {
            let cx = x0 + ci as f64 * ss_col_w;
            if let Some(group) = ss_by_parent.get(parent_id) {
                for (ri, nd) in group.iter().enumerate() {
                    let cy = ss_grid_top + FL_HDR_H + ri as f64 * ss_row_h + NH / 2.0;
                    pos.insert(nd.id.clone(), (cx, cy));
                }
            }
        }
    }

    // ── Colour helpers ────────────────────────────────────────────────────
    let eeg_ts: Vec<u64> = nodes.iter()
        .filter(|n| n.kind == "eeg_point").filter_map(|n| n.timestamp_unix).collect();
    let ts_min = eeg_ts.iter().copied().min().unwrap_or(0);
    let ts_rng = eeg_ts.iter().copied().max().unwrap_or(1).saturating_sub(ts_min).max(1) as f64;
    let eeg_fill = |ts: Option<u64>| -> String {
        ts.map(|t| turbo_hex((t.saturating_sub(ts_min)) as f64 / ts_rng))
          .unwrap_or_else(|| "#f59e0b".into())
    };
    let node_fill = |nd: &InteractiveGraphNode| -> String {
        match nd.kind.as_str() {
            "query"       => "#8b5cf6".into(),
            "text_label"  => "#3b82f6".into(),
            "eeg_point"   => eeg_fill(nd.timestamp_unix),
            "found_label" => "#10b981".into(),
            "screenshot"  => "#ec4899".into(),
            _             => "#888888".into(),
        }
    };
    let half_h = |kind: &str| -> f64 {
        match kind { "query" => QR, "eeg_point" => EEG_S, "screenshot" => NH / 2.0 + 4.0, _ => NH / 2.0 }
    };
    let edge_col = |kind: &str| -> (&str, &str, &str) {
        match kind {
            "text_sim"        => ("#8b5cf6", "",    "mv"),
            "eeg_bridge"      => ("#f59e0b", "5,3", "ma"),
            "eeg_sim"         => ("#f59e0b", "2,3", "ma"),
            "label_prox"      => ("#10b981", "",    "me"),
            "screenshot_prox" => ("#ec4899", "5,3", "ms"),
            "ocr_sim"         => ("#ec4899", "2,3", "ms"),
            _                 => ("#999999", "",    "mg"),
        }
    };

    // ── SVG document ──────────────────────────────────────────────────────
    let mut o = String::with_capacity(64 * 1024);
    let w = svg_w.ceil() as i64;
    let h = svg_h.ceil() as i64;

    o.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="Helvetica Neue,Helvetica,Arial,sans-serif">
  <rect width="{w}" height="{h}" fill="#ffffff"/>
  <defs>
"##));
    for (id, col) in [("mv","#8b5cf6"),("ma","#f59e0b"),("me","#10b981"),("ms","#ec4899"),("mg","#999999")] {
        o.push_str(&format!(
            "    <marker id=\"{id}\" markerWidth=\"7\" markerHeight=\"5\" refX=\"6\" refY=\"2.5\" orient=\"auto\" markerUnits=\"strokeWidth\">\
             <path d=\"M0,0 L7,2.5 L0,5 Z\" fill=\"{col}\"/></marker>\n"));
    }
    o.push_str("  </defs>\n");

    // ── Layer bands ───────────────────────────────────────────────────────
    if !layers[0].is_empty() {
        let by = query_y - (QR + 8.0);
        let bh = (QR + 8.0) * 2.0;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{by:.1}\" width=\"{w}\" height=\"{bh:.1}\" fill=\"#8b5cf6\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#8b5cf6\" opacity=\"0.55\" font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            by + 13.0, svg_esc(&labels.layer_query)));
    }
    if has_tl {
        let bh = tl_band_bot - tl_band_top;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{tl_band_top:.1}\" width=\"{w}\" height=\"{bh:.1}\" \
             fill=\"#3b82f6\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#3b82f6\" opacity=\"0.55\" \
             font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            tl_band_top + 13.0, svg_esc(&labels.layer_text_matches)));

        let block_w  = DAY_LBL_W + tl_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        let grid_bot = tl_grid_top + tl_grid_h;

        for (ci, &hour) in tl_hours.iter().enumerate() {
            let hx = cells_x0 + ci as f64 * tl_col_w + NW / 2.0;
            o.push_str(&format!(
                "  <text x=\"{hx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                 font-size=\"8\" fill=\"#3b82f6\" opacity=\"0.75\">{hour:02}h</text>\n",
                tl_band_top + BAND_PAD + HOUR_LBL_H - 3.0));
        }
        let day_lbl_x = cells_x0 - 6.0;
        for (ri, day) in tl_days.iter().enumerate() {
            let row_top = tl_grid_top + ri as f64 * tl_cell_h;
            let row_cy  = row_top + tl_cell_h / 2.0;
            o.push_str(&format!(
                "  <text x=\"{day_lbl_x:.1}\" y=\"{row_cy:.1}\" text-anchor=\"end\" \
                 dominant-baseline=\"middle\" font-size=\"8\" fill=\"#999\">{}</text>\n",
                svg_esc(&day[5..])));
            if ri > 0 {
                o.push_str(&format!(
                    "  <line x1=\"{cells_x0:.1}\" y1=\"{row_top:.1}\" \
                     x2=\"{:.1}\" y2=\"{row_top:.1}\" \
                     stroke=\"#3b82f6\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                    cells_x0 + tl_grid_w));
            }
        }
        for ci in 0..=n_tl_hours {
            let lx = cells_x0 + ci as f64 * tl_col_w;
            o.push_str(&format!(
                "  <line x1=\"{lx:.1}\" y1=\"{tl_grid_top:.1}\" \
                 x2=\"{lx:.1}\" y2=\"{grid_bot:.1}\" \
                 stroke=\"#3b82f6\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n"));
        }
    }
    if has_eeg {
        let by = eeg_band_top;
        let bh = eeg_band_bot - by;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{by:.1}\" width=\"{w}\" height=\"{bh:.1}\" fill=\"#f59e0b\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#f59e0b\" opacity=\"0.55\" font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            by + 13.0, svg_esc(&labels.layer_eeg_neighbors)));

        let block_w  = DAY_LBL_W + eeg_grid_w;
        let cells_x0 = (svg_w - block_w) / 2.0 + DAY_LBL_W;
        let grid_bot = eeg_grid_top + n_eeg_days as f64 * EEG_CELL_H;

        for (ci, &hour) in eeg_hours.iter().enumerate() {
            let hx = cells_x0 + ci as f64 * EEG_CELL_W + EEG_CELL_W / 2.0;
            o.push_str(&format!(
                "  <text x=\"{hx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                 font-size=\"8\" fill=\"#f59e0b\" opacity=\"0.75\">{hour:02}h</text>\n",
                eeg_band_top + BAND_PAD + HOUR_LBL_H - 3.0));
        }
        let day_lbl_x = cells_x0 - 6.0;
        for (ri, day) in eeg_days.iter().enumerate() {
            let row_cy = eeg_grid_top + ri as f64 * EEG_CELL_H + EEG_CELL_H / 2.0;
            o.push_str(&format!(
                "  <text x=\"{day_lbl_x:.1}\" y=\"{row_cy:.1}\" text-anchor=\"end\" \
                 dominant-baseline=\"middle\" font-size=\"8\" fill=\"#999\">{}</text>\n",
                svg_esc(&day[5..])));
            if ri > 0 {
                let ry = eeg_grid_top + ri as f64 * EEG_CELL_H;
                o.push_str(&format!(
                    "  <line x1=\"{cells_x0:.1}\" y1=\"{ry:.1}\" \
                     x2=\"{:.1}\" y2=\"{ry:.1}\" \
                     stroke=\"#f59e0b\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                    cells_x0 + eeg_grid_w));
            }
        }
        for ci in 0..=n_eeg_hours {
            let lx = cells_x0 + ci as f64 * EEG_CELL_W;
            o.push_str(&format!(
                "  <line x1=\"{lx:.1}\" y1=\"{eeg_grid_top:.1}\" \
                 x2=\"{lx:.1}\" y2=\"{grid_bot:.1}\" \
                 stroke=\"#f59e0b\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n"));
        }
    }
    if has_fl {
        let bh = fl_band_bot - fl_band_top;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{fl_band_top:.1}\" width=\"{w}\" height=\"{bh:.1}\" \
             fill=\"#10b981\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#10b981\" opacity=\"0.55\" \
             font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            fl_band_top + 13.0, svg_esc(&labels.layer_found_labels)));

        if fl_has_proj {
            let scatter_left = (svg_w - fl_scatter_w) / 2.0;
            let scatter_top  = fl_grid_top;
            let scatter_bot  = fl_grid_top + fl_scatter_h;
            let scatter_cx   = svg_w / 2.0;
            let scatter_cy   = fl_grid_top + fl_scatter_h / 2.0;

            o.push_str(&format!(
                "  <rect x=\"{scatter_left:.1}\" y=\"{scatter_top:.1}\" \
                 width=\"{fl_scatter_w:.1}\" height=\"{fl_scatter_h:.1}\" \
                 rx=\"4\" fill=\"none\" stroke=\"#10b981\" stroke-opacity=\"0.18\" \
                 stroke-width=\"1\"/>\n"));
            o.push_str(&format!(
                "  <line x1=\"{:.1}\" y1=\"{scatter_cy:.1}\" \
                 x2=\"{:.1}\" y2=\"{scatter_cy:.1}\" \
                 stroke=\"#10b981\" stroke-opacity=\"0.12\" stroke-width=\"1\" \
                 stroke-dasharray=\"3,3\"/>\n",
                scatter_left + 8.0, scatter_left + fl_scatter_w - 8.0));
            o.push_str(&format!(
                "  <line x1=\"{scatter_cx:.1}\" y1=\"{:.1}\" \
                 x2=\"{scatter_cx:.1}\" y2=\"{:.1}\" \
                 stroke=\"#10b981\" stroke-opacity=\"0.12\" stroke-width=\"1\" \
                 stroke-dasharray=\"3,3\"/>\n",
                scatter_top + 8.0, scatter_bot - 8.0));
            o.push_str(&format!(
                "  <text x=\"{scatter_cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                 font-size=\"6.5\" fill=\"#10b981\" opacity=\"0.40\"\
                 >← text embedding similarity →</text>\n",
                scatter_bot - 2.5));
        } else {
            let x0_col0 = (svg_w - fl_grid_w) / 2.0;
            for (ci, parent_id) in fl_parents.iter().enumerate() {
                let col_left = x0_col0 + ci as f64 * fl_col_w;
                let col_cx   = col_left + NW / 2.0;
                let hdr_y    = fl_grid_top + FL_HDR_H - 3.0;

                if ci % 2 == 0 {
                    o.push_str(&format!(
                        "  <rect x=\"{col_left:.1}\" y=\"{fl_grid_top:.1}\" \
                         width=\"{NW:.1}\" height=\"{:.1}\" \
                         fill=\"#10b981\" fill-opacity=\"0.04\" rx=\"3\"/>\n",
                        fl_grid_h));
                }

                let hdr_text = parent_id.strip_prefix("ep_")
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|ts| {
                        let dt = fmt_unix_utc(ts);
                        format!("{} {}", &dt[5..10], &dt[11..])
                    })
                    .unwrap_or_default();
                o.push_str(&format!(
                    "  <text x=\"{col_cx:.1}\" y=\"{hdr_y:.1}\" text-anchor=\"middle\" \
                     font-size=\"7.5\" fill=\"#10b981\" opacity=\"0.75\">{}</text>\n",
                    svg_esc(&hdr_text)));

                if ci > 0 {
                    o.push_str(&format!(
                        "  <line x1=\"{col_left:.1}\" y1=\"{fl_grid_top:.1}\" \
                         x2=\"{col_left:.1}\" y2=\"{:.1}\" \
                         stroke=\"#10b981\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                        fl_grid_top + fl_grid_h));
                }
            }
        }
    }

    // ── Screenshot band ─────────────────────────────────────────────────
    if has_ss {
        let bh = ss_band_bot - ss_band_top;
        o.push_str(&format!(
            "  <rect x=\"0\" y=\"{ss_band_top:.1}\" width=\"{w}\" height=\"{bh:.1}\" \
             fill=\"#ec4899\" fill-opacity=\"0.05\" rx=\"4\"/>\n\
             <text x=\"10\" y=\"{:.1}\" font-size=\"9\" fill=\"#ec4899\" opacity=\"0.55\" \
             font-weight=\"600\" letter-spacing=\"1\">{}</text>\n",
            ss_band_top + 13.0, svg_esc(&labels.layer_screenshots)));

        let x0_col0 = (svg_w - ss_grid_w) / 2.0;
        for (ci, parent_id) in ss_parents.iter().enumerate() {
            let col_left = x0_col0 + ci as f64 * ss_col_w;
            let col_cx   = col_left + NW / 2.0;
            let hdr_y    = ss_grid_top + FL_HDR_H - 3.0;

            if ci % 2 == 0 {
                o.push_str(&format!(
                    "  <rect x=\"{col_left:.1}\" y=\"{ss_grid_top:.1}\" \
                     width=\"{NW:.1}\" height=\"{:.1}\" \
                     fill=\"#ec4899\" fill-opacity=\"0.04\" rx=\"3\"/>\n",
                    ss_grid_h));
            }

            let hdr_text = parent_id.strip_prefix("ep_")
                .and_then(|s| s.parse::<u64>().ok())
                .map(|ts| {
                    let dt = fmt_unix_utc(ts);
                    format!("{} {}", &dt[5..10], &dt[11..])
                })
                .unwrap_or_default();
            o.push_str(&format!(
                "  <text x=\"{col_cx:.1}\" y=\"{hdr_y:.1}\" text-anchor=\"middle\" \
                 font-size=\"7.5\" fill=\"#ec4899\" opacity=\"0.75\">{}</text>\n",
                svg_esc(&hdr_text)));

            if ci > 0 {
                o.push_str(&format!(
                    "  <line x1=\"{col_left:.1}\" y1=\"{ss_grid_top:.1}\" \
                     x2=\"{col_left:.1}\" y2=\"{:.1}\" \
                     stroke=\"#ec4899\" stroke-opacity=\"0.2\" stroke-width=\"1\"/>\n",
                    ss_grid_top + ss_grid_h));
            }
        }
    }

    // ── Edges ─────────────────────────────────────────────────────────────
    for e in edges {
        let (Some(&(x1,y1)), Some(&(x2,y2))) = (pos.get(&e.from_id), pos.get(&e.to_id))
            else { continue };
        let dx = x2 - x1; let dy = y2 - y1;
        let len = (dx*dx + dy*dy).sqrt().max(1.0);
        let src_h = nodes.iter().find(|n| n.id == e.from_id).map(|n| half_h(&n.kind)).unwrap_or(NH / 2.0);
        let dst_h = nodes.iter().find(|n| n.id == e.to_id  ).map(|n| half_h(&n.kind)).unwrap_or(NH / 2.0);
        let sx1 = x1 + dx/len*(src_h + 2.0); let sy1 = y1 + dy/len*(src_h + 2.0);
        let sx2 = x2 - dx/len*(dst_h + 9.0); let sy2 = y2 - dy/len*(dst_h + 9.0);
        let midy = (sy1 + sy2) / 2.0;
        let cp1y = sy1 + (midy - sy1) * 0.55;
        let cp2y = sy2 - (sy2 - midy) * 0.55;
        let (col, dash, mid) = edge_col(&e.kind);
        let da = if dash.is_empty() { String::new() }
                 else { format!(" stroke-dasharray=\"{dash}\"") };
        o.push_str(&format!(
            "  <path d=\"M{sx1:.1},{sy1:.1} C{x1:.1},{cp1y:.1} {x2:.1},{cp2y:.1} {sx2:.1},{sy2:.1}\" \
             fill=\"none\" stroke=\"{col}\" stroke-width=\"1.8\" opacity=\"0.65\"{da} marker-end=\"url(#{mid})\"/>\n"));
    }

    // ── Nodes ─────────────────────────────────────────────────────────────
    for nd in nodes {
        let Some(&(cx, cy)) = pos.get(&nd.id) else { continue };
        let fill = node_fill(nd);

        match nd.kind.as_str() {
            "query" => {
                o.push_str(&format!(
                    "  <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{ro:.1}\" fill=\"{fill}\" fill-opacity=\"0.18\" stroke=\"{fill}\" stroke-width=\"2\"/>\n\
                     <circle cx=\"{cx:.1}\" cy=\"{cy:.1}\" r=\"{QR:.1}\" fill=\"{fill}\" fill-opacity=\"0.92\"/>\n",
                    ro = QR + 8.0));
            }
            "text_label" => {
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{NW:.1}\" height=\"{NH:.1}\" rx=\"6\" \
                     fill=\"{fill}\" fill-opacity=\"0.90\"/>\n",
                    cx - NW / 2.0, cy - NH / 2.0));
            }
            "found_label" => {
                o.push_str(&format!(
                    "  <ellipse cx=\"{cx:.1}\" cy=\"{cy:.1}\" rx=\"{:.1}\" ry=\"{:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"0.90\"/>\n",
                    NW / 2.0, NH / 2.0));
            }
            "eeg_point" => {
                let s = EEG_S;
                o.push_str(&format!(
                    "  <polygon points=\"{cx:.1},{:.1} {:.1},{cy:.1} {cx:.1},{:.1} {:.1},{cy:.1}\" \
                     fill=\"{fill}\" fill-opacity=\"0.92\"/>\n",
                    cy - s, cx + s * 1.35, cy + s, cx - s * 1.35));
            }
            "screenshot" => {
                // Rounded rect with a small camera icon accent
                let rw = NW + 8.0;
                let rh = NH + 8.0;
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{rw:.1}\" height=\"{rh:.1}\" rx=\"8\" \
                     fill=\"{fill}\" fill-opacity=\"0.88\" stroke=\"{fill}\" stroke-width=\"1.5\" stroke-opacity=\"0.4\"/>\n",
                    cx - rw / 2.0, cy - rh / 2.0));
                // Small camera glyph (top-right corner)
                let gx = cx + rw / 2.0 - 12.0;
                let gy = cy - rh / 2.0 + 10.0;
                o.push_str(&format!(
                    "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"8\" height=\"6\" rx=\"1\" \
                     fill=\"white\" fill-opacity=\"0.55\"/>\n\
                     <circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"1.5\" fill=\"{fill}\" fill-opacity=\"0.75\"/>\n",
                    gx, gy, gx + 4.0, gy + 3.0));
            }
            _ => {}
        }

        match nd.kind.as_str() {
            "eeg_point" => {
                let time_str = nd.timestamp_unix.map(|ts| {
                    let h = (ts % 86400) / 3600;
                    let m = (ts % 3600)  / 60;
                    format!("{h:02}:{m:02}")
                }).unwrap_or_default();
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{cy:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"7\" font-weight=\"600\" fill=\"white\">{}</text>\n",
                    svg_esc(&time_str)));
            }
            "screenshot" => {
                let title = nd.window_title.as_deref()
                    .or(nd.app_name.as_deref())
                    .unwrap_or("screenshot");
                let primary = trunc(title, 18);
                let ty = if nd.timestamp_unix.is_some() { cy - 7.0 } else { cy };
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{ty:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"9\" font-weight=\"600\" fill=\"white\">{}</text>\n",
                    svg_esc(&primary)));
                if let Some(ts) = nd.timestamp_unix {
                    o.push_str(&format!(
                        "  <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                         dominant-baseline=\"middle\" font-size=\"7\" fill=\"white\" opacity=\"0.72\">{}</text>\n",
                        cy + 8.5, svg_esc(&fmt_unix_utc(ts))));
                }
            }
            _ => {
                let primary = trunc(nd.text.as_deref().unwrap_or(""), 20);
                let has_sub = nd.timestamp_unix.is_some()
                    && matches!(nd.kind.as_str(), "text_label" | "found_label");
                let ty = if has_sub { cy - 7.0 } else { cy };
                o.push_str(&format!(
                    "  <text x=\"{cx:.1}\" y=\"{ty:.1}\" text-anchor=\"middle\" \
                     dominant-baseline=\"middle\" font-size=\"10\" font-weight=\"600\" fill=\"white\">{}</text>\n",
                    svg_esc(&primary)));
                if has_sub {
                    if let Some(ts) = nd.timestamp_unix {
                        o.push_str(&format!(
                            "  <text x=\"{cx:.1}\" y=\"{:.1}\" text-anchor=\"middle\" \
                             dominant-baseline=\"middle\" font-size=\"7.5\" fill=\"white\" opacity=\"0.72\">{}</text>\n",
                            cy + 8.5, svg_esc(&fmt_unix_utc(ts))));
                    }
                }
            }
        }
    }

    // ── Legend ────────────────────────────────────────────────────────────
    let legend_y = svg_h - 30.0;
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
    let lw = 72.0_f64;
    let lx0 = (svg_w - lw * legend_items.len() as f64) / 2.0;
    for (i, (col, lbl)) in legend_items.iter().enumerate() {
        let x = lx0 + i as f64 * lw;
        o.push_str(&format!(
            "  <circle cx=\"{:.1}\" cy=\"{legend_y:.1}\" r=\"4.5\" fill=\"{col}\" opacity=\"0.85\"/>\n\
             <text x=\"{:.1}\" y=\"{legend_y:.1}\" dominant-baseline=\"middle\" font-size=\"8.5\" fill=\"#555\">{}</text>\n",
            x + 4.5, x + 13.0, svg_esc(lbl)));
    }

    let footer_y = svg_h - 12.0;
    o.push_str(&format!(
        "  <text x=\"{:.1}\" y=\"{footer_y:.1}\" text-anchor=\"middle\" \
         font-size=\"7.5\" fill=\"#aaa\">{}</text>\n",
        svg_w / 2.0, svg_esc(&labels.generated_by)));

    o.push_str("</svg>\n");
    o
}


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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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
}
