// SPDX-License-Identifier: GPL-3.0-only
//! Universal JSON command dispatcher.
//!
//! Maps CLI command names (e.g. `{ "command": "status" }`) to daemon REST
//! handler logic.  Used by both the WS command handler and the `POST /v1/cmd`
//! HTTP tunnel.

mod data_cmds;
mod llm_cmds;
mod system_cmds;

use serde_json::{json, Value};

use crate::state::AppState;

#[cfg(feature = "llm")]
pub use llm_cmds::dispatch_llm_chat_streaming;

/// Dispatch a JSON command envelope to the appropriate handler.
///
/// The envelope must have a `"command"` string field.  Additional fields are
/// forwarded as parameters.  Returns a JSON response that always contains
/// `"command"` and `"ok"` fields matching the CLI protocol.
pub async fn dispatch(state: AppState, msg: Value) -> Value {
    let cmd = msg.get("command").and_then(Value::as_str).unwrap_or("").to_string();

    if cmd.is_empty() {
        return json!({ "command": "", "ok": false, "error": "missing command field" });
    }

    let result = if let Some(result) = dispatch_family_commands(&state, &msg, &cmd).await {
        result
    } else {
        match cmd.as_str() {
            // ── Core status / sessions ──────────────────────────────────────
            "status" => data_cmds::cmd_status(&state).await,
            "sessions" => data_cmds::cmd_sessions(&state).await,
            "session_metrics" => data_cmds::cmd_session_metrics(&state, &msg).await,

            // ── Device / session control ─────────────────────────────────────
            "devices" => data_cmds::cmd_devices(&state).await,
            "start_session" => data_cmds::cmd_start_session(&state, &msg).await,
            "cancel_session" => data_cmds::cmd_cancel_session(&state).await,

            // ── Labels ──────────────────────────────────────────────────────
            "label" => data_cmds::cmd_label(&state, &msg).await,
            "search_labels" => data_cmds::cmd_search_labels(&state, &msg).await,

            // ── Screenshots ─────────────────────────────────────────────────
            "search_screenshots" => data_cmds::cmd_search_screenshots(&state, &msg).await,
            "screenshots_around" => data_cmds::cmd_screenshots_around(&state, &msg).await,
            "screenshots_for_eeg" => data_cmds::cmd_screenshots_for_eeg(&state, &msg).await,
            "eeg_for_screenshots" => data_cmds::cmd_eeg_for_screenshots(&state, &msg).await,

            // ── EEG search / compare / sleep / umap ─────────────────────────
            "search" => data_cmds::cmd_search(&state, &msg).await,
            "compare" => data_cmds::cmd_compare(&state, &msg).await,
            "sleep" => data_cmds::cmd_sleep(&state, &msg).await,
            "interactive_search" => data_cmds::cmd_interactive_search(&state, &msg).await,
            "umap" => data_cmds::cmd_umap(&state, &msg).await,
            "umap_poll" => data_cmds::cmd_umap_poll(&state, &msg).await,

            // ── Calibrations ────────────────────────────────────────────────
            "list_calibrations" => data_cmds::cmd_list_calibrations(&state).await,
            "get_calibration" => data_cmds::cmd_get_calibration(&state, &msg).await,
            "create_calibration" => data_cmds::cmd_create_calibration(&state, &msg).await,
            "update_calibration" => data_cmds::cmd_update_calibration(&state, &msg).await,
            "delete_calibration" => data_cmds::cmd_delete_calibration(&state, &msg).await,
            "run_calibration" => cmd_stub(&cmd, "calibration requires GUI").await,

            // ── TTS / Notify / Timer ────────────────────────────────────────
            "say" => system_cmds::cmd_say(&state, &msg).await,
            "notify" => system_cmds::cmd_notify(&msg).await,
            "timer" => cmd_stub(&cmd, "timer requires GUI").await,

            // ── Health / Oura / Calendar ────────────────────────────────────
            "health_query" => system_cmds::cmd_health_query(&state, &msg).await,
            "health_summary" => system_cmds::cmd_health_summary(&state, &msg).await,
            "health_metric_types" => system_cmds::cmd_health_metric_types(&state).await,
            "oura_status" => system_cmds::cmd_oura_status(&state).await,
            "oura_sync" => system_cmds::cmd_oura_sync(&state, &msg).await,
            "calendar_status" => system_cmds::cmd_calendar_status().await,
            "calendar_request_permission" => system_cmds::cmd_calendar_permission().await,
            "calendar_events" => system_cmds::cmd_calendar_events(&msg).await,

            // ── Subscribe (WS-only, acknowledge) ────────────────────────────
            "subscribe" => Ok(json!({ "subscribed": true })),

            _ => Err(format!("unknown command: {cmd}")),
        }
    };

    match result {
        Ok(mut v) => {
            if let Some(obj) = v.as_object_mut() {
                obj.insert("command".into(), json!(cmd));
                if !obj.contains_key("ok") {
                    obj.insert("ok".into(), json!(true));
                }
            }
            v
        }
        Err(e) => json!({ "command": cmd, "ok": false, "error": e }),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

pub(super) fn skill_dir(state: &AppState) -> std::path::PathBuf {
    state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default()
}

pub(super) fn str_field(msg: &Value, key: &str) -> Option<String> {
    msg.get(key).and_then(Value::as_str).map(String::from)
}

pub(super) fn u64_field(msg: &Value, key: &str) -> Option<u64> {
    msg.get(key).and_then(Value::as_u64)
}

pub(super) fn f64_field(msg: &Value, key: &str) -> Option<f64> {
    msg.get(key).and_then(Value::as_f64)
}

pub(super) fn i64_field(msg: &Value, key: &str) -> Option<i64> {
    msg.get(key).and_then(Value::as_i64)
}

pub(super) fn bool_field(msg: &Value, key: &str) -> Option<bool> {
    msg.get(key).and_then(Value::as_bool)
}

async fn cmd_stub(cmd: &str, note: &str) -> Result<Value, String> {
    Ok(json!({ "command": cmd, "ok": false, "error": note }))
}

async fn dispatch_family_commands(state: &AppState, msg: &Value, cmd: &str) -> Option<Result<Value, String>> {
    match cmd {
        // Hooks
        "hooks_status" => Some(data_cmds::cmd_hooks_status(state).await),
        "hooks_get" => Some(data_cmds::cmd_hooks_get(state).await),
        "hooks_set" => Some(data_cmds::cmd_hooks_set(state, msg).await),
        "hooks_suggest" => Some(data_cmds::cmd_hooks_suggest(state, msg).await),
        "hooks_log" => Some(data_cmds::cmd_hooks_log(state, msg).await),

        // Sleep schedule
        "sleep_schedule" => Some(system_cmds::cmd_sleep_schedule(state).await),
        "sleep_schedule_set" => Some(system_cmds::cmd_sleep_schedule_set(state, msg).await),

        // DND
        "dnd" => Some(system_cmds::cmd_dnd(state).await),
        "dnd_set" => Some(system_cmds::cmd_dnd_set(msg).await),

        // Iroh
        "iroh_info" => Some(system_cmds::cmd_iroh_info(state).await),
        "iroh_totp_list" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_totp_list(
            &state.iroh_auth,
        ))),
        "iroh_totp_create" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_totp_create(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_totp_qr" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_totp_qr(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_totp_revoke" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_totp_revoke(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_clients_list" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_clients_list(
            &state.iroh_auth,
        ))),
        "iroh_client_register" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_client_register(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_client_revoke" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_client_revoke(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_client_set_scope" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_client_set_scope(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_scope_groups" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_scope_groups(
            &state.iroh_auth,
        ))),
        "iroh_client_permissions" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_client_permissions(
            &state.iroh_auth,
            msg,
        ))),
        "iroh_phone_invite" => Some(system_cmds::cmd_iroh(skill_iroh::commands::iroh_phone_invite(
            &state.iroh_auth,
            &state.iroh_runtime,
            msg,
        ))),

        // LLM
        "llm_status" => Some(llm_cmds::cmd_llm_status(state).await),
        "llm_start" => Some(llm_cmds::cmd_llm_start(state).await),
        "llm_stop" => Some(llm_cmds::cmd_llm_stop(state).await),
        "llm_catalog" => Some(llm_cmds::cmd_llm_catalog(state).await),
        "llm_add_model" => Some(llm_cmds::cmd_llm_add_model(state, msg).await),
        "llm_select_model" => Some(llm_cmds::cmd_llm_select_model(state, msg).await),
        "llm_select_mmproj" => Some(llm_cmds::cmd_llm_select_mmproj(state, msg).await),
        "llm_set_autoload_mmproj" => Some(llm_cmds::cmd_llm_set_autoload_mmproj(state, msg).await),
        "llm_download" => Some(llm_cmds::cmd_llm_download(state, msg).await),
        "llm_pause_download" => Some(llm_cmds::cmd_llm_pause_download(state, msg).await),
        "llm_resume_download" => Some(llm_cmds::cmd_llm_resume_download(state, msg).await),
        "llm_cancel_download" => Some(llm_cmds::cmd_llm_cancel_download(state, msg).await),
        "llm_delete" => Some(llm_cmds::cmd_llm_delete(state, msg).await),
        "llm_downloads" => Some(llm_cmds::cmd_llm_downloads(state).await),
        "llm_refresh_catalog" => Some(llm_cmds::cmd_llm_refresh(state).await),
        "llm_hardware_fit" => Some(llm_cmds::cmd_llm_hardware_fit(state).await),
        "llm_logs" => Some(llm_cmds::cmd_llm_logs(state).await),
        "llm_chat" => Some(llm_cmds::cmd_llm_chat(state, msg).await),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn helpers_extract_typed_fields() {
        let msg = json!({"s":"x","u":7,"f":1.25,"i":-2,"b":true});
        assert_eq!(str_field(&msg, "s").as_deref(), Some("x"));
        assert_eq!(u64_field(&msg, "u"), Some(7));
        assert_eq!(f64_field(&msg, "f"), Some(1.25));
        assert_eq!(i64_field(&msg, "i"), Some(-2));
        assert_eq!(bool_field(&msg, "b"), Some(true));
    }

    #[tokio::test]
    async fn dispatch_missing_command_fails() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let v = dispatch(state, json!({"foo":"bar"})).await;
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"], "missing command field");
    }

    #[tokio::test]
    async fn dispatch_unknown_command_fails() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let v = dispatch(state, json!({"command":"nope"})).await;
        assert_eq!(v["command"], "nope");
        assert_eq!(v["ok"], false);
        assert!(v["error"].as_str().unwrap_or("").contains("unknown command"));
    }

    #[tokio::test]
    async fn dispatch_status_returns_command_and_ok() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());
        let v = dispatch(state, json!({"command":"status"})).await;
        assert_eq!(v["command"], "status");
        assert_eq!(v["ok"], true);
        assert!(v.get("device").is_some());
    }

    #[tokio::test]
    async fn dispatch_devices_and_sessions_have_expected_shape() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let v1 = dispatch(state.clone(), json!({"command":"devices"})).await;
        assert_eq!(v1["ok"], true);
        assert!(v1["devices"].is_array());

        let v2 = dispatch(state, json!({"command":"sessions"})).await;
        assert_eq!(v2["ok"], true);
        assert!(v2["sessions"].is_array());
    }

    #[tokio::test]
    async fn dispatch_validation_errors_for_missing_required_fields() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let m1 = dispatch(state.clone(), json!({"command":"session_metrics"})).await;
        assert_eq!(m1["ok"], false);
        assert!(m1["error"].as_str().unwrap_or("").contains("start_utc"));

        let m2 = dispatch(state.clone(), json!({"command":"label"})).await;
        assert_eq!(m2["ok"], false);
        assert!(m2["error"].as_str().unwrap_or("").contains("text"));

        let m3 = dispatch(state.clone(), json!({"command":"search_labels"})).await;
        assert_eq!(m3["ok"], false);
        assert!(m3["error"].as_str().unwrap_or("").contains("query"));

        let m4 = dispatch(state.clone(), json!({"command":"dnd_set"})).await;
        assert_eq!(m4["ok"], false);
        assert!(m4["error"].as_str().unwrap_or("").contains("enabled"));

        let m5 = dispatch(state, json!({"command":"hooks_set"})).await;
        assert_eq!(m5["ok"], false);
        assert!(m5["error"].as_str().unwrap_or("").contains("hooks"));
    }

    #[tokio::test]
    async fn dispatch_hooks_and_iroh_info_paths() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let set = dispatch(
            state.clone(),
            json!({
                "command":"hooks_set",
                "hooks":[{
                    "name":"focus",
                    "enabled":true,
                    "keywords":["focus"],
                    "scenario":"any",
                    "command":"say",
                    "text":"hello",
                    "distance_threshold":0.2,
                    "recent_limit":8
                }]
            }),
        )
        .await;
        assert_eq!(set["ok"], true);
        assert!(set["hooks"].is_array());

        let get = dispatch(state.clone(), json!({"command":"hooks_get"})).await;
        assert_eq!(get["ok"], true);
        assert_eq!(get["hooks"].as_array().map(|a| a.len()).unwrap_or(0), 1);

        let status = dispatch(state.clone(), json!({"command":"hooks_status"})).await;
        assert_eq!(status["ok"], true);
        assert!(status["hooks"].is_array());

        let iroh = dispatch(state, json!({"command":"iroh_info"})).await;
        assert_eq!(iroh["ok"], true);
        // online=false because the tunnel hasn't started in tests
        assert_eq!(iroh["online"], false);
    }

    #[tokio::test]
    async fn dispatch_sleep_schedule_roundtrip_and_stub_command() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let before = dispatch(state.clone(), json!({"command":"sleep_schedule"})).await;
        assert_eq!(before["ok"], true);

        let set = dispatch(
            state.clone(),
            json!({
                "command":"sleep_schedule_set",
                "bedtime":"22:30",
                "wake_time":"06:45"
            }),
        )
        .await;
        assert_eq!(set["ok"], true);
        assert_eq!(set["bedtime"], "22:30");

        let timer = dispatch(state, json!({"command":"timer"})).await;
        assert_eq!(timer["ok"], false);
        assert!(timer["error"].as_str().unwrap_or("").contains("requires GUI"));
    }

    #[tokio::test]
    async fn dispatch_dnd_and_dnd_set_paths() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let dnd = dispatch(state.clone(), json!({"command":"dnd"})).await;
        assert_eq!(dnd["ok"], true);
        assert!(dnd.get("enabled").is_some());

        let off = dispatch(state.clone(), json!({"command":"dnd_set","enabled":false})).await;
        assert_eq!(off["ok"], true);
        assert_eq!(off["enabled"], false);

        let on = dispatch(state, json!({"command":"dnd_set","enabled":true})).await;
        assert_eq!(on["ok"], true);
        assert_eq!(on["enabled"], true);
        assert!(on.get("applied").is_some());
    }

    #[tokio::test]
    async fn dispatch_hooks_suggest_and_log_empty_paths() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let suggest = dispatch(state.clone(), json!({"command":"hooks_suggest","keywords":["focus"]})).await;
        assert_eq!(suggest["ok"], true);
        assert!(suggest.get("suggested").is_some());

        let log = dispatch(state, json!({"command":"hooks_log","limit":10,"offset":0})).await;
        assert_eq!(log["ok"], true);
        assert!(log["rows"].is_array());
        assert_eq!(log["count"], 0);
    }

    #[tokio::test]
    async fn dispatch_calibration_crud_roundtrip() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let created = dispatch(
            state.clone(),
            json!({"command":"create_calibration","name":"alpha","config":{"gain":2}}),
        )
        .await;
        assert_eq!(created["ok"], true);
        let id = created["id"].as_str().unwrap_or("").to_string();
        assert!(!id.is_empty());

        let listed = dispatch(state.clone(), json!({"command":"list_calibrations"})).await;
        assert_eq!(listed["ok"], true);
        assert!(listed["profiles"].as_array().map(|a| !a.is_empty()).unwrap_or(false));

        let got = dispatch(state.clone(), json!({"command":"get_calibration","id":id})).await;
        assert_eq!(got["ok"], true);
        assert!(got["profile"].get("id").is_some());

        let _updated = dispatch(
            state.clone(),
            json!({"command":"update_calibration","id":got["profile"]["id"],"name":"beta","config":{"gain":3}}),
        )
        .await;

        let deleted = dispatch(
            state.clone(),
            json!({"command":"delete_calibration","id":got["profile"]["id"]}),
        )
        .await;
        assert_eq!(deleted["ok"], true);

        let listed2 = dispatch(state, json!({"command":"list_calibrations"})).await;
        assert_eq!(listed2["ok"], true);
        assert_eq!(listed2["profiles"].as_array().map(|a| a.len()).unwrap_or(0), 0);
    }

    #[tokio::test]
    async fn dispatch_family_router_handles_known_groups() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let hooks = dispatch_family_commands(&state, &json!({}), "hooks_status").await;
        assert!(hooks.is_some());

        let dnd = dispatch_family_commands(&state, &json!({"enabled":false}), "dnd_set").await;
        assert!(dnd.is_some());

        let iroh_totp = dispatch_family_commands(&state, &json!({}), "iroh_totp_list").await;
        assert!(iroh_totp.is_some());

        let llm = dispatch_family_commands(&state, &json!({}), "llm_status").await;
        assert!(llm.is_some());

        let unknown = dispatch_family_commands(&state, &json!({}), "not-a-family-command").await;
        assert!(unknown.is_none());
    }

    #[tokio::test]
    async fn dispatch_command_matrix_has_no_unknowns() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("t".into(), td.path().to_path_buf());

        let cases = vec![
            json!({"command":"status"}),
            json!({"command":"devices"}),
            json!({"command":"sessions"}),
            json!({"command":"hooks_status"}),
            json!({"command":"hooks_get"}),
            json!({"command":"hooks_log","limit":1,"offset":0}),
            json!({"command":"sleep_schedule"}),
            json!({"command":"dnd"}),
            json!({"command":"dnd_set","enabled":false}),
            json!({"command":"iroh_info"}),
            json!({"command":"iroh_totp_list"}),
            json!({"command":"llm_status"}),
            json!({"command":"llm_catalog"}),
            json!({"command":"llm_downloads"}),
            json!({"command":"llm_logs"}),
            json!({"command":"subscribe"}),
            json!({"command":"calendar_status"}),
            json!({"command":"health_metric_types"}),
            json!({"command":"timer"}),
        ];

        for msg in cases {
            let out = dispatch(state.clone(), msg).await;
            let err = out["error"].as_str().unwrap_or("");
            assert!(
                !err.contains("unknown command"),
                "dispatcher returned unknown command for payload: {out}"
            );
        }
    }
}
