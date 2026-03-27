//! Full end-to-end lifecycle simulation:
//!
//!  1. Provision TOTP on the "server" (Skill app)
//!  2. Build the combined invite payload (endpoint+relay+secret) — what the QR encodes
//!  3. "Phone" decodes the invite, spins up its own iroh endpoint
//!  4. Phone generates a TOTP code from the shared secret
//!  5. Phone connects to Skill over iroh and registers with the OTP
//!  6. Phone sends a request → gets 200 OK
//!  7. Skill changes the client's scope from read → full
//!  8. Phone sends another request → still OK (scope change doesn't disconnect)
//!  9. Skill revokes the client
//! 10. Phone tries another request on the SAME connection → connection closed ("revoked")
//! 11. Phone tries a BRAND-NEW connection → also closed ("revoked")
//! 12. Verify the auth store state is consistent throughout

use std::sync::{Arc, Mutex};
use iroh::Watcher;
use tempfile::TempDir;
use tokio::time::Duration;

const ALPN: &[u8] = b"skill/http-ws/1";

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn secret_from_b32(b32: &str) -> Vec<u8> {
    base32::decode(base32::Alphabet::RFC4648 { padding: false }, b32)
        .expect("valid base32")
}

fn generate_totp(secret: &[u8], name: &str) -> String {
    totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1, 6, 1, 30,
        secret.to_vec(),
        Some("Skill".to_string()),
        name.to_string(),
    )
    .expect("totp")
    .generate_current()
    .expect("code")
}

/// Server that mirrors the real Skill tunnel behavior:
/// - Accepts any connection (QUIC layer can't pre-filter)
/// - First bi-stream may be a registration request (no auth needed for that)
/// - Subsequent bi-streams require auth; revoked clients get conn.close("revoked")
async fn skill_server(
    server_ep: iroh::Endpoint,
    auth: Arc<Mutex<skill_iroh::IrohAuthStore>>,
    max_connections: usize,
) {
    for _ in 0..max_connections {
        let incoming = match server_ep.accept().await {
            Some(i) => i,
            None => return,
        };
        let conn = match incoming.await {
            Ok(c) => c,
            Err(_) => continue,
        };
        let peer = conn.remote_id().to_string().to_lowercase();
        let auth = auth.clone();

        tokio::spawn(async move {
            loop {
                // Accept the bi-stream first (registration needs to get through)
                let (mut send, mut recv) = match conn.accept_bi().await {
                    Ok(s) => s,
                    Err(_) => return,
                };

                // Read request
                let raw = match recv.read_to_end(64 * 1024).await {
                    Ok(r) => r,
                    Err(_) => return,
                };
                let raw_str = String::from_utf8_lossy(&raw);
                let is_register = raw_str.contains("/v1/iroh/clients/register");

                // Auth check: registration requests are always allowed (they
                // carry a TOTP code); everything else requires the peer to be
                // an authorized client.
                if !is_register {
                    let store = auth.lock().expect("lock");
                    if !store.is_endpoint_allowed(&peer) {
                        drop(store);
                        conn.close(1u32.into(), b"revoked");
                        return;
                    }
                }

                let response = if is_register {
                    let body_str = raw_str.split("\r\n\r\n").nth(1).unwrap_or("");
                    let v: serde_json::Value = serde_json::from_str(body_str)
                        .unwrap_or_else(|_| serde_json::json!({}));
                    let eid = v["endpoint_id"].as_str().unwrap_or("");
                    let otp = v["otp"].as_str().unwrap_or("");
                    let tid = v.get("totp_id").and_then(|v| v.as_str());
                    let name = v.get("name").and_then(|v| v.as_str());
                    let scope = v.get("scope").and_then(|v| v.as_str());

                    let mut store = auth.lock().expect("lock");
                    match store.register_client(eid, otp, tid, name, scope) {
                        Ok(client) => {
                            let _ = store.mark_client_connected(&client.endpoint_id, "test", None);
                            let body = serde_json::json!({"ok": true, "client": client}).to_string();
                            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len())
                        }
                        Err(e) => {
                            let body = serde_json::json!({"ok": false, "error": e}).to_string();
                            format!("HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\n\r\n{body}", body.len())
                        }
                    }
                } else {
                    let first_line = raw_str.lines().next().unwrap_or("");
                    let body = serde_json::json!({"ok": true, "echo": first_line}).to_string();
                    format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len())
                };

                let _ = send.write_all(response.as_bytes()).await;
                let _ = send.finish();
                let _ = send.stopped().await;
            }
        });
    }
}

/// Send a request over an existing connection and return the parsed JSON body.
async fn request(conn: &iroh::endpoint::Connection, path: &str) -> Result<serde_json::Value, String> {
    let req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n");
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| format!("open_bi: {e}"))?;
    send.write_all(req.as_bytes()).await.map_err(|e| format!("write: {e}"))?;
    send.finish().map_err(|e| format!("finish: {e}"))?;
    let resp = recv.read_to_end(16 * 1024).await.map_err(|e| format!("read: {e}"))?;
    let text = String::from_utf8_lossy(&resp);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("{}");
    serde_json::from_str(body).map_err(|e| format!("json: {e}"))
}

/// Send a registration POST over an existing connection.
async fn register(
    conn: &iroh::endpoint::Connection,
    body_json: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let body = body_json.to_string();
    let req = format!(
        "POST /v1/iroh/clients/register HTTP/1.1\r\nHost: localhost\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| format!("open_bi: {e}"))?;
    send.write_all(req.as_bytes()).await.map_err(|e| format!("write: {e}"))?;
    send.finish().map_err(|e| format!("finish: {e}"))?;
    let resp = recv.read_to_end(16 * 1024).await.map_err(|e| format!("read: {e}"))?;
    let text = String::from_utf8_lossy(&resp);
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("{}");
    serde_json::from_str(body).map_err(|e| format!("json: {e}"))
}

// ─── The test ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn full_lifecycle_simulation() -> anyhow::Result<()> {
    let td = TempDir::new()?;
    let skill_dir = td.path();

    // Shared auth store (like the real Skill app holds in Arc<Mutex<>>)
    let auth: Arc<Mutex<skill_iroh::IrohAuthStore>> = Arc::new(Mutex::new(
        skill_iroh::IrohAuthStore::open(skill_dir),
    ));

    // ══════════════════════════════════════════════════════════════════════
    // Step 1: Skill provisions a TOTP
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 1: Skill provisions a TOTP");
    let (totp_view, otpauth_url, _qr) = {
        let mut store = auth.lock().expect("lock");
        store.create_totp("phone-1").map_err(|e| anyhow::anyhow!(e))?
    };
    eprintln!("    TOTP id:   {}", totp_view.id);
    eprintln!("    otpauth:   {}", &otpauth_url[..60]);

    // ══════════════════════════════════════════════════════════════════════
    // Step 2: Build combined invite payload (endpoint+relay+secret)
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 2: Skill starts iroh endpoint & builds invite");
    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;
    server_ep.online().await;

    let server_id = server_ep.id().to_string();
    let server_addr = server_ep.watch_addr().get();
    let relay_url = server_addr.relay_urls().next()
        .map(|u| u.to_string())
        .unwrap_or_default();

    let invite = {
        let store = auth.lock().expect("lock");
        store.build_invite_payload(&totp_view.id, &server_id, &relay_url)
            .map_err(|e| anyhow::anyhow!(e))?
    };
    let invite_json = serde_json::to_string(&invite)?;
    eprintln!("    Invite payload ({} bytes):", invite_json.len());
    eprintln!("      endpoint_id:  {}", invite.endpoint_id);
    eprintln!("      relay_url:    {}", invite.relay_url);
    eprintln!("      secret:       {}…", &invite.secret_base32[..8]);

    // Start the Skill server (accepts up to 5 connections)
    let srv = server_ep.clone();
    let auth2 = auth.clone();
    let _server_task = tokio::spawn(async move {
        skill_server(srv, auth2, 5).await;
    });

    // ══════════════════════════════════════════════════════════════════════
    // Step 3: Phone decodes the invite and creates its endpoint
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 3: Phone decodes invite, spins up endpoint");
    let decoded: skill_iroh::IrohInvitePayload = serde_json::from_str(&invite_json)?;
    let secret = secret_from_b32(&decoded.secret_base32);

    let phone_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .bind().await?;
    phone_ep.online().await;
    let phone_id = phone_ep.id().to_string();
    eprintln!("    Phone endpoint: {}", &phone_id[..16]);

    // ══════════════════════════════════════════════════════════════════════
    // Step 4: Phone generates TOTP code
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 4: Phone generates TOTP code");
    let code = generate_totp(&secret, &decoded.name);
    eprintln!("    TOTP code: {code}");

    // ══════════════════════════════════════════════════════════════════════
    // Step 5: Phone connects and registers
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 5: Phone connects to Skill and registers");
    let conn = phone_ep.connect(server_addr.clone(), ALPN).await?;
    eprintln!("    Connected to: {}", conn.remote_id());

    let reg = register(&conn, &serde_json::json!({
        "endpoint_id": phone_id,
        "otp": code,
        "name": "my-phone",
        "scope": "read",
    })).await.map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(reg["ok"], true, "registration should succeed: {reg}");
    eprintln!("    Registered: {}", reg["client"]["id"]);
    let client_id = reg["client"]["id"].as_str().expect("client id").to_string();

    // Verify auth store
    {
        let store = auth.lock().expect("lock");
        assert!(store.is_endpoint_allowed(&phone_id));
        assert_eq!(store.scope_for_endpoint(&phone_id).as_deref(), Some("read"));
        let clients = store.list_clients();
        assert_eq!(clients.len(), 1);
        assert_eq!(clients[0].name, "my-phone");
        assert!(clients[0].last_connected_at.is_some());
    }
    eprintln!("    ✓ Auth store verified");

    // ══════════════════════════════════════════════════════════════════════
    // Step 6: Phone sends a normal request → 200 OK
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 6: Phone sends request (pre-revocation)");
    let resp = request(&conn, "/v1/status").await
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(resp["ok"], true);
    eprintln!("    ✓ Got 200 OK: {resp}");

    // ══════════════════════════════════════════════════════════════════════
    // Step 7: Skill changes scope to "full"
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 7: Skill changes scope read → full");
    {
        let mut store = auth.lock().expect("lock");
        store.set_client_scope(&client_id, "full").map_err(|e| anyhow::anyhow!(e))?;
        assert_eq!(store.scope_for_endpoint(&phone_id).as_deref(), Some("full"));
    }
    eprintln!("    ✓ Scope is now 'full'");

    // ══════════════════════════════════════════════════════════════════════
    // Step 8: Phone sends another request → still OK
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 8: Phone sends request (after scope change)");
    let resp2 = request(&conn, "/v1/sessions").await
        .map_err(|e| anyhow::anyhow!(e))?;
    assert_eq!(resp2["ok"], true);
    eprintln!("    ✓ Got 200 OK (scope change didn't break connection)");

    // ══════════════════════════════════════════════════════════════════════
    // Step 9: Skill revokes the client
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 9: Skill revokes the client");
    {
        let mut store = auth.lock().expect("lock");
        store.revoke_client(&client_id).map_err(|e| anyhow::anyhow!(e))?;
        assert!(!store.is_endpoint_allowed(&phone_id));
    }
    eprintln!("    ✓ Client revoked");

    // ══════════════════════════════════════════════════════════════════════
    // Step 10: Phone tries same connection → blocked
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 10: Phone tries request on SAME connection (post-revocation)");
    // Small delay to let server's auth check loop cycle
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = request(&conn, "/v1/status").await;
    match &result {
        Err(e) => {
            assert!(
                e.contains("revoked") || e.contains("connection lost") || e.contains("closed"),
                "expected revocation error, got: {e}"
            );
            eprintln!("    ✓ Blocked: {e}");
        }
        Ok(v) => {
            panic!("revoked client should NOT get a response, got: {v}");
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Step 11: Phone tries a brand-new connection → also blocked
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 11: Phone opens NEW connection (post-revocation)");
    let conn2 = phone_ep.connect(server_addr, ALPN).await?;
    // Small delay for server to accept and check auth
    tokio::time::sleep(Duration::from_millis(50)).await;

    let result2 = request(&conn2, "/v1/status").await;
    match &result2 {
        Err(e) => {
            assert!(
                e.contains("revoked") || e.contains("connection lost") || e.contains("closed"),
                "expected revocation error on new connection, got: {e}"
            );
            eprintln!("    ✓ New connection also blocked: {e}");
        }
        Ok(v) => {
            panic!("revoked client on NEW connection should NOT get a response, got: {v}");
        }
    }

    // ══════════════════════════════════════════════════════════════════════
    // Step 12: Verify final auth store state
    // ══════════════════════════════════════════════════════════════════════
    eprintln!("\n═══ Step 12: Final auth store verification");
    {
        let store = auth.lock().expect("lock");

        // TOTP should still exist with last_used_at set
        let totps = store.list_totp();
        assert_eq!(totps.len(), 1);
        assert!(totps[0].last_used_at.is_some(), "TOTP should have last_used_at");
        assert!(totps[0].revoked_at.is_none(), "TOTP itself was NOT revoked");
        eprintln!("    TOTP: name={}, last_used={:?}", totps[0].name, totps[0].last_used_at);

        // Client should be revoked
        let clients = store.list_clients();
        assert_eq!(clients.len(), 1);
        assert!(clients[0].revoked_at.is_some(), "client should be revoked");
        assert_eq!(clients[0].scope, "full", "scope should still be 'full' (last set)");
        assert!(!store.is_endpoint_allowed(&phone_id));
        eprintln!("    Client: name={}, scope={}, revoked_at={:?}",
            clients[0].name, clients[0].scope, clients[0].revoked_at);

        // Persistence: reload from disk and verify
        let store2 = skill_iroh::IrohAuthStore::open(td.path());
        assert_eq!(store2.list_clients().len(), 1);
        assert!(store2.list_clients()[0].revoked_at.is_some());
        assert!(!store2.is_endpoint_allowed(&phone_id));
        eprintln!("    ✓ Disk state matches in-memory state");
    }

    eprintln!("\n╔═══════════════════════════════════════════════════════╗");
    eprintln!("║  ✅ Full lifecycle simulation passed (12/12 steps)    ║");
    eprintln!("╚═══════════════════════════════════════════════════════╝\n");
    Ok(())
}
