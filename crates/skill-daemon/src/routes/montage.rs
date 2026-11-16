// SPDX-License-Identifier: GPL-3.0-only
//! Montage routes: the library and defaults of the settings, and the montage
//! of a single recording — given by default when it was made, or by hand
//! afterwards. See `skill_data::montage` for what a montage is and the forms
//! it can be written in.
//!
//! | | |
//! |---|---|
//! | `GET  /settings/montages` | the library and the default rules |
//! | `POST /settings/montages` | replace both (`{library, defaults}`) |
//! | `POST /settings/montages/save` | add or replace one montage, optionally as a default: `{name, montage, defaultFor?}` |
//! | `POST /settings/montages/delete` | remove one, and the defaults that name it: `{name}` |
//! | `POST /settings/montages/preview` | lay a montage over channels without saving anything: `{montage, channelNames \| csvPath}` |
//! | `GET  /history/sessions/montage?csvPath=` | a recording's channels as they are to be read |
//! | `POST /history/sessions/montage` | give a recording a montage, or take it away: `{csvPath, montage \| null}` |
//!
//! Every reply that involves a montage carries `warnings`: what did not fit
//! (a label for a channel the recording does not have, a label used twice).
//! Warnings stop nothing; `ok: false` with an `error` does.

use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use skill_data::montage::{self, Montage, MontageOrigin, MontageRule, MontageSettings, MontageSpec};

use crate::routes::settings_io::{load_user_settings, modify_settings_blocking};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings/montages", get(get_montages).post(set_montages))
        .route("/settings/montages/save", post(save_montage))
        .route("/settings/montages/delete", post(delete_montage))
        .route("/settings/montages/preview", post(preview_montage))
        .route(
            "/history/sessions/montage",
            get(get_session_montage).post(set_session_montage),
        )
}

fn fail(error: impl std::fmt::Display) -> Json<Value> {
    Json(json!({ "ok": false, "error": error.to_string() }))
}

fn settings_reply(settings: &MontageSettings) -> Json<Value> {
    Json(json!({
        "ok": true,
        "library": settings.library,
        "defaults": settings.defaults,
        "warnings": settings.problems(),
    }))
}

async fn get_montages(State(state): State<AppState>) -> Json<Value> {
    settings_reply(&load_user_settings(&state).montages)
}

async fn set_montages(State(state): State<AppState>, Json(new): Json<MontageSettings>) -> Json<Value> {
    let saved = modify_settings_blocking(&state, move |s| {
        s.montages = new;
        s.montages.clone()
    })
    .await;
    settings_reply(&saved)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveMontage {
    /// Its name in the library; a montage of that name is replaced.
    name: String,
    /// In any of the forms a montage can be written in.
    montage: MontageSpec,
    /// Make it the default for sessions like this (`{}`: for every session).
    /// Absent: its defaults stay as they are.
    #[serde(alias = "default_for")]
    default_for: Option<DefaultFor>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DefaultFor {
    #[serde(alias = "device_kind")]
    device_kind: Option<String>,
    #[serde(alias = "device_name")]
    device_name: Option<String>,
    #[serde(alias = "channel_count")]
    channel_count: Option<usize>,
}

async fn save_montage(State(state): State<AppState>, Json(req): Json<SaveMontage>) -> Json<Value> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return fail("the montage needs a name");
    }
    let library = load_user_settings(&state).montages.library;
    let mut new = match Montage::from_spec(req.montage, &library) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    new.name.clone_from(&name);
    let saved = modify_settings_blocking(&state, move |s| {
        let m = &mut s.montages;
        match m.library.iter_mut().find(|o| o.name.eq_ignore_ascii_case(&name)) {
            Some(old) => *old = new,
            None => m.library.push(new),
        }
        if let Some(d) = req.default_for {
            let rule = MontageRule {
                montage: name.clone(),
                device_kind: d.device_kind.filter(|v| !v.trim().is_empty()),
                device_name: d.device_name.filter(|v| !v.trim().is_empty()),
                channel_count: d.channel_count,
            };
            // The same conditions with another montage: this one takes its place. A new
            // rule goes first, so that the newest choice is the one that decides.
            m.defaults.retain(|r| {
                (&r.device_kind, &r.device_name, r.channel_count)
                    != (&rule.device_kind, &rule.device_name, rule.channel_count)
            });
            m.defaults.insert(0, rule);
        }
        m.clone()
    })
    .await;
    settings_reply(&saved)
}

#[derive(Debug, Deserialize)]
struct NamedMontage {
    name: String,
}

async fn delete_montage(State(state): State<AppState>, Json(req): Json<NamedMontage>) -> Json<Value> {
    let saved = modify_settings_blocking(&state, move |s| {
        let m = &mut s.montages;
        m.library.retain(|o| !o.name.eq_ignore_ascii_case(req.name.trim()));
        m.defaults.retain(|r| !r.montage.eq_ignore_ascii_case(req.name.trim()));
        m.clone()
    })
    .await;
    settings_reply(&saved)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewMontage {
    montage: MontageSpec,
    /// The channels to lay it over ...
    #[serde(alias = "channel_names")]
    channel_names: Option<Vec<String>>,
    /// ... or the recording whose channels they are.
    #[serde(alias = "csv_path")]
    csv_path: Option<String>,
}

async fn preview_montage(State(state): State<AppState>, Json(req): Json<PreviewMontage>) -> Json<Value> {
    let library = load_user_settings(&state).montages.library;
    let m = match Montage::from_spec(req.montage, &library) {
        Ok(m) => m,
        Err(e) => return fail(e),
    };
    let recorded = match (req.channel_names, req.csv_path) {
        (Some(names), _) => names,
        (None, Some(path)) => match recording_path(&state, &path) {
            Ok(rec) => match montage::read_session_montage(&rec) {
                Some(file) => file.recorded_channel_names,
                None => montage::effective_channels(&rec)
                    .map(|(r, _)| r.channels.into_iter().map(|c| c.recorded).collect())
                    .unwrap_or_default(),
            },
            Err(e) => return fail(e),
        },
        // nothing to lay it over: the montage's own channels, in order
        (None, None) => (1..=m.channels.len()).map(|k| format!("CH{k}")).collect(),
    };
    let resolved = m.resolve(&recorded);
    Json(json!({ "ok": true, "montage": m, "resolved": resolved, "warnings": resolved.warnings }))
}

/// A recording's path, checked to lie inside the data directory.
fn recording_path(state: &AppState, csv_path: &str) -> Result<std::path::PathBuf, String> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let path = std::path::PathBuf::from(csv_path);
    // The recording may be Parquet only: the folder is what must exist.
    let parent = path
        .parent()
        .and_then(|p| p.canonicalize().ok())
        .ok_or("no such recording")?;
    let skill_dir = skill_dir.canonicalize().unwrap_or(skill_dir);
    if !parent.starts_with(&skill_dir) {
        return Err("the recording is not in the data directory".into());
    }
    let path = parent.join(path.file_name().ok_or("no such recording")?);
    if !path.with_extension("json").exists() {
        return Err("no such recording".into());
    }
    Ok(path)
}

fn session_reply(rec: &std::path::Path) -> Json<Value> {
    match montage::effective_channels(rec) {
        Some((resolved, origin)) => Json(json!({
            "ok": true,
            // `default`: chosen by a rule when the session started; `session`: given by
            // hand; null: the recording has no montage, these are its recorded names
            "origin": origin,
            "montage": montage::read_session_montage(rec).map(|f| f.montage),
            "warnings": resolved.warnings,
            "resolved": resolved,
        })),
        None => fail("the recording's channels are not known"),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionQuery {
    #[serde(alias = "csv_path")]
    csv_path: String,
}

async fn get_session_montage(State(state): State<AppState>, Query(q): Query<SessionQuery>) -> Json<Value> {
    match recording_path(&state, &q.csv_path) {
        Ok(rec) => session_reply(&rec),
        Err(e) => fail(e),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetSessionMontage {
    #[serde(alias = "csv_path")]
    csv_path: String,
    /// In any of the forms a montage can be written in; `null` takes the
    /// recording's montage away.
    montage: Option<MontageSpec>,
}

async fn set_session_montage(State(state): State<AppState>, Json(req): Json<SetSessionMontage>) -> Json<Value> {
    let rec = match recording_path(&state, &req.csv_path) {
        Ok(rec) => rec,
        Err(e) => return fail(e),
    };
    let done = match req.montage {
        None => montage::clear_session_montage(&rec).map(|_| ()),
        Some(spec) => {
            let library = load_user_settings(&state).montages.library;
            match Montage::from_spec(spec, &library) {
                Ok(m) => montage::set_session_montage(&rec, m, MontageOrigin::Session, None).map(|_| ()),
                Err(e) => return fail(e),
            }
        }
    };
    match done {
        Ok(()) => session_reply(&rec),
        Err(e) => fail(e),
    }
}
