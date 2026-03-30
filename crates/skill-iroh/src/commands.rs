// SPDX-License-Identifier: GPL-3.0-only

use base64::{engine::general_purpose, Engine as _};
use image::codecs::png::PngEncoder;
use image::ColorType;
use image::ImageEncoder;
use qrcodegen::{QrCode, QrCodeEcc};

use crate::{lock_or_recover, SharedIrohAuth, SharedIrohRuntime};
use serde_json::{json, Value};

/// Render a JSON string into a QR-code PNG and return it as a data-URI.
fn json_to_qr_data_uri(payload_json: &str) -> Result<String, String> {
    let qr = QrCode::encode_text(payload_json, QrCodeEcc::Medium).map_err(|e| format!("QR encode error: {e}"))?;
    let size = qr.size();
    let scale = 8; // pixels per module
    let border = 2 * scale;
    let img_size = (size * scale + 2 * border) as u32;
    let mut img = vec![255u8; (img_size * img_size) as usize];
    for y in 0..size {
        for x in 0..size {
            let color = if qr.get_module(x, y) { 0 } else { 255 };
            for dy in 0..scale {
                for dx in 0..scale {
                    let px = (y * scale + dy + border) as u32;
                    let py = (x * scale + dx + border) as u32;
                    let idx = (px * img_size + py) as usize;
                    img[idx] = color;
                }
            }
        }
    }
    let mut buf = Vec::new();
    let encoder = PngEncoder::new(&mut buf);
    encoder
        .write_image(&img, img_size, img_size, ColorType::L8.into())
        .map_err(|e| format!("QR PNG encode error: {e}"))?;
    let b64 = general_purpose::STANDARD.encode(&buf);
    Ok(format!("data:image/png;base64,{b64}"))
}

/// Generate a phone-invite QR that encodes **everything** a mobile client needs:
///   endpoint_id + relay_url + TOTP secret + totp_id + name
///
/// The caller can either pass an existing `totp_id` or a `name` to create a new
/// TOTP on the fly.  The `endpoint_id` and `relay_url` are read from the
/// running iroh tunnel runtime state.
///
/// Request JSON:
///   { "name": "phone" }              — creates a new TOTP and returns the invite
///   { "totp_id": "totp_..." }        — reuses an existing (non-revoked) TOTP
///
/// Response JSON:
///   { "payload": { endpoint_id, relay_url, totp_id, secret_base32, name, created_at },
///     "otpauth_url": "otpauth://totp/...",
///     "qr_png_base64": "data:image/png;base64,..." }
pub fn iroh_phone_invite(auth: &SharedIrohAuth, runtime: &SharedIrohRuntime, msg: &Value) -> Result<Value, String> {
    // Grab endpoint info from the running tunnel
    let (endpoint_id, relay_url) = {
        let r = lock_or_recover(runtime);
        if !r.online {
            return Err("iroh tunnel is not online yet".into());
        }
        (r.endpoint_id.clone(), r.relay_url.clone())
    };

    // Resolve or create the TOTP entry
    let totp_id = if let Some(id) = msg.get("totp_id").and_then(Value::as_str) {
        id.to_string()
    } else {
        let name = msg
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "provide either \"totp_id\" or \"name\" to create a new TOTP".to_string())?;
        let (view, _, _) = lock_or_recover(auth).create_totp(name)?;
        view.id
    };

    let invite = lock_or_recover(auth).build_invite_payload(&totp_id, &endpoint_id, &relay_url)?;
    let payload_json = serde_json::to_string(&invite).map_err(|e| format!("serialize: {e}"))?;
    let qr_data_uri = json_to_qr_data_uri(&payload_json)?;

    // Also produce the standard otpauth:// URL for authenticator apps
    let (otpauth_url, _) = lock_or_recover(auth).totp_qr(&totp_id)?;

    Ok(json!({
        "payload": invite,
        "otpauth_url": otpauth_url,
        "qr_png_base64": qr_data_uri,
    }))
}

pub fn iroh_info(auth: &SharedIrohAuth, runtime: &SharedIrohRuntime) -> Result<Value, String> {
    let r = lock_or_recover(runtime).clone();

    let a = lock_or_recover(auth);
    let totp_total = a.list_totp().len();
    let clients = a.list_clients();
    let clients_total = clients.len();
    let clients_active = clients.iter().filter(|c| c.revoked_at.is_none()).count();

    Ok(json!({
        "online": r.online,
        "endpoint_id": r.endpoint_id,
        "relay_url": r.relay_url,
        "direct_addrs": r.direct_addrs,
        "local_port": r.local_port,
        "started_at": r.started_at,
        "last_error": r.last_error,
        "auth": {
            "totp_total": totp_total,
            "clients_total": clients_total,
            "clients_active": clients_active,
        }
    }))
}

pub fn iroh_totp_list(auth: &SharedIrohAuth) -> Result<Value, String> {
    let mut rows = lock_or_recover(auth).list_totp();
    rows.sort_by_key(|r| r.created_at);
    rows.reverse();
    Ok(json!({ "totp": rows }))
}

pub fn iroh_totp_create(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let name = msg
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"name\" (string)".to_string())?;

    let (totp, otpauth_url, qr_png_base64) = lock_or_recover(auth).create_totp(name)?;

    Ok(json!({
        "totp": totp,
        "otpauth_url": otpauth_url,
        "qr_png_base64": qr_png_base64,
    }))
}

pub fn iroh_totp_qr(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let id = msg
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;

    let (otpauth_url, qr_png_base64) = lock_or_recover(auth).totp_qr(id)?;

    Ok(json!({
        "id": id,
        "otpauth_url": otpauth_url,
        "qr_png_base64": qr_png_base64,
    }))
}

pub fn iroh_totp_revoke(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let id = msg
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;

    lock_or_recover(auth).revoke_totp(id)?;
    Ok(json!({ "revoked": true, "id": id }))
}

pub fn iroh_clients_list(auth: &SharedIrohAuth) -> Result<Value, String> {
    let mut rows = lock_or_recover(auth).list_clients();
    rows.sort_by_key(|r| r.created_at);
    rows.reverse();
    Ok(json!({ "clients": rows }))
}

pub fn iroh_client_register(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let endpoint_id = msg
        .get("endpoint_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"endpoint_id\" (string)".to_string())?;
    let otp = msg
        .get("otp")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"otp\" (string)".to_string())?;
    let totp_id = msg.get("totp_id").and_then(Value::as_str);
    let name = msg.get("name").and_then(Value::as_str);
    let scope = msg.get("scope").and_then(Value::as_str);

    let client = lock_or_recover(auth).register_client(endpoint_id, otp, totp_id, name, scope)?;

    Ok(json!({ "client": client, "registered": true }))
}

pub fn iroh_client_revoke(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let id = msg
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;

    lock_or_recover(auth).revoke_client(id)?;
    Ok(json!({ "revoked": true, "id": id }))
}

pub fn iroh_client_set_scope(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let id = msg
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;
    let scope = msg
        .get("scope")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"scope\" (string: read|full|custom)".to_string())?;

    let groups: Option<Vec<String>> = msg
        .get("groups")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());

    let allow: Option<Vec<String>> = msg
        .get("allow")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());

    let deny: Option<Vec<String>> = msg
        .get("deny")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).map(String::from).collect());

    let perms = lock_or_recover(auth).set_client_permissions(id, scope, groups, allow, deny)?;

    let dangerous = crate::scope::dangerous_groups_enabled(&perms);
    let danger_labels: Vec<&str> = dangerous.iter().map(|g| g.label).collect();

    Ok(json!({
        "ok": true,
        "id": id,
        "permissions": perms,
        "warnings": if dangerous.is_empty() {
            Vec::new()
        } else {
            let mut w = vec!["⚠ DANGEROUS groups enabled:".to_string()];
            for g in &dangerous {
                w.push(format!("  • {} — {}", g.label, g.description));
            }
            w
        },
        "dangerous_groups": danger_labels,
    }))
}

/// List all available command groups and their commands.
pub fn iroh_scope_groups(_auth: &SharedIrohAuth) -> Result<Value, String> {
    let groups: Vec<Value> = crate::scope::GROUPS
        .iter()
        .map(|g| {
            json!({
                "id": g.id,
                "label": g.label,
                "description": g.description,
                "dangerous": g.dangerous,
                "commands": g.commands,
            })
        })
        .collect();

    Ok(json!({
        "groups": groups,
        "builtin_scopes": {
            "read": crate::scope::READ_GROUPS,
            "full": "all groups",
        }
    }))
}

/// Return the resolved permissions for a specific client.
pub fn iroh_client_permissions(auth: &SharedIrohAuth, msg: &Value) -> Result<Value, String> {
    let id = msg
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing required field: \"id\" (string)".to_string())?;

    let a = lock_or_recover(auth);
    let clients = a.list_clients();
    let client = clients
        .iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("unknown client id: {id}"))?;

    let report = crate::scope::permission_report(&client.permissions);
    Ok(json!({
        "id": id,
        "name": client.name,
        "endpoint_id": client.endpoint_id,
        "report": report,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{totp_from_entry, IrohAuthStore};
    use crate::IrohRuntimeState;
    use std::sync::{Arc, Mutex};

    fn make_auth(dir: &std::path::Path) -> SharedIrohAuth {
        Arc::new(Mutex::new(IrohAuthStore::open(dir)))
    }

    fn make_runtime() -> SharedIrohRuntime {
        Arc::new(Mutex::new(IrohRuntimeState::default()))
    }

    // ── iroh_info ────────────────────────────────────────────────────────

    #[test]
    fn info_returns_default_state() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        let v = iroh_info(&auth, &rt).expect("info");
        assert_eq!(v["online"], false);
        assert_eq!(v["endpoint_id"], "");
        assert_eq!(v["auth"]["totp_total"], 0);
        assert_eq!(v["auth"]["clients_total"], 0);
        assert_eq!(v["auth"]["clients_active"], 0);
    }

    #[test]
    fn info_reflects_runtime_state() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        {
            let mut r = rt.lock().expect("lock");
            r.online = true;
            r.endpoint_id = "abc123".into();
            r.relay_url = "https://relay.example.com".into();
            r.local_port = 9999;
        }
        let v = iroh_info(&auth, &rt).expect("info");
        assert_eq!(v["online"], true);
        assert_eq!(v["endpoint_id"], "abc123");
        assert_eq!(v["relay_url"], "https://relay.example.com");
        assert_eq!(v["local_port"], 9999);
    }

    #[test]
    fn info_counts_clients() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();

        // Create a TOTP and register a client
        {
            let mut a = auth.lock().expect("lock");
            a.create_totp("phone").expect("totp");
        }
        let otp = {
            let a = auth.lock().expect("lock");
            let _tentry = a.list_totp()[0].clone();
            // Need to access db directly - use the store's internal totp
            drop(a);
            // Re-open to get totp entry with secret (need raw db for secret_b32)
            let _store = IrohAuthStore::open(td.path());
            let raw: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(td.path().join("iroh_auth.json")).expect("read"))
                    .expect("parse");
            let b32 = raw["totp"][0]["secret_b32"].as_str().expect("secret").to_string();
            let entry = crate::IrohTotpEntry {
                id: raw["totp"][0]["id"].as_str().expect("id").to_string(),
                name: "phone".into(),
                secret_b32: b32,
                created_at: 0,
                revoked_at: None,
                last_used_at: None,
            };
            totp_from_entry(&entry).expect("totp").generate_current().expect("otp")
        };

        {
            let totp_id = auth.lock().expect("lock").list_totp()[0].id.clone();
            auth.lock()
                .expect("lock")
                .register_client("ep1", &otp, Some(&totp_id), Some("dev"), None)
                .expect("register");
        }

        let v = iroh_info(&auth, &rt).expect("info");
        assert_eq!(v["auth"]["totp_total"], 1);
        assert_eq!(v["auth"]["clients_total"], 1);
        assert_eq!(v["auth"]["clients_active"], 1);
    }

    // ── iroh_totp_list ───────────────────────────────────────────────────

    #[test]
    fn totp_list_empty() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let v = iroh_totp_list(&auth).expect("list");
        assert_eq!(v["totp"].as_array().expect("arr").len(), 0);
    }

    #[test]
    fn totp_list_returns_entries() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        auth.lock().expect("lock").create_totp("a").expect("a");
        auth.lock().expect("lock").create_totp("b").expect("b");
        let v = iroh_totp_list(&auth).expect("list");
        assert_eq!(v["totp"].as_array().expect("arr").len(), 2);
    }

    // ── iroh_totp_create ─────────────────────────────────────────────────

    #[test]
    fn totp_create_success() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let msg = json!({"name": "my-phone"});
        let v = iroh_totp_create(&auth, &msg).expect("create");
        assert!(v["otpauth_url"].as_str().expect("url").starts_with("otpauth://"));
        assert!(v["qr_png_base64"]
            .as_str()
            .expect("qr")
            .starts_with("data:image/png;base64,"));
        assert_eq!(v["totp"]["name"], "my-phone");
    }

    #[test]
    fn totp_create_missing_name_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let msg = json!({});
        let err = iroh_totp_create(&auth, &msg).expect_err("err");
        assert!(err.contains("name"));
    }

    // ── iroh_totp_qr ────────────────────────────────────────────────────

    #[test]
    fn totp_qr_success() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let (view, _, _) = auth.lock().expect("lock").create_totp("ph").expect("totp");
        let msg = json!({"id": view.id});
        let v = iroh_totp_qr(&auth, &msg).expect("qr");
        assert!(v["otpauth_url"].as_str().expect("url").starts_with("otpauth://"));
        assert!(v["qr_png_base64"]
            .as_str()
            .expect("qr")
            .starts_with("data:image/png;base64,"));
    }

    #[test]
    fn totp_qr_missing_id_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let err = iroh_totp_qr(&auth, &json!({})).expect_err("err");
        assert!(err.contains("id"));
    }

    // ── iroh_totp_revoke ─────────────────────────────────────────────────

    #[test]
    fn totp_revoke_success() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let (view, _, _) = auth.lock().expect("lock").create_totp("ph").expect("totp");
        let msg = json!({"id": view.id});
        let v = iroh_totp_revoke(&auth, &msg).expect("revoke");
        assert_eq!(v["revoked"], true);
        // Verify it's actually revoked
        let list = iroh_totp_list(&auth).expect("list");
        let t = &list["totp"][0];
        assert!(t["revoked_at"].as_u64().is_some());
    }

    #[test]
    fn totp_revoke_missing_id_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        assert!(iroh_totp_revoke(&auth, &json!({})).is_err());
    }

    // ── iroh_clients_list ────────────────────────────────────────────────

    #[test]
    fn clients_list_empty() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let v = iroh_clients_list(&auth).expect("list");
        assert_eq!(v["clients"].as_array().expect("arr").len(), 0);
    }

    // ── iroh_client_register ─────────────────────────────────────────────

    #[test]
    fn client_register_success() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        auth.lock().expect("lock").create_totp("ph").expect("totp");

        let otp = {
            let raw: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(td.path().join("iroh_auth.json")).expect("read"))
                    .expect("parse");
            let entry = crate::IrohTotpEntry {
                id: raw["totp"][0]["id"].as_str().expect("id").to_string(),
                name: "ph".into(),
                secret_b32: raw["totp"][0]["secret_b32"].as_str().expect("s").to_string(),
                created_at: 0,
                revoked_at: None,
                last_used_at: None,
            };
            totp_from_entry(&entry).expect("totp").generate_current().expect("otp")
        };

        let totp_id = auth.lock().expect("lock").list_totp()[0].id.clone();
        let msg = json!({
            "endpoint_id": "abc123",
            "otp": otp,
            "totp_id": totp_id,
            "name": "dev",
            "scope": "full"
        });
        let v = iroh_client_register(&auth, &msg).expect("register");
        assert_eq!(v["registered"], true);
        assert_eq!(v["client"]["name"], "dev");
        assert_eq!(v["client"]["scope"], "full");
    }

    #[test]
    fn client_register_missing_fields_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        assert!(iroh_client_register(&auth, &json!({})).is_err());
        assert!(iroh_client_register(&auth, &json!({"endpoint_id": "x"})).is_err());
    }

    // ── iroh_client_revoke ───────────────────────────────────────────────

    #[test]
    fn client_revoke_success() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        auth.lock().expect("lock").create_totp("ph").expect("totp");

        let otp = {
            let raw: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(td.path().join("iroh_auth.json")).expect("read"))
                    .expect("parse");
            let entry = crate::IrohTotpEntry {
                id: raw["totp"][0]["id"].as_str().expect("id").to_string(),
                name: "ph".into(),
                secret_b32: raw["totp"][0]["secret_b32"].as_str().expect("s").to_string(),
                created_at: 0,
                revoked_at: None,
                last_used_at: None,
            };
            totp_from_entry(&entry).expect("totp").generate_current().expect("otp")
        };
        let totp_id = auth.lock().expect("lock").list_totp()[0].id.clone();
        auth.lock()
            .expect("lock")
            .register_client("ep1", &otp, Some(&totp_id), Some("dev"), None)
            .expect("register");

        let client_id = auth.lock().expect("lock").list_clients()[0].id.clone();
        let v = iroh_client_revoke(&auth, &json!({"id": client_id})).expect("revoke");
        assert_eq!(v["revoked"], true);
        assert!(!auth.lock().expect("lock").is_endpoint_allowed("ep1"));
    }

    #[test]
    fn client_revoke_missing_id_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        assert!(iroh_client_revoke(&auth, &json!({})).is_err());
    }

    // ── iroh_client_set_scope ────────────────────────────────────────────

    #[test]
    fn client_set_scope_success() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        auth.lock().expect("lock").create_totp("ph").expect("totp");

        let otp = {
            let raw: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(td.path().join("iroh_auth.json")).expect("read"))
                    .expect("parse");
            let entry = crate::IrohTotpEntry {
                id: raw["totp"][0]["id"].as_str().expect("id").to_string(),
                name: "ph".into(),
                secret_b32: raw["totp"][0]["secret_b32"].as_str().expect("s").to_string(),
                created_at: 0,
                revoked_at: None,
                last_used_at: None,
            };
            totp_from_entry(&entry).expect("totp").generate_current().expect("otp")
        };
        let totp_id = auth.lock().expect("lock").list_totp()[0].id.clone();
        auth.lock()
            .expect("lock")
            .register_client("ep1", &otp, Some(&totp_id), Some("dev"), Some("read"))
            .expect("register");

        let client_id = auth.lock().expect("lock").list_clients()[0].id.clone();
        let v = iroh_client_set_scope(&auth, &json!({"id": client_id, "scope": "full"})).expect("set");
        assert_eq!(v["ok"], true);
        let dangerous = v["dangerous_groups"].as_array().expect("dangerous_groups");
        assert!(!dangerous.is_empty(), "full scope should flag dangerous groups");

        // Verify scope changed
        assert_eq!(
            auth.lock().expect("lock").scope_for_endpoint("ep1").as_deref(),
            Some("full")
        );

        // Set back to read
        let v2 = iroh_client_set_scope(&auth, &json!({"id": client_id, "scope": "read"})).expect("set");
        let dangerous2 = v2["dangerous_groups"].as_array().expect("dangerous_groups");
        assert!(dangerous2.is_empty(), "read scope should have no dangerous groups");
    }

    #[test]
    fn client_set_scope_missing_fields_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        assert!(iroh_client_set_scope(&auth, &json!({})).is_err());
        assert!(iroh_client_set_scope(&auth, &json!({"id": "x"})).is_err());
    }

    // ── iroh_phone_invite ────────────────────────────────────────────────

    #[test]
    fn phone_invite_offline_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        // Tunnel not online → error
        let err = iroh_phone_invite(&auth, &rt, &json!({"name": "phone"})).expect_err("err");
        assert!(err.contains("not online"));
    }

    #[test]
    fn phone_invite_creates_totp_and_encodes_all_fields() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        {
            let mut r = rt.lock().expect("lock");
            r.online = true;
            r.endpoint_id = "abcdef1234567890".into();
            r.relay_url = "https://relay.example.com/".into();
        }
        let v = iroh_phone_invite(&auth, &rt, &json!({"name": "my-phone"})).expect("invite");

        // QR should be a valid PNG data-URI
        assert!(v["qr_png_base64"]
            .as_str()
            .expect("qr")
            .starts_with("data:image/png;base64,"));
        // otpauth URL for authenticator apps
        assert!(v["otpauth_url"].as_str().expect("url").starts_with("otpauth://"));

        // Payload must include endpoint_id, relay_url, totp_id, secret_base32
        let p = &v["payload"];
        assert_eq!(p["endpoint_id"], "abcdef1234567890");
        assert_eq!(p["relay_url"], "https://relay.example.com/");
        assert!(!p["totp_id"].as_str().expect("tid").is_empty());
        assert!(!p["secret_base32"].as_str().expect("secret").is_empty());
        assert_eq!(p["name"], "my-phone");
        assert!(p["created_at"].as_u64().is_some());

        // Payload JSON should be decode-able back into IrohInvitePayload
        let payload_str = serde_json::to_string(&p).expect("ser");
        let decoded: crate::IrohInvitePayload = serde_json::from_str(&payload_str).expect("deser");
        assert_eq!(decoded.endpoint_id, "abcdef1234567890");
        assert_eq!(decoded.relay_url, "https://relay.example.com/");
    }

    #[test]
    fn phone_invite_reuses_existing_totp() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        {
            let mut r = rt.lock().expect("lock");
            r.online = true;
            r.endpoint_id = "ep123".into();
            r.relay_url = "https://relay.test/".into();
        }
        // Pre-create a TOTP
        let (view, _, _) = auth.lock().expect("lock").create_totp("pre-existing").expect("totp");
        let v = iroh_phone_invite(&auth, &rt, &json!({"totp_id": view.id})).expect("invite");
        assert_eq!(v["payload"]["totp_id"], view.id);
        assert_eq!(v["payload"]["name"], "pre-existing");
        // Should still be just 1 TOTP
        assert_eq!(auth.lock().expect("lock").list_totp().len(), 1);
    }

    #[test]
    fn phone_invite_revoked_totp_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        {
            let mut r = rt.lock().expect("lock");
            r.online = true;
            r.endpoint_id = "ep".into();
            r.relay_url = "https://r/".into();
        }
        let (view, _, _) = auth.lock().expect("lock").create_totp("revoked").expect("totp");
        auth.lock().expect("lock").revoke_totp(&view.id).expect("revoke");
        let err = iroh_phone_invite(&auth, &rt, &json!({"totp_id": view.id})).expect_err("err");
        assert!(err.contains("revoked"));
    }

    #[test]
    fn phone_invite_missing_name_and_id_errors() {
        let td = tempfile::tempdir().expect("td");
        let auth = make_auth(td.path());
        let rt = make_runtime();
        {
            let mut r = rt.lock().expect("lock");
            r.online = true;
            r.endpoint_id = "ep".into();
            r.relay_url = "https://r/".into();
        }
        let err = iroh_phone_invite(&auth, &rt, &json!({})).expect_err("err");
        assert!(err.contains("totp_id") || err.contains("name"));
    }
}
