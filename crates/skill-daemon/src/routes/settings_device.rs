// SPDX-License-Identifier: GPL-3.0-only
//! Device, filter, and storage configuration handlers.

use axum::{extract::State, Json};
use serde::Deserialize;
use skill_eeg::eeg_filter::{FilterConfig, PowerlineFreq};

use crate::{
    routes::settings_io::{load_user_settings, save_user_settings},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub(crate) struct NotchPresetRequest {
    pub(crate) value: Option<PowerlineFreq>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct U64ValueRequest {
    pub(crate) value: u64,
}

pub(crate) async fn get_filter_config(State(state): State<AppState>) -> Json<FilterConfig> {
    Json(load_user_settings(&state).filter_config)
}

pub(crate) async fn set_filter_config(
    State(state): State<AppState>,
    Json(config): Json<FilterConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.filter_config = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn set_notch_preset(
    State(state): State<AppState>,
    Json(req): Json<NotchPresetRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.filter_config.notch = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_storage_format(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).storage_format}))
}

pub(crate) async fn set_storage_format(
    State(state): State<AppState>,
    Json(req): Json<super::settings::StringValueRequest>,
) -> Json<serde_json::Value> {
    let fmt = match req.value.to_ascii_lowercase().as_str() {
        "parquet" => "parquet",
        "both" => "both",
        _ => "csv",
    };
    let mut settings = load_user_settings(&state);
    settings.storage_format = fmt.to_string();
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": fmt}))
}

pub(crate) async fn get_embedding_overlap(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).embedding_overlap_secs}))
}

pub(crate) async fn set_embedding_overlap(
    State(state): State<AppState>,
    Json(req): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let overlap = req
        .get("value")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(skill_constants::EMBEDDING_OVERLAP_SECS as f64) as f32;
    let clamped = overlap.clamp(
        skill_constants::EMBEDDING_OVERLAP_MIN_SECS,
        skill_constants::EMBEDDING_OVERLAP_MAX_SECS,
    );
    let mut settings = load_user_settings(&state);
    settings.embedding_overlap_secs = clamped;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": clamped}))
}

pub(crate) async fn get_update_check_interval(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({"value": load_user_settings(&state).update_check_interval_secs}))
}

pub(crate) async fn set_update_check_interval(
    State(state): State<AppState>,
    Json(req): Json<U64ValueRequest>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.update_check_interval_secs = req.value;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true, "value": req.value}))
}

pub(crate) async fn get_openbci_config(State(state): State<AppState>) -> Json<skill_settings::OpenBciConfig> {
    Json(load_user_settings(&state).openbci)
}

pub(crate) async fn set_openbci_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::OpenBciConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.openbci = config.clone();
    save_user_settings(&state, &settings);
    if let Ok(mut wifi) = state.scanner_wifi_config.lock() {
        wifi.wifi_shield_ip = config.wifi_shield_ip;
        wifi.galea_ip = config.galea_ip;
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_device_api_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let c = load_user_settings(&state).device_api;
    let (emotiv_client_id, emotiv_client_secret) = skill_settings::keychain::get_emotiv_credentials();
    let idun_api_token = skill_settings::keychain::get_idun_api_token();
    let oura_access_token = skill_settings::keychain::get_oura_access_token();
    let (neurosity_email, neurosity_password, neurosity_device_id) =
        skill_settings::keychain::get_neurosity_credentials();
    Json(serde_json::json!({
        "emotiv_client_id": emotiv_client_id,
        "emotiv_client_secret": emotiv_client_secret,
        "idun_api_token": idun_api_token,
        "oura_access_token": oura_access_token,
        "neurosity_email": neurosity_email,
        "neurosity_password": neurosity_password,
        "neurosity_device_id": neurosity_device_id,
        "brainmaster_model": c.brainmaster_model,
    }))
}

pub(crate) async fn set_device_api_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::DeviceApiConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.device_api = config.clone();
    save_user_settings(&state, &settings);
    skill_settings::keychain::save_device_api_secrets(&skill_settings::keychain::Secrets {
        api_token: String::new(),
        emotiv_client_id: config.emotiv_client_id.clone(),
        emotiv_client_secret: config.emotiv_client_secret.clone(),
        idun_api_token: config.idun_api_token.clone(),
        oura_access_token: config.oura_access_token.clone(),
        neurosity_email: config.neurosity_email.clone(),
        neurosity_password: config.neurosity_password.clone(),
        neurosity_device_id: config.neurosity_device_id.clone(),
    });
    if let Ok(mut cortex) = state.scanner_cortex_config.lock() {
        cortex.emotiv_client_id = config.emotiv_client_id;
        cortex.emotiv_client_secret = config.emotiv_client_secret;
    }
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn get_scanner_config(State(state): State<AppState>) -> Json<skill_settings::ScannerConfig> {
    Json(load_user_settings(&state).scanner)
}

pub(crate) async fn get_device_log(State(state): State<AppState>) -> Json<Vec<skill_daemon_common::DeviceLogEntry>> {
    let out = state
        .device_log
        .lock()
        .map(|g| g.iter().cloned().collect())
        .unwrap_or_default();
    Json(out)
}

pub(crate) async fn set_scanner_config(
    State(state): State<AppState>,
    Json(config): Json<skill_settings::ScannerConfig>,
) -> Json<serde_json::Value> {
    let mut settings = load_user_settings(&state);
    settings.scanner = config;
    save_user_settings(&state, &settings);
    Json(serde_json::json!({"ok": true}))
}

pub(crate) async fn list_serial_ports() -> Json<Vec<String>> {
    let ports = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        tokio::task::spawn_blocking(|| {
            serialport::available_ports()
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.port_name)
                .collect::<Vec<String>>()
        }),
    )
    .await
    .ok()
    .and_then(std::result::Result::ok)
    .unwrap_or_default();

    Json(ports)
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
    async fn filter_config_roundtrip() {
        let (_td, state) = mk_state();
        let cfg = get_filter_config(State(state.clone())).await.0;
        // Default filter config should have some sample_rate
        assert!(cfg.sample_rate >= 0.0);
    }

    #[tokio::test]
    async fn storage_format_roundtrip() {
        let (_td, state) = mk_state();
        let res = get_storage_format(State(state.clone())).await.0;
        assert!(res.get("value").is_some());
    }

    #[tokio::test]
    async fn set_notch_preset_applies() {
        let (_td, state) = mk_state();
        let req = NotchPresetRequest {
            value: Some(PowerlineFreq::Hz60),
        };
        let res = set_notch_preset(State(state.clone()), Json(req)).await.0;
        assert_eq!(res["ok"], true);
        let cfg = get_filter_config(State(state.clone())).await.0;
        assert_eq!(cfg.notch, Some(PowerlineFreq::Hz60));
    }

    #[tokio::test]
    async fn device_log_returns_vec() {
        let (_td, state) = mk_state();
        let res = get_device_log(State(state.clone())).await.0;
        // Default device log should be empty
        assert!(res.is_empty());
    }

    #[tokio::test]
    async fn openbci_config_roundtrip() {
        let (_td, state) = mk_state();
        let res = get_openbci_config(State(state.clone())).await.0;
        // Should return a valid config (default OpenBciConfig)
        let json_val = serde_json::to_value(&res).unwrap();
        assert!(json_val.get("board").is_some() || json_val.get("serial_port").is_some() || !json_val.is_null());
    }

    #[tokio::test]
    async fn scanner_config_roundtrip() {
        let (_td, state) = mk_state();
        let res = get_scanner_config(State(state.clone())).await.0;
        let json_val = serde_json::to_value(&res).unwrap();
        assert!(!json_val.is_null());
    }

    #[tokio::test]
    async fn storage_format_set_and_get() {
        let (_td, state) = mk_state();
        // Set format to "parquet"
        let req = super::super::settings::StringValueRequest {
            value: "parquet".into(),
        };
        let _ = set_storage_format(State(state.clone()), Json(req)).await;
        let res = get_storage_format(State(state.clone())).await.0;
        assert_eq!(res["value"], "parquet");
    }
}
