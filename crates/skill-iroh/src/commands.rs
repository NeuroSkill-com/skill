use rand::RngCore;
use base64::{engine::general_purpose, Engine as _};
use qrcodegen::{QrCode, QrCodeEcc};
use image::ColorType;
use image::codecs::png::PngEncoder;
/// Generate a single-use onboarding payload and QR code for phone invites
pub fn iroh_phone_invite(_auth: &SharedIrohAuth, _msg: &Value) -> Result<Value, String> {
    // Generate a random one-time token
    let mut token_bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut token_bytes);
    let token = general_purpose::URL_SAFE_NO_PAD.encode(&token_bytes);

    // Compose onboarding payload (customize as needed)
    let payload = json!({
        "invite_token": token,
        "created_at": crate::unix_secs(),
        // Add more onboarding fields as needed
    });

    // Generate QR code PNG (base64)
    let qr = QrCode::encode_text(&payload_str, QrCodeEcc::Medium)
        .map_err(|e| format!("QR encode error: {e}"))?;
    // Render QR code to PNG using qrcodegen and image
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
    let mut encoder = PngEncoder::new(&mut buf);
    encoder.encode(&img, img_size, img_size, ColorType::L8)
        .map_err(|e| format!("QR PNG encode error: {e}"))?;
    let qr_png_base64 = general_purpose::STANDARD.encode(&buf);

    // TODO: Store the token as valid and mark as used after first use (not implemented here)

    Ok(json!({
        "payload": payload,
        "qr_png_base64": format!("data:image/png;base64,{}", qr_png_base64),
    }))
}
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::{json, Value};

            encoder!.encode(&img, img_size, img_size, ColorType::L8)

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
        .ok_or_else(|| "missing required field: \"scope\" (string: read|full)".to_string())?;

    lock_or_recover(auth).set_client_scope(id, scope)?;
    Ok(json!({
        "ok": true,
        "id": id,
        "scope": scope,
        "warning": if scope.eq_ignore_ascii_case("full") {
            "FULL ACCESS grants complete API control over this device."
        } else {
            ""
        }
    }))
}
