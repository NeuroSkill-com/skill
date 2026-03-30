#![allow(clippy::unwrap_used, clippy::panic)]
use iroh::endpoint::Connection;
use iroh::Watcher;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{timeout, Duration};
const ALPN: &[u8] = b"skill/http-ws/1";

/// Minimal server handler: reads raw HTTP from an iroh bi-stream, parses the
/// JSON body, calls `IrohAuthStore::register_client`, and writes back an HTTP
/// response. No hyper dependency needed.
async fn handle_one_request(
    send: &mut iroh::endpoint::SendStream,
    recv: &mut iroh::endpoint::RecvStream,
    skill_dir: &std::path::Path,
) -> anyhow::Result<()> {
    let raw = recv.read_to_end(64 * 1024).await?;
    let raw_str = String::from_utf8_lossy(&raw);

    // Split HTTP header from body
    let body_str = raw_str.split("\r\n\r\n").nth(1).unwrap_or("");

    let v: serde_json::Value = serde_json::from_str(body_str).unwrap_or_else(|_| serde_json::json!({}));

    let endpoint_id = v.get("endpoint_id").and_then(serde_json::Value::as_str).unwrap_or("");
    let otp = v.get("otp").and_then(serde_json::Value::as_str).unwrap_or("");
    let totp_id = v.get("totp_id").and_then(serde_json::Value::as_str);
    let name = v.get("name").and_then(serde_json::Value::as_str);
    let scope = v.get("scope").and_then(serde_json::Value::as_str);

    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    let (status, body) = match store.register_client(endpoint_id, otp, totp_id, name, scope) {
        Ok(client) => {
            let _ = store.mark_client_connected(&client.endpoint_id, "iroh-test", None);
            let body = serde_json::json!({"ok": true, "client": client}).to_string();
            ("200 OK", body)
        }
        Err(e) => {
            let body = serde_json::json!({"ok": false, "error": e}).to_string();
            ("400 Bad Request", body)
        }
    };

    let resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    send.write_all(resp.as_bytes()).await?;
    send.finish()?;
    // Wait for the stream to be fully sent before returning
    send.stopped().await?;
    Ok(())
}

/// Accept connections in a loop until `max` requests have been served.
async fn serve_n_requests(
    server_ep: iroh::Endpoint,
    skill_dir: Arc<std::path::PathBuf>,
    max: usize,
) -> anyhow::Result<()> {
    for _ in 0..max {
        let incoming = server_ep.accept().await.ok_or_else(|| anyhow::anyhow!("no incoming"))?;
        let conn = incoming.await?;
        let (mut send, mut recv) = conn.accept_bi().await?;
        handle_one_request(&mut send, &mut recv, &skill_dir).await?;
        // Keep conn alive until handler is done; drop it here.
        drop(conn);
    }
    Ok(())
}

/// Helper: extract the TOTP secret bytes from an otpauth URL
fn secret_from_otpauth(otpauth_url: &str) -> anyhow::Result<Vec<u8>> {
    let parsed = url::Url::parse(otpauth_url)?;
    let secret_b32 = parsed
        .query_pairs()
        .find(|(k, _)| k == "secret")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| anyhow::anyhow!("no secret in otpauth url"))?;
    base32::decode(base32::Alphabet::Rfc4648 { padding: false }, &secret_b32)
        .ok_or_else(|| anyhow::anyhow!("invalid base32"))
}

/// Helper: generate a current TOTP code from secret bytes
fn generate_totp_code(secret: &[u8], account_name: &str) -> anyhow::Result<String> {
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_vec(),
        Some("Skill".to_string()),
        account_name.to_string(),
    )?;
    Ok(totp.generate_current()?)
}

/// Helper: send an HTTP POST over iroh and return the parsed JSON body
async fn send_register_request(conn: &Connection, body_json: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let body = body_json.to_string();
    let req = format!(
        "POST /v1/iroh/clients/register HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(req.as_bytes()).await?;
    send.finish()?;
    let resp = recv.read_to_end(16 * 1024).await?;
    let resp_text = String::from_utf8_lossy(&resp);
    let body_str = resp_text.split("\r\n\r\n").nth(1).unwrap_or("");
    let v: serde_json::Value = serde_json::from_str(body_str)?;
    Ok(v)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn register_success_and_mark_connected() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    let (tview, otpauth_url, _qr) = store.create_totp("phone").map_err(|e| anyhow::anyhow!(e))?;
    let secret = secret_from_otpauth(&otpauth_url)?;

    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let server_dir = Arc::new(skill_dir.to_path_buf());
    let srv = server_ep.clone();
    let sd = server_dir.clone();
    let serve_handle = tokio::spawn(async move { serve_n_requests(srv, sd, 1).await });

    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;
    server_ep.online().await;

    let server_addr = server_ep.watch_addr().get();
    let code = generate_totp_code(&secret, &tview.name)?;

    let conn = client_ep.connect(server_addr, ALPN).await?;
    let v = send_register_request(
        &conn,
        &serde_json::json!({
            "endpoint_id": client_ep.id().to_string(),
            "otp": code,
            "name": "cli",
            "scope": "read"
        }),
    )
    .await?;

    assert_eq!(v.get("ok").and_then(|b| b.as_bool()), Some(true));

    let _ = timeout(Duration::from_secs(5), serve_handle).await??;

    // Verify persisted state
    let s2 = skill_iroh::IrohAuthStore::open(skill_dir);
    let clients = s2.list_clients();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].name, "cli");
    assert_eq!(clients[0].scope, "read");
    assert!(clients[0].last_connected_at.is_some());
    assert_eq!(clients[0].last_remote_addr.as_deref(), Some("iroh-test"));
    Ok(())
}

#[tokio::test]
async fn register_with_full_scope_and_scope_change() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    let (tview, otpauth_url, _qr) = store.create_totp("phone").map_err(|e| anyhow::anyhow!(e))?;
    let secret = secret_from_otpauth(&otpauth_url)?;

    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let server_dir = Arc::new(skill_dir.to_path_buf());
    let srv = server_ep.clone();
    let sd = server_dir.clone();
    let serve_handle = tokio::spawn(async move { serve_n_requests(srv, sd, 1).await });

    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;
    server_ep.online().await;

    let server_addr = server_ep.watch_addr().get();
    let code = generate_totp_code(&secret, &tview.name)?;

    let conn = client_ep.connect(server_addr, ALPN).await?;
    let v = send_register_request(
        &conn,
        &serde_json::json!({
            "endpoint_id": client_ep.id().to_string(),
            "otp": code,
            "name": "full-client",
            "scope": "full"
        }),
    )
    .await?;
    assert_eq!(v.get("ok").and_then(|b| b.as_bool()), Some(true));

    let _ = timeout(Duration::from_secs(5), serve_handle).await??;

    // Verify stored scope
    let s2 = skill_iroh::IrohAuthStore::open(skill_dir);
    let clients = s2.list_clients();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].scope, "full");

    // Change scope via store API
    let id = clients[0].id.clone();
    let mut s3 = skill_iroh::IrohAuthStore::open(skill_dir);
    s3.set_client_scope(&id, "read").map_err(|e| anyhow::anyhow!(e))?;
    let s4 = skill_iroh::IrohAuthStore::open(skill_dir);
    assert_eq!(s4.list_clients()[0].scope, "read");
    Ok(())
}

#[tokio::test]
async fn invalid_otp_rejected() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    store.create_totp("phone").map_err(|e| anyhow::anyhow!(e))?;

    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let server_dir = Arc::new(skill_dir.to_path_buf());
    let srv = server_ep.clone();
    let sd = server_dir.clone();
    let serve_handle = tokio::spawn(async move { serve_n_requests(srv, sd, 1).await });

    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;
    server_ep.online().await;

    let server_addr = server_ep.watch_addr().get();
    let conn = client_ep.connect(server_addr, ALPN).await?;
    let v = send_register_request(
        &conn,
        &serde_json::json!({
            "endpoint_id": client_ep.id().to_string(),
            "otp": "000000"
        }),
    )
    .await?;
    assert_eq!(v.get("ok").and_then(|b| b.as_bool()), Some(false));
    assert!(v
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .contains("invalid otp"));

    let _ = timeout(Duration::from_secs(5), serve_handle).await??;
    Ok(())
}

#[tokio::test]
async fn revoked_totp_rejected() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    let (tview, otpauth_url, _) = store.create_totp("phone").map_err(|e| anyhow::anyhow!(e))?;
    let secret = secret_from_otpauth(&otpauth_url)?;
    store.revoke_totp(&tview.id).map_err(|e| anyhow::anyhow!(e))?;

    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let server_dir = Arc::new(skill_dir.to_path_buf());
    let srv = server_ep.clone();
    let sd = server_dir.clone();
    let serve_handle = tokio::spawn(async move { serve_n_requests(srv, sd, 1).await });

    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;
    server_ep.online().await;

    let server_addr = server_ep.watch_addr().get();
    let code = generate_totp_code(&secret, &tview.name)?;

    let conn = client_ep.connect(server_addr, ALPN).await?;
    let v = send_register_request(
        &conn,
        &serde_json::json!({
            "endpoint_id": client_ep.id().to_string(),
            "otp": code,
            "totp_id": tview.id,
        }),
    )
    .await?;
    assert_eq!(v.get("ok").and_then(|b| b.as_bool()), Some(false));
    assert!(v
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .contains("revoked"));

    let _ = timeout(Duration::from_secs(5), serve_handle).await??;
    Ok(())
}

#[tokio::test]
async fn ambiguous_totp_requires_hint() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    let (t1, otpauth1, _) = store.create_totp("p1").map_err(|e| anyhow::anyhow!(e))?;
    let (_t2, _, _) = store.create_totp("p2").map_err(|e| anyhow::anyhow!(e))?;

    // Force both TOTPs to share the same secret
    let auth_path = skill_dir.join("iroh_auth.json");
    let mut db: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&auth_path)?)?;
    if let Some(arr) = db.get_mut("totp").and_then(|v| v.as_array_mut()) {
        if arr.len() >= 2 {
            let s0 = arr[0]
                .get("secret_b32")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            arr[1]["secret_b32"] = serde_json::Value::String(s0);
            std::fs::write(&auth_path, serde_json::to_string_pretty(&db)?)?;
        }
    }

    let secret = secret_from_otpauth(&otpauth1)?;

    // We need 2 requests: one ambiguous (fail), one with hint (success)
    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let server_dir = Arc::new(skill_dir.to_path_buf());
    let srv = server_ep.clone();
    let sd = server_dir.clone();
    let serve_handle = tokio::spawn(async move { serve_n_requests(srv, sd, 2).await });

    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;
    server_ep.online().await;
    let server_addr = server_ep.watch_addr().get();

    // Request 1: ambiguous (no hint) -> should fail
    let code1 = generate_totp_code(&secret, &t1.name)?;
    let conn1 = client_ep.connect(server_addr.clone(), ALPN).await?;
    let v1 = send_register_request(
        &conn1,
        &serde_json::json!({
            "endpoint_id": client_ep.id().to_string(),
            "otp": code1,
        }),
    )
    .await?;
    assert_eq!(v1.get("ok").and_then(|b| b.as_bool()), Some(false));
    assert!(v1
        .get("error")
        .and_then(|e| e.as_str())
        .unwrap_or("")
        .contains("multiple"));

    // Request 2: with totp_id hint -> should succeed
    let code2 = generate_totp_code(&secret, &t1.name)?;
    let conn2 = client_ep.connect(server_addr, ALPN).await?;
    let v2 = send_register_request(
        &conn2,
        &serde_json::json!({
            "endpoint_id": client_ep.id().to_string(),
            "otp": code2,
            "totp_id": t1.id,
        }),
    )
    .await?;
    assert_eq!(v2.get("ok").and_then(|b| b.as_bool()), Some(true));

    let _ = timeout(Duration::from_secs(5), serve_handle).await??;
    Ok(())
}

/// A server that checks `is_endpoint_allowed` before serving each bi-stream,
/// and closes the QUIC connection with code 1 ("revoked") when the client is
/// no longer authorised.  This mirrors the real tunnel's `handle_connection`.
async fn serve_with_auth_check(
    server_ep: iroh::Endpoint,
    auth: std::sync::Arc<std::sync::Mutex<skill_iroh::IrohAuthStore>>,
    expected_streams: usize,
) -> anyhow::Result<()> {
    let incoming = server_ep.accept().await.ok_or_else(|| anyhow::anyhow!("no incoming"))?;
    let conn = incoming.await?;
    let peer = conn.remote_id().to_string().to_lowercase();

    for _i in 0..expected_streams {
        // Auth check before accepting the next stream (like the real tunnel)
        {
            let store = auth.lock().expect("lock");
            if !store.is_endpoint_allowed(&peer) {
                conn.close(1u32.into(), b"revoked");
                return Ok(());
            }
        }

        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(_) => {
                // Connection was closed (client side or our close propagated)
                return Ok(());
            }
        };

        // Auth check after accept (revocation may have happened while waiting)
        {
            let store = auth.lock().expect("lock");
            if !store.is_endpoint_allowed(&peer) {
                conn.close(1u32.into(), b"revoked");
                return Ok(());
            }
        }

        // Echo the request back as a simple 200
        let _raw = match recv.read_to_end(64 * 1024).await {
            Ok(r) => r,
            Err(_) => return Ok(()),
        };
        let resp = "HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\n{\"ok\": true}\r\n";
        let _ = send.write_all(resp.as_bytes()).await;
        let _ = send.finish();
        let _ = send.stopped().await;
    }
    Ok(())
}

#[tokio::test]
async fn revoked_client_cannot_communicate() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    // ── Setup: create TOTP, register a client ────────────────────────────
    let mut store = skill_iroh::IrohAuthStore::open(skill_dir);
    let (_tview, otpauth_url, _) = store.create_totp("phone").map_err(|e| anyhow::anyhow!(e))?;
    let secret = secret_from_otpauth(&otpauth_url)?;

    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;
    server_ep.online().await;
    let server_addr = server_ep.watch_addr().get();

    // Register the client first (via the register server)
    {
        let srv = server_ep.clone();
        let sd = Arc::new(skill_dir.to_path_buf());
        let serve_handle = tokio::spawn(async move { serve_n_requests(srv, sd, 1).await });
        let code = generate_totp_code(&secret, "phone")?;
        let conn = client_ep.connect(server_addr.clone(), ALPN).await?;
        let v = send_register_request(
            &conn,
            &serde_json::json!({
                "endpoint_id": client_ep.id().to_string(),
                "otp": code,
                "name": "test-client",
                "scope": "read"
            }),
        )
        .await?;
        assert_eq!(v.get("ok").and_then(|b| b.as_bool()), Some(true));
        let _ = timeout(Duration::from_secs(5), serve_handle).await??;
    }

    // Verify client is registered
    let store2 = skill_iroh::IrohAuthStore::open(skill_dir);
    assert!(store2.is_endpoint_allowed(&client_ep.id().to_string()));
    let client_id = store2.list_clients()[0].id.clone();

    // ── Phase 1: client can communicate (pre-revocation) ─────────────────
    // Shared auth store for the auth-checking server
    let auth = std::sync::Arc::new(std::sync::Mutex::new(skill_iroh::IrohAuthStore::open(skill_dir)));

    // Start server that will serve up to 3 streams but checks auth each time
    let srv2 = server_ep.clone();
    let auth2 = auth.clone();
    let serve_handle = tokio::spawn(async move { serve_with_auth_check(srv2, auth2, 3).await });

    // Small delay to let server start accepting
    tokio::time::sleep(Duration::from_millis(50)).await;

    let conn2 = client_ep.connect(server_addr.clone(), ALPN).await?;

    // First request: should succeed
    let (mut send1, mut recv1) = conn2.open_bi().await?;
    send1.write_all(b"GET /test HTTP/1.1\r\n\r\n").await?;
    send1.finish()?;
    let resp1 = recv1.read_to_end(16 * 1024).await?;
    let resp1_str = String::from_utf8_lossy(&resp1);
    assert!(
        resp1_str.contains("200 OK"),
        "pre-revocation request should succeed, got: {resp1_str}"
    );

    // ── Phase 2: revoke the client ───────────────────────────────────────
    {
        let mut s = auth.lock().expect("lock");
        s.revoke_client(&client_id).map_err(|e| anyhow::anyhow!(e))?;
    }

    // Verify revocation took effect
    assert!(!auth
        .lock()
        .expect("lock")
        .is_endpoint_allowed(&client_ep.id().to_string()));

    // ── Phase 3: client should be blocked ────────────────────────────────
    // Try to open a new bi-stream on the same connection — the server should
    // either close the connection or refuse the stream.
    let result = async {
        let (mut send2, mut recv2) = conn2.open_bi().await?;
        send2.write_all(b"GET /after-revoke HTTP/1.1\r\n\r\n").await?;
        send2.finish()?;
        let resp2 = recv2.read_to_end(16 * 1024).await?;
        Ok::<Vec<u8>, anyhow::Error>(resp2)
    }
    .await;

    match result {
        Err(e) => {
            // Expected: connection closed / stream reset / error
            let err_str = format!("{e:?}");
            eprintln!("Post-revocation request correctly failed: {err_str}");
        }
        Ok(resp2) => {
            // If we somehow got a response, it should NOT be a 200
            let resp2_str = String::from_utf8_lossy(&resp2);
            assert!(
                !resp2_str.contains("200 OK"),
                "revoked client should NOT get a 200 OK, got: {resp2_str}"
            );
        }
    }

    // ── Phase 4: new connection should also be rejected ──────────────────
    // The revoked client tries to connect again from scratch
    // Start a fresh auth-checking server
    let srv3 = server_ep.clone();
    let auth3 = auth.clone();
    let serve_handle2 = tokio::spawn(async move { serve_with_auth_check(srv3, auth3, 1).await });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let conn3 = client_ep.connect(server_addr, ALPN).await?;
    let new_conn_result = async {
        let (mut send3, mut recv3) = conn3.open_bi().await?;
        send3.write_all(b"GET /new-conn HTTP/1.1\r\n\r\n").await?;
        send3.finish()?;
        let resp3 = recv3.read_to_end(16 * 1024).await?;
        Ok::<Vec<u8>, anyhow::Error>(resp3)
    }
    .await;

    match new_conn_result {
        Err(e) => {
            eprintln!("New connection after revocation correctly failed: {e:?}");
        }
        Ok(resp3) => {
            let resp3_str = String::from_utf8_lossy(&resp3);
            assert!(
                !resp3_str.contains("200 OK"),
                "revoked client on NEW connection should NOT get 200 OK, got: {resp3_str}"
            );
        }
    }

    // Clean up
    let _ = timeout(Duration::from_secs(2), serve_handle).await;
    let _ = timeout(Duration::from_secs(2), serve_handle2).await;
    Ok(())
}

#[tokio::test]
async fn endpoint_info_roundtrip() -> anyhow::Result<()> {
    // Test that we can create an endpoint, get its ID and relay, and use them
    let ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    ep.online().await;

    let id = ep.id().to_string();
    assert!(!id.is_empty());

    let addr = ep.watch_addr().get();
    // Should have at least a relay URL
    let relay_urls: Vec<_> = addr.relay_urls().collect();
    assert!(!relay_urls.is_empty(), "endpoint should have at least one relay URL");

    Ok(())
}
