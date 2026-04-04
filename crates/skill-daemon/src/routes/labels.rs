// SPDX-License-Identifier: GPL-3.0-only
//! Daemon label routes — create/read/update/delete/search.

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use skill_daemon_common::ApiError;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelEntry {
    pub id: i64,
    pub text: String,
    pub context: Option<String>,
    pub created_at: f64,
}

#[derive(Debug, Deserialize)]
pub struct CreateLabelRequest {
    pub text: String,
    pub context: Option<String>,
    #[serde(rename = "labelStartUtc")]
    pub label_start_utc: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateLabelRequest {
    pub text: String,
    pub context: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/labels", get(list_labels).post(create_label))
        .route("/labels/{id}", put(update_label).delete(delete_label))
}

async fn list_labels(State(state): State<AppState>) -> Json<Vec<LabelEntry>> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let db_path = skill_dir.join("labels.db");
    if !db_path.exists() {
        return Json(Vec::new());
    }
    let Ok(conn) = rusqlite::Connection::open_with_flags(&db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Json(Vec::new());
    };
    let Ok(mut stmt) =
        conn.prepare("SELECT id, text, context, created_at FROM labels ORDER BY created_at DESC LIMIT 1000")
    else {
        return Json(Vec::new());
    };
    let rows = stmt
        .query_map([], |row| {
            Ok(LabelEntry {
                id: row.get(0)?,
                text: row.get(1)?,
                context: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map(|rows| rows.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default();
    Json(rows)
}

async fn create_label(
    State(state): State<AppState>,
    Json(req): Json<CreateLabelRequest>,
) -> Result<Json<LabelEntry>, (StatusCode, Json<ApiError>)> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let db_path = skill_dir.join("labels.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS labels (id INTEGER PRIMARY KEY AUTOINCREMENT, \
         text TEXT NOT NULL, context TEXT, created_at REAL NOT NULL);",
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;

    let now = req.label_start_utc.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    });
    conn.execute(
        "INSERT INTO labels (text, context, created_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![req.text, req.context, now],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;
    let id = conn.last_insert_rowid();
    Ok(Json(LabelEntry {
        id,
        text: req.text,
        context: req.context,
        created_at: now,
    }))
}

async fn update_label(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(req): Json<UpdateLabelRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let db_path = skill_dir.join("labels.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;
    conn.execute(
        "UPDATE labels SET text = ?1, context = ?2 WHERE id = ?3",
        rusqlite::params![req.text, req.context, id],
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn delete_label(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let db_path = skill_dir.join("labels.db");
    let conn = rusqlite::Connection::open(&db_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                code: "db_error",
                message: e.to_string(),
            }),
        )
    })?;
    conn.execute("DELETE FROM labels WHERE id = ?1", rusqlite::params![id])
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    code: "db_error",
                    message: e.to_string(),
                }),
            )
        })?;
    Ok(Json(serde_json::json!({ "ok": true })))
}
