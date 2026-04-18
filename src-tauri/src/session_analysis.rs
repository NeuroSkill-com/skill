// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Session analysis — most computation delegated to daemon via frontend daemonInvoke.
// Only window commands remain here.

use tauri::{AppHandle, Manager};

// Re-export types from skill-history for backward compatibility with callers.
pub(crate) use skill_history::CsvMetricsResult;

fn daemon_analysis(path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    crate::daemon_cmds::post_json_value_with_auth(path, &body)
}

#[tauri::command]
pub(crate) async fn get_day_metrics_batch(
    csv_paths: Vec<String>,
    max_ts_points: Option<usize>,
) -> Result<std::collections::HashMap<String, CsvMetricsResult>, String> {
    let val = daemon_analysis(
        "/v1/analysis/day-metrics",
        serde_json::json!({ "csv_paths": csv_paths, "max_ts_points": max_ts_points }),
    )?;
    serde_json::from_value(val).map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) async fn open_compare_window(app: AppHandle) -> Result<(), String> {
    crate::window_cmds::focus_or_create(
        &app,
        crate::window_cmds::WindowSpec {
            label: "compare",
            route: "compare",
            title: "NeuroSkill™ – Compare",
            inner_size: (780.0, 640.0),
            min_inner_size: Some((600.0, 440.0)),
            ..Default::default()
        },
    )
}

#[tauri::command]
pub(crate) async fn open_compare_window_with_sessions(
    app: AppHandle,
    start_a: i64,
    end_a: i64,
    start_b: i64,
    end_b: i64,
) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("compare") {
        // Navigate away first so WebKit tears down its ScrollingTree
        // before the window is destroyed (prevents SIGSEGV in WebCore).
        let _ = win.eval("window.stop()");
        if let Ok(url) = "about:blank".parse() {
            let _ = win.navigate(url);
        }
        let _ = win.close();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let url_path = format!(
        "compare?startA={}&endA={}&startB={}&endB={}",
        start_a, end_a, start_b, end_b
    );
    tauri::WebviewWindowBuilder::new(&app, "compare", tauri::WebviewUrl::App(url_path.into()))
        .title("NeuroSkill™ – Compare")
        .inner_size(780.0, 640.0)
        .min_inner_size(600.0, 440.0)
        .resizable(true)
        .center()
        .decorations(false)
        .transparent(true)
        .build()
        .map(|w| {
            let _ = w.set_focus();
        })
        .map_err(|e| e.to_string())
}
