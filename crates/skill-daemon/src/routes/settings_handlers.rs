// SPDX-License-Identifier: GPL-3.0-only
//! Declarative macros for repetitive settings GET/SET axum handlers.

/// GET handler returning `{"value": …}` for a nested settings path.
#[macro_export]
macro_rules! settings_nested_get_value {
    ($get:ident => $($path:ident).+) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            let settings = $crate::routes::settings_io::load_user_settings(&state);
            axum::Json(serde_json::json!({ "value": settings.$($path).+ }))
        }
    };
}

/// GET handler returning `{"value": …}` for a settings field.
#[macro_export]
macro_rules! settings_get_value {
    ($get:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": $crate::routes::settings_io::load_user_settings(&state).$field
            }))
        }
    };
}

/// Bool field; setter returns `{"value": …}` (activity-tracking API shape).
#[macro_export]
macro_rules! settings_bool_set_value {
    ($get:ident, $set:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": $crate::routes::settings_io::load_user_settings(&state).$field
            }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::BoolValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$field = value;
            })
            .await;
            axum::Json(serde_json::json!({ "value": value }))
        }
    };
}

/// Bool in settings plus a matching `AppState` atomic; getter reads the atomic.
#[macro_export]
macro_rules! settings_bool_atomic {
    ($get:ident, $set:ident, field: $field:ident, atomic: $atomic:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": state.$atomic.load(std::sync::atomic::Ordering::Relaxed)
            }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::BoolValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$field = value;
            })
            .await;
            state.$atomic.store(value, std::sync::atomic::Ordering::Relaxed);
            axum::Json(serde_json::json!({ "value": value }))
        }
    };
}

/// String field, JSON `{"value": …}` / setter `{"ok": true}`.
#[macro_export]
macro_rules! settings_string {
    ($get:ident, $set:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": $crate::routes::settings_io::load_user_settings(&state).$field
            }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::StringValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$field = value;
            })
            .await;
            axum::Json(serde_json::json!({ "ok": true }))
        }
    };
}

/// Bool field stored in `UserSettings`, JSON `{"value": …}` / `{"ok": true, "value": …}`.
#[macro_export]
macro_rules! settings_bool {
    ($get:ident, $set:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": $crate::routes::settings_io::load_user_settings(&state).$field
            }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::BoolValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$field = value;
            })
            .await;
            axum::Json(serde_json::json!({ "ok": true, "value": value }))
        }
    };
}

/// GET handler returning a config struct field.
#[macro_export]
macro_rules! settings_struct_get {
    ($ty:ty, $get:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<$ty> {
            axum::Json($crate::routes::settings_io::load_user_settings(&state).$field)
        }
    };
}

/// Whole config struct as request/response body.
#[macro_export]
macro_rules! settings_struct {
    ($ty:ty, $get:ident, $set:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<$ty> {
            axum::Json($crate::routes::settings_io::load_user_settings(&state).$field)
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(config): axum::Json<$ty>,
        ) -> axum::Json<serde_json::Value> {
            $crate::routes::settings_io::patch_settings_ok(&state, move |s| {
                s.$field = config;
            })
            .await
        }
    };
}

/// Bool field at a nested path (`settings.llm.tools.foo`).
#[macro_export]
macro_rules! settings_nested_bool {
    ($get:ident, $set:ident => $($path:ident).+) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            let settings = $crate::routes::settings_io::load_user_settings(&state);
            axum::Json(serde_json::json!({ "value": settings.$($path).+ }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::BoolValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$($path).+ = value;
            })
            .await;
            axum::Json(serde_json::json!({ "ok": true, "value": value }))
        }
    };
}

/// `u64` scalar in settings (device routes).
#[macro_export]
macro_rules! settings_u64 {
    ($get:ident, $set:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": $crate::routes::settings_io::load_user_settings(&state).$field
            }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::U64ValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$field = value;
            })
            .await;
            axum::Json(serde_json::json!({ "ok": true, "value": value }))
        }
    };
}

/// `u64` field at a nested path.
#[macro_export]
macro_rules! settings_nested_u64 {
    ($get:ident, $set:ident => $($path:ident).+) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            let settings = $crate::routes::settings_io::load_user_settings(&state);
            axum::Json(serde_json::json!({ "value": settings.$($path).+ }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::U64ValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$($path).+ = value;
            })
            .await;
            axum::Json(serde_json::json!({ "ok": true, "value": value }))
        }
    };
}

/// `u64` scalar in settings (UI routes).
#[macro_export]
macro_rules! settings_u64_ui {
    ($get:ident, $set:ident => $field:ident) => {
        pub(crate) async fn $get(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
        ) -> axum::Json<serde_json::Value> {
            axum::Json(serde_json::json!({
                "value": $crate::routes::settings_io::load_user_settings(&state).$field
            }))
        }

        pub(crate) async fn $set(
            axum::extract::State(state): axum::extract::State<$crate::state::AppState>,
            axum::Json(req): axum::Json<$crate::routes::settings::U64ValueRequest>,
        ) -> axum::Json<serde_json::Value> {
            let value = req.value;
            $crate::routes::settings_io::modify_settings_blocking(&state, move |s| {
                s.$field = value;
            })
            .await;
            axum::Json(serde_json::json!({ "ok": true, "value": value }))
        }
    };
}
