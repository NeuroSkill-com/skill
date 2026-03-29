//! # iroh-example-client
//!
//! End-to-end example demonstrating the full Skill iroh authorization flow:
//!
//! 1. Create a TOTP secret (server-side, via `IrohAuthStore`)
//! 2. Extract the secret from the `otpauth://` URL
//! 3. Spin up a local iroh `Endpoint` and retrieve its node-id + relay URL
//! 4. Generate the current TOTP code from the shared secret
//! 5. Connect to the Skill iroh server and register via HTTP-over-QUIC
//!
//! In production the TOTP secret is provisioned via QR code on the phone.
//! This example does everything locally for demonstration purposes.

use std::sync::Arc;

/// The ALPN protocol identifier used by Skill's iroh tunnel.
const IROH_ALPN: &[u8] = b"skill/http-ws/1";

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Extract raw secret bytes from an `otpauth://` URL.
fn secret_bytes_from_otpauth(otpauth_url: &str) -> anyhow::Result<Vec<u8>> {
    let parsed = url::Url::parse(otpauth_url)?;
    let b32 = parsed
        .query_pairs()
        .find(|(k, _)| k == "secret")
        .map(|(_, v)| v.into_owned())
        .ok_or_else(|| anyhow::anyhow!("otpauth URL missing `secret` query parameter"))?;
    base32::decode(base32::Alphabet::RFC4648 { padding: false }, &b32)
        .ok_or_else(|| anyhow::anyhow!("invalid base32 in otpauth secret"))
}

/// Generate the current 6-digit TOTP code.
fn generate_totp(secret: &[u8], account_name: &str) -> anyhow::Result<String> {
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,  // digits
        1,  // skew
        30, // period
        secret.to_vec(),
        Some("Skill".to_string()),
        account_name.to_string(),
    )?;
    Ok(totp.generate_current()?)
}

// ─── mock server (for the self-contained example) ─────────────────────────────

/// A tiny iroh server that accepts one registration request.
/// In production this runs inside the Skill app.
async fn run_mock_server(
    server_ep: iroh::Endpoint,
    skill_dir: Arc<std::path::PathBuf>,
) -> anyhow::Result<serde_json::Value> {
    let incoming = server_ep
        .accept()
        .await
        .ok_or_else(|| anyhow::anyhow!("no incoming connection"))?;
    let conn = incoming.await?;
    let (mut send, mut recv) = conn.accept_bi().await?;

    // Read the full request (client will call finish() after writing)
    let raw = recv.read_to_end(64 * 1024).await?;
    let raw_str = String::from_utf8_lossy(&raw);

    let body_str = raw_str.split("\r\n\r\n").nth(1).unwrap_or("");
    let req: serde_json::Value = serde_json::from_str(body_str)?;

    let endpoint_id = req["endpoint_id"].as_str().unwrap_or("");
    let otp = req["otp"].as_str().unwrap_or("");
    let totp_id = req.get("totp_id").and_then(|v| v.as_str());
    let name = req.get("name").and_then(|v| v.as_str());
    let scope = req.get("scope").and_then(|v| v.as_str());

    let mut store = skill_iroh::IrohAuthStore::open(&skill_dir);
    let (status, resp_body) = match store.register_client(endpoint_id, otp, totp_id, name, scope) {
        Ok(client) => {
            let _ = store.mark_client_connected(&client.endpoint_id, "example-server", None);
            ("200 OK", serde_json::json!({"ok": true, "client": client}))
        }
        Err(e) => ("400 Bad Request", serde_json::json!({"ok": false, "error": e})),
    };

    let body = resp_body.to_string();
    let http_resp = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    send.write_all(http_resp.as_bytes()).await?;
    send.finish()?;
    send.stopped().await?;

    Ok(resp_body)
}

// ─── main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║  Skill iroh — end-to-end authorization example          ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // ── Step 1: Create a temp "skill dir" and provision a TOTP secret ────────
    let tmp = tempfile::tempdir()?;
    let skill_dir = tmp.path().to_path_buf();
    let skill_dir_arc = Arc::new(skill_dir.clone());

    let mut store = skill_iroh::IrohAuthStore::open(&skill_dir);
    let (totp_view, otpauth_url, _qr_png_b64) = store.create_totp("example-phone").map_err(|e| anyhow::anyhow!(e))?;

    println!("1. TOTP provisioned");
    println!("   id:          {}", totp_view.id);
    println!("   name:        {}", totp_view.name);
    println!("   otpauth URL: {}", otpauth_url);
    println!();

    // ── Step 2: Extract the shared secret ────────────────────────────────────
    let secret = secret_bytes_from_otpauth(&otpauth_url)?;
    println!(
        "2. Extracted TOTP secret ({} bytes, base32 in otpauth URL)",
        secret.len()
    );
    println!();

    // ── Step 3: Spin up iroh endpoints (server + client) ─────────────────────
    let server_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .alpns(vec![IROH_ALPN.to_vec()])
        .bind()
        .await?;
    server_ep.online().await;

    let server_id = server_ep.id().to_string();
    let server_addr = {
        use iroh::Watcher;
        server_ep.watch_addr().get()
    };
    let relay_url = server_addr
        .relay_urls()
        .next()
        .map(|u| u.to_string())
        .unwrap_or_else(|| "<none>".into());

    println!("3. Server iroh endpoint online");
    println!("   endpoint_id: {server_id}");
    println!("   relay URL:   {relay_url}");
    println!();

    let client_ep = iroh::Endpoint::builder(iroh::endpoint::presets::N0).bind().await?;
    client_ep.online().await;

    let client_id = client_ep.id().to_string();
    let client_addr = {
        use iroh::Watcher;
        client_ep.watch_addr().get()
    };
    let client_relay = client_addr
        .relay_urls()
        .next()
        .map(|u| u.to_string())
        .unwrap_or_else(|| "<none>".into());

    println!("   Client iroh endpoint online");
    println!("   endpoint_id: {client_id}");
    println!("   relay URL:   {client_relay}");
    println!();

    // ── Step 4: Generate the current TOTP code ───────────────────────────────
    let totp_code = generate_totp(&secret, &totp_view.name)?;
    println!("4. Generated TOTP code: {totp_code}");
    println!();

    // ── Step 5: Connect to server and register ───────────────────────────────
    println!("5. Connecting client → server over iroh...");

    let srv_ep = server_ep.clone();
    let sd = skill_dir_arc.clone();
    let server_handle = tokio::spawn(async move { run_mock_server(srv_ep, sd).await });

    let conn = client_ep.connect(server_addr, IROH_ALPN).await?;
    println!("   Connected! Remote peer: {}", conn.remote_id());

    let register_body = serde_json::json!({
        "endpoint_id": client_id,
        "otp": totp_code,
        "name": "example-client",
        "scope": "read",
    });
    let body_str = register_body.to_string();
    let http_req = format!(
        "POST /v1/iroh/clients/register HTTP/1.1\r\n\
         Host: localhost\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         \r\n\
         {body_str}",
        body_str.len()
    );

    let (mut send, mut recv) = conn.open_bi().await?;
    send.write_all(http_req.as_bytes()).await?;
    send.finish()?;

    let resp_raw = recv.read_to_end(16 * 1024).await?;
    let resp_text = String::from_utf8_lossy(&resp_raw);

    // Parse HTTP response body
    let resp_body_str = resp_text.split("\r\n\r\n").nth(1).unwrap_or("");
    let resp_json: serde_json::Value = serde_json::from_str(resp_body_str)?;

    println!("\n   Server response:");
    println!("   {}", serde_json::to_string_pretty(&resp_json)?);

    // Wait for server task
    let server_result = server_handle.await??;
    assert_eq!(
        server_result.get("ok").and_then(|v| v.as_bool()),
        Some(true),
        "registration should succeed"
    );

    // ── Step 6: Verify persisted state ───────────────────────────────────────
    println!("\n6. Verifying persisted auth state...");
    let store2 = skill_iroh::IrohAuthStore::open(&skill_dir);

    let totps = store2.list_totp();
    println!("   TOTP entries: {}", totps.len());
    for t in &totps {
        println!(
            "     • {} (id={}, revoked={}, last_used={:?})",
            t.name,
            t.id,
            t.revoked_at.is_some(),
            t.last_used_at
        );
    }

    let clients = store2.list_clients();
    println!("   Client entries: {}", clients.len());
    for c in &clients {
        println!(
            "     • {} (endpoint={}, scope={}, connected_at={:?})",
            c.name, c.endpoint_id, c.scope, c.last_connected_at
        );
    }

    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0].name, "example-client");
    assert_eq!(clients[0].scope, "read");
    assert!(clients[0].last_connected_at.is_some());
    assert!(store2.is_endpoint_allowed(&client_id));

    // ── Step 7: Demonstrate the combined invite QR payload ─────────────
    println!("\n7. Building combined invite QR payload (endpoint+relay+secret)...");
    // In production this is what the phone scans — one QR with everything.
    let store3 = skill_iroh::IrohAuthStore::open(&skill_dir);
    let totp_id = &totps[0].id;
    let invite = store3
        .build_invite_payload(totp_id, &server_id, &relay_url)
        .map_err(|e| anyhow::anyhow!(e))?;

    println!("   Invite payload:");
    println!("     endpoint_id:  {}", invite.endpoint_id);
    println!("     relay_url:    {}", invite.relay_url);
    println!("     totp_id:      {}", invite.totp_id);
    println!("     secret_base32: {}…", &invite.secret_base32[..8]);
    println!("     name:         {}", invite.name);

    // Verify it round-trips through JSON (what goes into the QR)
    let invite_json = serde_json::to_string(&invite)?;
    let decoded: skill_iroh::IrohInvitePayload = serde_json::from_str(&invite_json)?;
    assert_eq!(decoded.endpoint_id, server_id);
    assert_eq!(decoded.relay_url, relay_url);
    assert!(!decoded.secret_base32.is_empty());

    // Verify the phone could use the decoded secret to generate a valid TOTP
    let phone_code = generate_totp(
        &base32::decode(base32::Alphabet::RFC4648 { padding: false }, &decoded.secret_base32)
            .ok_or_else(|| anyhow::anyhow!("bad base32"))?,
        &decoded.name,
    )?;
    println!("   Phone-side TOTP code from invite: {phone_code}");

    println!("\n✅ All checks passed! End-to-end iroh authorization flow works.");
    println!("   The invite QR encodes: endpoint_id + relay_url + TOTP secret.");
    Ok(())
}
