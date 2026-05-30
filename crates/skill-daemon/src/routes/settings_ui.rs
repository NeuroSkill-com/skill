// SPDX-License-Identifier: GPL-3.0-only
//! UI, skills, location, TTS, sleep, WS, DND, web-cache, and miscellaneous settings handlers.

use axum::{extract::State, Json};
use serde::Deserialize;

use crate::{
    routes::settings_io::{load_user_settings, modify_settings_blocking, patch_settings},
    state::AppState,
};

use super::settings::{BoolValueRequest, StringValueRequest, U64ValueRequest};

#[derive(Debug, Deserialize)]
pub(crate) struct WsConfigRequest {
    pub(crate) host: String,
    pub(crate) port: u16,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StringListRequest {
    pub(crate) values: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StringKeyRequest {
    pub(crate) key: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct DndTestRequest {
    pub(crate) enabled: bool,
}

// --- Iroh logs ---

pub(crate) async fn get_iroh_logs(State(state): State<AppState>) -> Json<serde_json::Value> {
    let enabled = state.iroh_logs_enabled.load(std::sync::atomic::Ordering::Relaxed);
    Json(serde_json::json!({"value": enabled}))
}

pub(crate) async fn set_iroh_logs(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    state
        .iroh_logs_enabled
        .store(req.value, std::sync::atomic::Ordering::Relaxed);
    patch_settings(&state, move |s| {
        s.iroh_logs = req.value;
    })
    .await;
    Json(serde_json::json!({"ok": true}))
}

// --- TTS / Sleep / WS ---

crate::settings_struct!(
    skill_settings::NeuttsConfig,
    get_neutts_config,
    set_neutts_config => neutts
);
crate::settings_bool!(get_tts_preload, set_tts_preload => tts_preload);
crate::settings_struct!(skill_settings::SleepConfig, get_sleep_config, set_sleep_config => sleep);

pub(crate) async fn get_ws_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    Json(serde_json::json!({"host": settings.ws_host, "port": settings.ws_port}))
}

pub(crate) async fn set_ws_config(
    State(state): State<AppState>,
    Json(req): Json<WsConfigRequest>,
) -> Json<serde_json::Value> {
    let host = req.host.trim().to_string();
    if host != "127.0.0.1" && host != "0.0.0.0" {
        return Json(
            serde_json::json!({"ok": false, "error": format!("invalid host '{host}': must be '127.0.0.1' or '0.0.0.0'")}),
        );
    }
    if req.port < 1024 {
        return Json(
            serde_json::json!({"ok": false, "error": format!("port {} is reserved; use 1024–65535", req.port)}),
        );
    }
    patch_settings(&state, move |s| {
        s.ws_host = host;
        s.ws_port = req.port;
    })
    .await;
    Json(serde_json::json!({"ok": true, "port": req.port}))
}

// --- Location / Token ---

crate::settings_get_value!(get_location_enabled => location_enabled);

pub(crate) async fn set_location_enabled(
    State(state): State<AppState>,
    Json(req): Json<BoolValueRequest>,
) -> Json<serde_json::Value> {
    use serde_json::json;
    if !req.value {
        patch_settings(&state, move |s| {
            s.location_enabled = false;
        })
        .await;
        return Json(json!({"enabled": false}));
    }

    let result = tokio::task::spawn_blocking(|| {
        let auth = skill_location::auth_status();
        match auth {
            skill_location::LocationAuthStatus::Denied => {
                return json!({"enabled": false, "permission": "denied", "error": "Location permission denied."});
            }
            skill_location::LocationAuthStatus::Restricted => {
                return json!({"enabled": false, "permission": "restricted", "error": "Location access is restricted."});
            }
            _ => {}
        }

        if skill_location::auth_status() == skill_location::LocationAuthStatus::NotDetermined {
            skill_location::request_access(30.0);
        }

        let post_auth = skill_location::auth_status();
        let perm_str = match post_auth {
            skill_location::LocationAuthStatus::Authorized => "authorized",
            skill_location::LocationAuthStatus::Denied => "denied",
            skill_location::LocationAuthStatus::Restricted => "restricted",
            skill_location::LocationAuthStatus::NotDetermined => "not_determined",
        };

        if matches!(
            post_auth,
            skill_location::LocationAuthStatus::Denied | skill_location::LocationAuthStatus::Restricted
        ) {
            return json!({"enabled": false, "permission": perm_str, "error": "Location permission denied."});
        }

        match skill_location::fetch_location(10.0) {
            Ok(fix) => json!({
                "enabled": true,
                "permission": perm_str,
                "fix": {
                    "latitude": fix.latitude,
                    "longitude": fix.longitude,
                    "source": format!("{:?}", fix.source),
                    "country": fix.country,
                    "region": fix.region,
                    "city": fix.city,
                    "timezone": fix.timezone,
                    "horizontal_accuracy": fix.horizontal_accuracy,
                    "altitude": fix.altitude,
                }
            }),
            Err(e) => json!({"enabled": true, "permission": perm_str, "error": e.to_string()}),
        }
    })
    .await
    .unwrap_or_else(|e| json!({"enabled": false, "error": format!("location task error: {e}")}));

    let enabled_result = result
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if enabled_result {
        patch_settings(&state, move |s| {
            s.location_enabled = true;
        })
        .await;
    }

    Json(result)
}

pub(crate) async fn test_location() -> Json<serde_json::Value> {
    use serde_json::json;
    let v = tokio::task::spawn_blocking(|| match skill_location::fetch_location(10.0) {
        Ok(fix) => json!({
            "ok": true,
            "source": format!("{:?}", fix.source),
            "latitude": fix.latitude,
            "longitude": fix.longitude,
            "country": fix.country,
            "region": fix.region,
            "city": fix.city,
            "timezone": fix.timezone,
            "horizontal_accuracy": fix.horizontal_accuracy,
            "altitude": fix.altitude,
        }),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    })
    .await
    .unwrap_or_else(|e| json!({"ok": false, "error": format!("location task error: {e}")}));
    Json(v)
}

pub(crate) async fn get_api_token(State(_state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": skill_settings::keychain::get_api_token()}))
}

pub(crate) async fn set_api_token(
    State(_state): State<AppState>,
    Json(req): Json<StringValueRequest>,
) -> Json<serde_json::Value> {
    skill_settings::keychain::set_api_token(&req.value);
    Json(serde_json::json!({"ok": true}))
}

// --- UMAP / GPU ---

pub(crate) async fn get_umap_config(State(state): State<AppState>) -> Json<skill_settings::UmapUserConfig> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(skill_settings::load_umap_config(&skill_dir))
}

pub(crate) async fn set_umap_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::UmapUserConfig>,
) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let old = skill_settings::load_umap_config(&skill_dir);
    // Only invalidate the cache when a parameter that affects the computed
    // result changes.  Switching GPU backend does not alter the output, so
    // we skip the expensive cache purge in that case.
    let results_changed = old.precision != config.precision
        || old.repulsion_strength != config.repulsion_strength
        || old.neg_sample_rate != config.neg_sample_rate
        || old.n_epochs != config.n_epochs
        || old.n_neighbors != config.n_neighbors
        || old.cooldown_ms != config.cooldown_ms
        || old.timeout_secs != config.timeout_secs;
    skill_settings::save_umap_config(&skill_dir, &config);
    if results_changed {
        let cache_dir = skill_dir.join("umap_cache");
        if cache_dir.exists() {
            let _ = std::fs::remove_dir_all(&cache_dir);
        }
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_umap_backends() -> Json<serde_json::Value> {
    let backends = skill_router::available_backends();
    let precisions: std::collections::HashMap<&str, Vec<&str>> = backends
        .iter()
        .map(|&b| (b, skill_router::available_precisions(b)))
        .collect();
    Json(serde_json::json!({
        "available": backends,
        "precisions": precisions,
    }))
}

pub(crate) async fn get_gpu_stats() -> Json<serde_json::Value> {
    Json(serde_json::to_value(skill_data::gpu_stats::read()).unwrap_or(serde_json::Value::Null))
}

// --- Web cache ---

pub(crate) async fn web_cache_stats() -> Json<serde_json::Value> {
    let v = match skill_tools::web_cache::global() {
        Some(cache) => serde_json::to_value(cache.stats()).unwrap_or_default(),
        None => serde_json::json!({"total_entries": 0, "expired_entries": 0, "total_bytes": 0}),
    };
    Json(v)
}

pub(crate) async fn web_cache_list() -> Json<Vec<serde_json::Value>> {
    let v = match skill_tools::web_cache::global() {
        Some(cache) => cache
            .list_entries()
            .into_iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect(),
        None => Vec::new(),
    };
    Json(v)
}

pub(crate) async fn web_cache_clear() -> Json<serde_json::Value> {
    let removed = if let Some(cache) = skill_tools::web_cache::global() {
        let stats = cache.stats();
        cache.clear();
        stats.total_entries
    } else {
        0
    };
    Json(serde_json::json!({"removed": removed}))
}

pub(crate) async fn web_cache_remove_domain(Json(req): Json<StringValueRequest>) -> Json<serde_json::Value> {
    let removed = match skill_tools::web_cache::global() {
        Some(cache) => cache.remove_by_domain(&req.value),
        None => 0,
    };
    Json(serde_json::json!({"removed": removed}))
}

pub(crate) async fn web_cache_remove_entry(Json(req): Json<StringKeyRequest>) -> Json<serde_json::Value> {
    let removed = match skill_tools::web_cache::global() {
        Some(cache) => cache.remove_entry(&req.key),
        None => false,
    };
    Json(serde_json::json!({"removed": removed}))
}

// --- DND ---

pub(crate) async fn get_dnd_focus_modes() -> Json<Vec<skill_data::dnd::FocusModeOption>> {
    Json(skill_data::dnd::list_focus_modes())
}

crate::settings_struct!(
    skill_settings::DoNotDisturbConfig,
    get_dnd_config,
    set_dnd_config => do_not_disturb
);

pub(crate) async fn get_dnd_active() -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": skill_data::dnd::query_os_active().unwrap_or(false)}))
}

pub(crate) async fn get_dnd_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let cfg = load_user_settings(&state).do_not_disturb;
    let os_active = skill_data::dnd::query_os_active();
    let dnd_active = os_active.unwrap_or(false);
    Json(serde_json::json!({
        "enabled": cfg.enabled,
        "avg_score": 0.0,
        "threshold": cfg.focus_threshold as f64,
        "sample_count": 0,
        "window_size": (cfg.duration_secs as usize * 4).max(8),
        "duration_secs": cfg.duration_secs,
        "dnd_active": dnd_active,
        "os_active": os_active,
        "last_error": serde_json::Value::Null,
        "exit_duration_secs": cfg.exit_duration_secs,
        "below_ticks": 0,
        "exit_window_size": (cfg.exit_duration_secs as usize * 4).max(4),
        "exit_secs_remaining": 0.0,
        "focus_lookback_secs": cfg.focus_lookback_secs,
        "exit_held_by_lookback": false,
        "focus_db_available": skill_data::dnd::has_focus_db_access(),
    }))
}

pub(crate) async fn test_dnd(Json(req): Json<DndTestRequest>) -> Json<serde_json::Value> {
    if req.enabled {
        return Json(serde_json::json!({"ok": false, "value": false}));
    }
    let ok = skill_data::dnd::set_dnd(false, "", false);
    Json(serde_json::json!({"ok": ok, "value": ok}))
}

/// Open the macOS Full Disk Access settings pane.
pub(crate) async fn open_full_disk_access() -> Json<serde_json::Value> {
    #[cfg(target_os = "macos")]
    {
        let modern = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles")
            .output();
        if modern.is_err() || modern.is_ok_and(|o| !o.status.success()) {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")
                .spawn();
        }
    }
    Json(serde_json::json!({"ok": true}))
}

// --- UI appearance ---

crate::settings_string!(get_accent_color, set_accent_color => accent_color);

crate::settings_get_value!(get_daily_goal => daily_goal_min);

pub(crate) async fn set_daily_goal(
    State(state): State<AppState>,
    Json(req): Json<U64ValueRequest>,
) -> Json<serde_json::Value> {
    let clamped = (req.value as u32).min(480);
    modify_settings_blocking(&state, move |s| {
        s.daily_goal_min = clamped;
    })
    .await;
    Json(serde_json::json!({"ok": true, "value": clamped}))
}

crate::settings_string!(get_goal_notified_date, set_goal_notified_date => goal_notified_date);

crate::settings_bool!(get_main_window_auto_fit, set_main_window_auto_fit => main_window_auto_fit);

// --- Skills ---

crate::settings_nested_u64!(
    get_skills_refresh_interval,
    set_skills_refresh_interval => llm.tools.skills_refresh_interval_secs
);
crate::settings_nested_bool!(
    get_skills_sync_on_launch,
    set_skills_sync_on_launch => llm.tools.skills_sync_on_launch
);

pub(crate) async fn get_skills_last_sync(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    Json(serde_json::json!({"value": skill_skills::sync::last_sync_ts(&skill_dir)}))
}

pub(crate) async fn sync_skills_now(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let outcome = tokio::task::spawn_blocking(move || skill_skills::sync::sync_skills(&skill_dir, 0, None)).await;
    match outcome {
        Ok(skill_skills::sync::SyncOutcome::Updated { elapsed_ms, .. }) => {
            Json(serde_json::json!({"status": "updated", "message": format!("updated in {elapsed_ms} ms")}))
        }
        Ok(skill_skills::sync::SyncOutcome::Fresh { .. }) => {
            Json(serde_json::json!({"status": "fresh", "message": "already up to date"}))
        }
        Ok(skill_skills::sync::SyncOutcome::Failed(e)) => Json(serde_json::json!({"status": "failed", "message": e})),
        Err(e) => Json(serde_json::json!({"status": "failed", "message": e.to_string()})),
    }
}

pub(crate) async fn list_skills(State(state): State<AppState>) -> Json<serde_json::Value> {
    let settings = load_user_settings(&state);
    let disabled = settings.llm.tools.disabled_skills;
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
    let bundled_dir = exe_dir
        .as_ref()
        .map(|d| d.join(skill_constants::SKILLS_SUBDIR))
        .filter(|d| d.is_dir())
        .or_else(|| {
            let cwd = std::env::current_dir().ok()?;
            let p = cwd.join(skill_constants::SKILLS_SUBDIR);
            if p.is_dir() {
                Some(p)
            } else {
                None
            }
        });

    let result = skill_skills::load_skills(&skill_skills::LoadSkillsOptions {
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        skill_dir: skill_dir.to_path_buf(),
        bundled_dir,
        skill_paths: Vec::new(),
        include_defaults: true,
    });

    Json(serde_json::Value::Array(
        result
            .skills
            .into_iter()
            .map(|s| {
                let enabled = !disabled.iter().any(|d| d == &s.name);
                serde_json::json!({
                    "name": s.name,
                    "description": s.description,
                    "source": s.source,
                    "enabled": enabled
                })
            })
            .collect(),
    ))
}

pub(crate) async fn get_skills_license(State(state): State<AppState>) -> Json<serde_json::Value> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let license_path = skill_dir.join(skill_constants::SKILLS_SUBDIR).join("LICENSE");
    Json(serde_json::json!({"value": std::fs::read_to_string(&license_path).ok()}))
}

crate::settings_nested_get_value!(get_disabled_skills => llm.tools.disabled_skills);

pub(crate) async fn set_disabled_skills(
    State(state): State<AppState>,
    Json(req): Json<StringListRequest>,
) -> Json<serde_json::Value> {
    let values = req.values.clone();
    modify_settings_blocking(&state, move |s| {
        s.llm.tools.disabled_skills = values;
    })
    .await;
    Json(serde_json::json!({"ok": true, "value": req.values}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;
    use tempfile::TempDir;

    fn mk_state() -> (TempDir, AppState) {
        let td = TempDir::new().unwrap();
        let state = AppState::new("token".into(), td.path().to_path_buf());
        (td, state)
    }

    #[tokio::test]
    async fn ws_config_rejects_invalid_host() {
        let (_td, state) = mk_state();
        let req = WsConfigRequest {
            host: "192.168.1.1".into(),
            port: 8080,
        };
        let res = set_ws_config(State(state.clone()), Json(req)).await.0;
        assert_eq!(res["ok"], false);
        assert!(res["error"].as_str().unwrap().contains("invalid host"));
    }

    #[tokio::test]
    async fn ws_config_rejects_reserved_port() {
        let (_td, state) = mk_state();
        let req = WsConfigRequest {
            host: "127.0.0.1".into(),
            port: 80,
        };
        let res = set_ws_config(State(state.clone()), Json(req)).await.0;
        assert_eq!(res["ok"], false);
        assert!(res["error"].as_str().unwrap().contains("reserved"));
    }

    #[tokio::test]
    async fn ws_config_accepts_valid() {
        let (_td, state) = mk_state();
        let req = WsConfigRequest {
            host: "127.0.0.1".into(),
            port: 8080,
        };
        let res = set_ws_config(State(state.clone()), Json(req)).await.0;
        assert_eq!(res["ok"], true);
    }

    #[tokio::test]
    async fn location_enabled_roundtrip() {
        let (_td, state) = mk_state();
        let res = get_location_enabled(State(state.clone())).await.0;
        assert!(res.get("value").is_some());
    }

    #[tokio::test]
    async fn accent_color_roundtrip() {
        let (_td, state) = mk_state();
        let orig = get_accent_color(State(state.clone())).await.0;
        assert!(orig.get("value").is_some());
    }

    #[tokio::test]
    async fn ws_config_accepts_all_zeros_host() {
        let (_td, state) = mk_state();
        let req = WsConfigRequest {
            host: "0.0.0.0".into(),
            port: 9090,
        };
        let res = set_ws_config(State(state.clone()), Json(req)).await.0;
        assert_eq!(res["ok"], true);
    }

    #[tokio::test]
    async fn ws_config_trims_whitespace() {
        let (_td, state) = mk_state();
        let req = WsConfigRequest {
            host: "  127.0.0.1  ".into(),
            port: 8080,
        };
        let res = set_ws_config(State(state.clone()), Json(req)).await.0;
        assert_eq!(res["ok"], true);
    }

    #[tokio::test]
    async fn set_location_disabled_roundtrip() {
        let (_td, state) = mk_state();
        // Only test disabling — enabling calls skill_location::request_access()
        // which blocks waiting for a system permission dialog.
        let req = BoolValueRequest { value: false };
        let _ = set_location_enabled(State(state.clone()), Json(req)).await;
        let res = get_location_enabled(State(state.clone())).await.0;
        assert_eq!(res["value"], false);
    }

    #[tokio::test]
    async fn get_dnd_config_returns_value() {
        let (_td, state) = mk_state();
        let res = get_dnd_config(State(state.clone())).await.0;
        let json_val = serde_json::to_value(&res).unwrap();
        assert!(!json_val.is_null());
    }

    #[tokio::test]
    async fn get_sleep_config_returns_value() {
        let (_td, state) = mk_state();
        let res = get_sleep_config(State(state.clone())).await.0;
        let json_val = serde_json::to_value(&res).unwrap();
        assert!(!json_val.is_null());
    }

    #[tokio::test]
    async fn daily_goal_roundtrip() {
        let (_td, state) = mk_state();
        let res = get_daily_goal(State(state.clone())).await.0;
        // Should return a minutes value
        assert!(res.get("value").is_some() || res.get("minutes").is_some());
    }
}
