// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::unix_secs;

const IROH_AUTH_FILE: &str = "iroh_auth.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IrohTotpEntry {
    pub id: String,
    pub name: String,
    pub secret_b32: String,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
    pub last_used_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct IrohClientEntry {
    pub id: String,
    pub name: String,
    pub endpoint_id: String,
    pub totp_id: String,
    #[serde(default = "default_scope_string")]
    pub scope: String,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
    pub last_connected_at: Option<u64>,
    pub last_remote_addr: Option<String>,
    pub last_ip: Option<String>,
    pub last_country: Option<String>,
    pub last_city: Option<String>,
    pub last_locale: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct IrohAuthDb {
    pub totp: Vec<IrohTotpEntry>,
    pub clients: Vec<IrohClientEntry>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrohTotpView {
    pub id: String,
    pub name: String,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
    pub last_used_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IrohClientView {
    pub id: String,
    pub name: String,
    pub endpoint_id: String,
    pub totp_id: String,
    pub scope: String,
    pub created_at: u64,
    pub revoked_at: Option<u64>,
    pub last_connected_at: Option<u64>,
    pub last_remote_addr: Option<String>,
    pub last_ip: Option<String>,
    pub last_country: Option<String>,
    pub last_city: Option<String>,
    pub last_locale: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IrohGeo {
    pub ip: String,
    pub country: Option<String>,
    pub city: Option<String>,
    pub locale: Option<String>,
}

pub struct IrohAuthStore {
    path: PathBuf,
    db: IrohAuthDb,
}

impl IrohAuthStore {
    pub fn open(skill_dir: &Path) -> Self {
        let path = skill_dir.join(IROH_AUTH_FILE);
        let db = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<IrohAuthDb>(&s).ok())
            .unwrap_or_default();
        Self { path, db }
    }

    pub fn list_totp(&self) -> Vec<IrohTotpView> {
        self.db
            .totp
            .iter()
            .map(|t| IrohTotpView {
                id: t.id.clone(),
                name: t.name.clone(),
                created_at: t.created_at,
                revoked_at: t.revoked_at,
                last_used_at: t.last_used_at,
            })
            .collect()
    }

    pub fn list_clients(&self) -> Vec<IrohClientView> {
        self.db
            .clients
            .iter()
            .map(|c| IrohClientView {
                id: c.id.clone(),
                name: c.name.clone(),
                endpoint_id: c.endpoint_id.clone(),
                totp_id: c.totp_id.clone(),
                scope: c.scope.clone(),
                created_at: c.created_at,
                revoked_at: c.revoked_at,
                last_connected_at: c.last_connected_at,
                last_remote_addr: c.last_remote_addr.clone(),
                last_ip: c.last_ip.clone(),
                last_country: c.last_country.clone(),
                last_city: c.last_city.clone(),
                last_locale: c.last_locale.clone(),
            })
            .collect()
    }

    pub fn create_totp(&mut self, name: &str) -> Result<(IrohTotpView, String, String), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("name must not be empty".into());
        }

        let secret = Secret::generate_secret();
        let secret_b32 = match secret.to_encoded() {
            Secret::Encoded(s) => s,
            Secret::Raw(_) => return Err("failed to encode TOTP secret".into()),
        };

        let now = unix_secs();
        let id = make_id("totp");
        self.db.totp.push(IrohTotpEntry {
            id: id.clone(),
            name: name.to_owned(),
            secret_b32,
            created_at: now,
            revoked_at: None,
            last_used_at: None,
        });
        self.save()?;

        let (otpauth_url, qr_png_base64) = self.totp_qr_inner(&id)?;
        let view = self
            .list_totp()
            .into_iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "internal error: created TOTP not found".to_string())?;
        Ok((view, otpauth_url, qr_png_base64))
    }

    pub fn revoke_totp(&mut self, id: &str) -> Result<(), String> {
        let now = unix_secs();
        let Some(t) = self.db.totp.iter_mut().find(|t| t.id == id) else {
            return Err(format!("unknown totp id: {id}"));
        };
        if t.revoked_at.is_none() {
            t.revoked_at = Some(now);
            // Cascade revoke: revoke all clients linked to this TOTP
            for client in self.db.clients.iter_mut() {
                if client.totp_id == id && client.revoked_at.is_none() {
                    client.revoked_at = Some(now);
                }
            }
            self.save()?;
        }
        Ok(())
    }

    pub fn revoke_client(&mut self, id: &str) -> Result<(), String> {
        let now = unix_secs();
        let Some(c) = self.db.clients.iter_mut().find(|c| c.id == id) else {
            return Err(format!("unknown client id: {id}"));
        };
        if c.revoked_at.is_none() {
            c.revoked_at = Some(now);
            self.save()?;
        }
        Ok(())
    }

    pub fn register_client(
        &mut self,
        endpoint_id: &str,
        otp: &str,
        totp_id_hint: Option<&str>,
        name_hint: Option<&str>,
        scope_hint: Option<&str>,
    ) -> Result<IrohClientView, String> {
        let endpoint_id = endpoint_id.trim().to_lowercase();
        if endpoint_id.is_empty() {
            return Err("endpoint_id must not be empty".into());
        }
        let otp = otp.trim();
        if otp.is_empty() {
            return Err("otp must not be empty".into());
        }

        let matched_totp_id = self.verify_otp(otp, totp_id_hint)?;
        let now = unix_secs();

        // Enforce only one active client per TOTP
        if self.db.clients.iter().any(|c| c.totp_id == matched_totp_id && c.revoked_at.is_none()) {
            return Err("Only one active client is allowed per TOTP. Revoke the existing client before registering a new one.".into());
        }

        if let Some(idx) = self
            .db
            .clients
            .iter()
            .position(|c| c.endpoint_id == endpoint_id && c.revoked_at.is_none())
        {
            // Optionally, update the existing client (if endpoint_id matches)
            let existing = &mut self.db.clients[idx];
            if let Some(name) = name_hint {
                let n = name.trim();
                if !n.is_empty() {
                    existing.name = n.to_owned();
                }
            }
            existing.totp_id = matched_totp_id.clone();
            if let Some(scope) = scope_hint {
                existing.scope = normalize_scope(scope)?;
            }
            self.save()?;
            let existing = &self.db.clients[idx];
            return Ok(IrohClientView {
                id: existing.id.clone(),
                name: existing.name.clone(),
                endpoint_id: existing.endpoint_id.clone(),
                totp_id: existing.totp_id.clone(),
                scope: existing.scope.clone(),
                created_at: existing.created_at,
                revoked_at: existing.revoked_at,
                last_connected_at: existing.last_connected_at,
                last_remote_addr: existing.last_remote_addr.clone(),
                last_ip: existing.last_ip.clone(),
                last_country: existing.last_country.clone(),
                last_city: existing.last_city.clone(),
                last_locale: existing.last_locale.clone(),
            });
        }

        let name = name_hint
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("client-{}", &endpoint_id.chars().take(8).collect::<String>()));

        let id = make_id("client");
        let scope = match scope_hint {
            Some(s) => normalize_scope(s)?,
            None => default_scope().to_string(),
        };
        let view = IrohClientView {
            id: id.clone(),
            name: name.clone(),
            endpoint_id: endpoint_id.clone(),
            totp_id: matched_totp_id.clone(),
            scope: scope.clone(),
            created_at: now,
            revoked_at: None,
            last_connected_at: None,
            last_remote_addr: None,
            last_ip: None,
            last_country: None,
            last_city: None,
            last_locale: None,
        };

        self.db.clients.push(IrohClientEntry {
            id,
            name,
            endpoint_id,
            totp_id: matched_totp_id,
            scope,
            created_at: now,
            revoked_at: None,
            last_connected_at: None,
            last_remote_addr: None,
            last_ip: None,
            last_country: None,
            last_city: None,
            last_locale: None,
        });
        self.save()?;
        Ok(view)
    }

    pub fn mark_client_connected(
        &mut self,
        endpoint_id: &str,
        remote_addr: &str,
        geo: Option<IrohGeo>,
    ) -> Result<(), String> {
        let endpoint_id = endpoint_id.trim().to_lowercase();
        let Some(c) = self
            .db
            .clients
            .iter_mut()
            .find(|c| c.endpoint_id == endpoint_id && c.revoked_at.is_none())
        else {
            return Err(format!("unknown or revoked endpoint_id: {endpoint_id}"));
        };

        c.last_connected_at = Some(unix_secs());
        c.last_remote_addr = Some(remote_addr.to_owned());
        if let Some(geo) = geo {
            c.last_ip = Some(geo.ip);
            c.last_country = geo.country;
            c.last_city = geo.city;
            c.last_locale = geo.locale;
        }
        self.save()
    }

    pub fn totp_qr(&self, id: &str) -> Result<(String, String), String> {
        self.totp_qr_inner(id)
    }

    pub fn is_endpoint_allowed(&self, endpoint_id: &str) -> bool {
        let endpoint_id = endpoint_id.trim().to_lowercase();
        self.db
            .clients
            .iter()
            .any(|c| c.revoked_at.is_none() && c.endpoint_id == endpoint_id)
    }

    pub fn scope_for_endpoint(&self, endpoint_id: &str) -> Option<String> {
        let endpoint_id = endpoint_id.trim().to_lowercase();
        self.db
            .clients
            .iter()
            .find(|c| c.revoked_at.is_none() && c.endpoint_id == endpoint_id)
            .map(|c| c.scope.clone())
    }

    pub fn set_client_scope(&mut self, id: &str, scope: &str) -> Result<(), String> {
        let scope = normalize_scope(scope)?;
        let Some(c) = self.db.clients.iter_mut().find(|c| c.id == id) else {
            return Err(format!("unknown client id: {id}"));
        };
        c.scope = scope;
        self.save()
    }

    fn totp_qr_inner(&self, id: &str) -> Result<(String, String), String> {
        let totp_entry = self
            .db
            .totp
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("unknown totp id: {id}"))?;
        let t = totp_from_entry(totp_entry)?;
        let otpauth_url = t.get_url();
        let qr_b64 = t.get_qr_base64().map_err(|e| format!("failed to build QR: {e}"))?;
        Ok((otpauth_url, format!("data:image/png;base64,{qr_b64}")))
    }

    fn verify_otp(&mut self, otp: &str, totp_id_hint: Option<&str>) -> Result<String, String> {
        if let Some(id) = totp_id_hint {
            let Some(idx) = self.db.totp.iter().position(|t| t.id == id) else {
                return Err(format!("unknown totp id: {id}"));
            };
            {
                let t = &self.db.totp[idx];
                if t.revoked_at.is_some() {
                    return Err("that TOTP credential is revoked".into());
                }
                let ok = totp_from_entry(t).and_then(|totp| {
                    totp.check_current(otp)
                        .map_err(|e| format!("failed to verify otp: {e}"))
                })?;
                if !ok {
                    return Err("invalid otp".into());
                }
            }
            self.db.totp[idx].last_used_at = Some(unix_secs());
            self.save()?;
            return Ok(self.db.totp[idx].id.clone());
        }

        let mut matched: Vec<String> = Vec::new();
        for t in self.db.totp.iter_mut().filter(|t| t.revoked_at.is_none()) {
            let ok = totp_from_entry(t).and_then(|totp| {
                totp.check_current(otp)
                    .map_err(|e| format!("failed to verify otp: {e}"))
            })?;
            if ok {
                matched.push(t.id.clone());
            }
        }

        match matched.len() {
            0 => Err("invalid otp".into()),
            1 => {
                let id = matched.remove(0);
                if let Some(t) = self.db.totp.iter_mut().find(|t| t.id == id) {
                    t.last_used_at = Some(unix_secs());
                    self.save()?;
                }
                Ok(id)
            }
            _ => Err("otp matched multiple active TOTP credentials; pass `totp_id` explicitly".to_string()),
        }
    }

    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let s = serde_json::to_string_pretty(&self.db).map_err(|e| format!("serialize auth db failed: {e}"))?;
        std::fs::write(&self.path, s).map_err(|e| format!("write {} failed: {e}", self.path.display()))
    }
}

pub fn default_scope() -> &'static str {
    "read"
}

pub fn default_scope_string() -> String {
    default_scope().to_string()
}

pub fn normalize_scope(scope: &str) -> Result<String, String> {
    let s = scope.trim().to_lowercase();
    match s.as_str() {
        "read" | "readonly" => Ok("read".to_string()),
        "full" => Ok("full".to_string()),
        _ => Err(format!("invalid scope '{scope}': expected 'read' or 'full'")),
    }
}

fn make_id(prefix: &str) -> String {
    let now = unix_secs();
    let r: u64 = rand::random();
    format!("{prefix}_{now}_{r:016x}")
}

fn totp_from_entry(e: &IrohTotpEntry) -> Result<TOTP, String> {
    let secret = Secret::Encoded(e.secret_b32.clone())
        .to_bytes()
        .map_err(|e| format!("invalid TOTP secret: {e}"))?;
    TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret,
        Some("Skill".to_string()),
        e.name.clone(),
    )
    .map_err(|e| format!("failed to build TOTP: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store() -> (tempfile::TempDir, IrohAuthStore) {
        let td = tempfile::tempdir().expect("tempdir");
        let s = IrohAuthStore::open(td.path());
        (td, s)
    }

    #[test]
    fn scope_normalization() {
        assert_eq!(normalize_scope("read").expect("read"), "read");
        assert_eq!(normalize_scope("readonly").expect("readonly"), "read");
        assert_eq!(normalize_scope("full").expect("full"), "full");
        assert!(normalize_scope("admin").is_err());
    }

    #[test]
    fn register_defaults_to_read_scope() {
        let (_td, mut s) = tmp_store();
        let (_t, _url, _qr) = s.create_totp("phone").expect("create totp");
        let tentry = s.db.totp.first().expect("totp exists").clone();
        let otp = totp_from_entry(&tentry)
            .expect("build totp")
            .generate_current()
            .expect("otp");

        let c = s
            .register_client("abc123", &otp, Some(&tentry.id), Some("device"), None)
            .expect("register");
        assert_eq!(c.scope, "read");
        assert_eq!(s.scope_for_endpoint("abc123").as_deref(), Some("read"));
    }

    #[test]
    fn register_can_set_full_scope_and_revoke() {
        let (_td, mut s) = tmp_store();
        let (_t, _url, _qr) = s.create_totp("phone").expect("create totp");
        let tentry = s.db.totp.first().expect("totp exists").clone();
        let otp = totp_from_entry(&tentry)
            .expect("build totp")
            .generate_current()
            .expect("otp");

        let c = s
            .register_client("def456", &otp, Some(&tentry.id), Some("admin device"), Some("full"))
            .expect("register");
        assert_eq!(c.scope, "full");
        s.revoke_client(&c.id).expect("revoke client");
        assert!(!s.is_endpoint_allowed("def456"));
    }
}
