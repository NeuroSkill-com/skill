use axum::{
    extract::{ConnectInfo, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use skill_daemon_common::ApiError;
use std::net::SocketAddr;

use crate::state::AppState;
use crate::util::{record_request, token_path};

pub(crate) async fn auth_middleware(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    // CORS preflight requests never carry credentials — let them through so
    // the CorsLayer (applied as an outer layer) can respond with the proper
    // `Access-Control-Allow-*` headers.
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }

    let peer = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let command = request.uri().path().to_string();

    match auth_decision(&headers, &request, &state) {
        AuthDecision::Allowed => {
            record_request(&state, peer, command, true);
            next.run(request).await
        }
        AuthDecision::MissingOrInvalid => {
            record_request(&state, peer, command, false);
            let body = Json(ApiError {
                code: "unauthorized",
                message: "missing or invalid bearer token".to_string(),
            });
            (StatusCode::UNAUTHORIZED, body).into_response()
        }
        AuthDecision::Forbidden => {
            record_request(&state, peer, command, false);
            let body = Json(ApiError {
                code: "forbidden",
                message: "token does not have permission for this endpoint".to_string(),
            });
            (StatusCode::FORBIDDEN, body).into_response()
        }
    }
}

pub(crate) fn extract_bearer_token(headers: &HeaderMap, request: &axum::extract::Request) -> Option<String> {
    // 1. Authorization: Bearer <token> header
    if let Some(value) = headers.get(header::AUTHORIZATION) {
        if let Ok(value) = value.to_str() {
            if let Some(token) = value.strip_prefix("Bearer ") {
                return Some(token.to_string());
            }
        }
    }

    // 2. ?token=<token> query parameter (for WebSocket — browsers can't set headers)
    if let Some(query) = request.uri().query() {
        for pair in query.split('&') {
            if let Some(val) = pair.strip_prefix("token=") {
                let decoded = urlencoding::decode(val).unwrap_or_default();
                return Some(decoded.into_owned());
            }
        }
    }

    None
}

pub(crate) enum AuthDecision {
    Allowed,
    MissingOrInvalid,
    Forbidden,
}

pub(crate) fn auth_decision(headers: &HeaderMap, request: &axum::extract::Request, state: &AppState) -> AuthDecision {
    // ── Iroh peer bypass ──────────────────────────────────────────────────
    // Requests arriving through the iroh tunnel originate from a local TCP
    // connection whose source port is registered in iroh_peer_map.
    // The iroh tunnel already provides end-to-end encrypted, cryptographically
    // verified peer identity, so we trust it in place of a bearer token.
    // Registered peers get full access; unregistered peers may only reach the
    // registration endpoint so they can complete the TOTP handshake.
    let src_port = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.port())
        .unwrap_or(0);
    if src_port != 0 {
        let maybe_peer = state.iroh_peer_map.lock().ok().and_then(|m| m.get(&src_port).cloned());
        if let Some(peer_id) = maybe_peer {
            let is_registered = state
                .iroh_auth
                .lock()
                .map(|a| a.is_endpoint_allowed(&peer_id))
                .unwrap_or(false);
            if is_registered {
                // Enforce scope-based ACL for iroh peers.
                let scope = state.iroh_auth.lock().ok().and_then(|a| a.scope_for_endpoint(&peer_id));
                let acl = match scope.as_deref() {
                    Some("admin") => crate::auth::TokenAcl::Admin,
                    Some("data") => crate::auth::TokenAcl::Data,
                    Some("stream") => crate::auth::TokenAcl::Stream,
                    _ => crate::auth::TokenAcl::ReadOnly,
                };
                let method = request.method().as_str();
                let path = request.uri().path();
                if acl.allows(method, path) {
                    return AuthDecision::Allowed;
                }
                return AuthDecision::Forbidden;
            }
            // Unregistered peer — only the registration endpoint is open.
            let path = request.uri().path();
            let stripped = path.strip_prefix("/v1").unwrap_or(path);
            if stripped == "/iroh/clients/register" {
                return AuthDecision::Allowed;
            }
            return AuthDecision::MissingOrInvalid;
        }
    }

    // ── Bearer token auth ─────────────────────────────────────────────────
    let Some(token) = extract_bearer_token(headers, request) else {
        return AuthDecision::MissingOrInvalid;
    };

    // Check in-memory default token first (fast path)
    if let Ok(current) = state.auth_token.lock() {
        if token == *current {
            return AuthDecision::Allowed;
        }
    }

    // Check on-disk default token (handles refresh without restart)
    if let Ok(path) = token_path() {
        if let Ok(file_token) = std::fs::read_to_string(path) {
            if token == file_token.trim() {
                return AuthDecision::Allowed;
            }
        }
    }

    // Check multi-token store and distinguish invalid token vs ACL denied.
    let method = request.method().as_str();
    let path = request.uri().path();
    if let Ok(mut store) = state.token_store.lock() {
        if store.authorize(&token, method, path) {
            return AuthDecision::Allowed;
        }
        if store.validate(&token).is_some() {
            return AuthDecision::Forbidden;
        }
        return AuthDecision::MissingOrInvalid;
    }

    AuthDecision::MissingOrInvalid
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::middleware;
    use axum::routing::get;
    use axum::Router;
    use tempfile::TempDir;
    use tower::ServiceExt;

    #[test]
    fn auth_decision_missing_invalid_and_query_bearer() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());
        let headers = HeaderMap::new();

        let req_missing = Request::builder().uri("/v1/status").body(Body::empty()).unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_missing, &state),
            AuthDecision::MissingOrInvalid
        ));

        let req_query = Request::builder()
            .uri("/v1/status?token=default-token")
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_query, &state),
            AuthDecision::Allowed
        ));

        let mut bad_headers = HeaderMap::new();
        bad_headers.insert(header::AUTHORIZATION, "Bearer totally-wrong".parse().unwrap());
        let req_bad = Request::builder().uri("/v1/status").body(Body::empty()).unwrap();
        assert!(matches!(
            auth_decision(&bad_headers, &req_bad, &state),
            AuthDecision::MissingOrInvalid
        ));
    }

    #[test]
    fn auth_decision_forbidden_when_acl_denies_endpoint() {
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());

        let stream_secret = {
            let mut store = state.token_store.lock().unwrap();
            let tok = store
                .create(
                    "stream".to_string(),
                    crate::auth::TokenAcl::Stream,
                    crate::auth::TokenExpiry::Never,
                )
                .expect("create stream token");
            tok.token
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {stream_secret}").parse().unwrap(),
        );

        // Stream ACL allows read status.
        let req_get = Request::builder()
            .method("GET")
            .uri("/v1/status")
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_get, &state),
            AuthDecision::Allowed
        ));

        // But forbids control mutation.
        let req_post = Request::builder()
            .method("POST")
            .uri("/v1/control/start-session")
            .body(Body::empty())
            .unwrap();
        assert!(matches!(
            auth_decision(&headers, &req_post, &state),
            AuthDecision::Forbidden
        ));
    }

    #[test]
    fn auth_decision_iroh_registered_peer_allowed() {
        use std::net::SocketAddr;
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());

        // Register a client in iroh_auth
        let peer_id = "aabbccdd1122334455667788aabbccdd1122334455667788aabbccdd11223344";
        {
            let mut auth = state.iroh_auth.lock().unwrap();
            let (_, _, _) = auth.create_totp("test").unwrap();
            let raw: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(td.path().join("iroh_auth.json")).unwrap()).unwrap();
            let entry = skill_iroh::IrohTotpEntry {
                id: raw["totp"][0]["id"].as_str().unwrap().to_string(),
                name: "test".into(),
                secret_b32: raw["totp"][0]["secret_b32"].as_str().unwrap().to_string(),
                created_at: 0,
                revoked_at: None,
                last_used_at: None,
            };
            let otp = skill_iroh::totp_from_entry(&entry).unwrap().generate_current().unwrap();
            let totp_id = raw["totp"][0]["id"].as_str().unwrap().to_string();
            auth.register_client(peer_id, &otp, Some(&totp_id), Some("phone"), None)
                .unwrap();
        }

        // Simulate iroh peer: register port 54321 → peer_id in peer_map
        state.iroh_peer_map.lock().unwrap().insert(54321, peer_id.to_string());

        // Build a request that looks like it comes from that local port
        let addr: SocketAddr = "127.0.0.1:54321".parse().unwrap();
        let req = Request::builder()
            .uri("/v1/status")
            .extension(ConnectInfo(addr))
            .body(Body::empty())
            .unwrap();
        assert!(
            matches!(auth_decision(&HeaderMap::new(), &req, &state), AuthDecision::Allowed),
            "registered iroh peer should be allowed without bearer token"
        );
    }

    #[test]
    fn auth_decision_iroh_unregistered_peer_registration_only() {
        use std::net::SocketAddr;
        let td = TempDir::new().unwrap();
        let state = AppState::new("default-token".to_string(), td.path().to_path_buf());

        // Register port for an unknown/unregistered peer
        state
            .iroh_peer_map
            .lock()
            .unwrap()
            .insert(54322, "unregistered-peer-id".to_string());

        let addr: SocketAddr = "127.0.0.1:54322".parse().unwrap();

        // Registration endpoint should be allowed
        let req_reg = Request::builder()
            .method("POST")
            .uri("/v1/iroh/clients/register")
            .extension(ConnectInfo(addr))
            .body(Body::empty())
            .unwrap();
        assert!(
            matches!(
                auth_decision(&HeaderMap::new(), &req_reg, &state),
                AuthDecision::Allowed
            ),
            "unregistered iroh peer should reach registration endpoint"
        );

        // Any other endpoint should be denied
        let req_status = Request::builder()
            .uri("/v1/status")
            .extension(ConnectInfo(addr))
            .body(Body::empty())
            .unwrap();
        assert!(
            matches!(
                auth_decision(&HeaderMap::new(), &req_status, &state),
                AuthDecision::MissingOrInvalid
            ),
            "unregistered iroh peer should be denied non-registration endpoints"
        );
    }

    #[test]
    fn extract_bearer_token_header_and_query() {
        let req = Request::builder()
            .uri("/v1/events?token=query-token")
            .body(Body::empty())
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer header-token".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req).as_deref(), Some("header-token"));

        let req = Request::builder()
            .uri("/v1/events?token=query-token")
            .body(Body::empty())
            .unwrap();
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers, &req).as_deref(), Some("query-token"));

        let req = Request::builder()
            .uri("/v1/events?token=abc%2Bdef%3D")
            .body(Body::empty())
            .unwrap();
        let headers = HeaderMap::new();
        assert_eq!(extract_bearer_token(&headers, &req).as_deref(), Some("abc+def="));
    }

    #[test]
    fn extract_bearer_token_rejects_malformed_auth_header() {
        let req = Request::builder().uri("/v1/version").body(Body::empty()).unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Basic abc123".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req), None);

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req), None);

        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "bearer token".parse().unwrap());
        assert_eq!(extract_bearer_token(&headers, &req), None);
    }

    #[tokio::test]
    async fn auth_middleware_allows_options_without_token() {
        async fn ok() -> &'static str {
            "ok"
        }

        let td = TempDir::new().unwrap();
        let state = AppState::new("token".to_string(), td.path().to_path_buf());
        let app = Router::new()
            .route("/v1/echo", get(ok).post(ok))
            .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
            .with_state(state);

        let res = app
            .oneshot(
                Request::builder()
                    .method(axum::http::Method::OPTIONS)
                    .uri("/v1/echo")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
    }
}
