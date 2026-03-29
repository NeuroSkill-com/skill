// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use totp_rs::{Algorithm, Secret, TOTP};

use crate::scope::ClientScope;
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
    /// Legacy field — kept for backward-compat deserialization.
    /// New code should read `permissions.scope` instead.
    #[serde(default = "default_scope_string")]
    pub scope: String,
    /// Granular permission overrides.
    #[serde(default)]
    pub permissions: ClientScope,
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
    pub permissions: ClientScope,
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

/// Everything a phone needs to connect and register with the Skill iroh server.
/// Serialised as JSON and encoded into a single QR code.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IrohInvitePayload {
    pub endpoint_id: String,
    pub relay_url: String,
    pub totp_id: String,
    pub secret_base32: String,
    pub name: String,
    pub created_at: u64,
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
        self.db.clients.iter().map(client_view).collect()
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

        // Check if this endpoint is already registered (update path)
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
                let s = normalize_scope(scope)?;
                existing.scope = s.clone();
                existing.permissions.scope = s;
            }
            self.save()?;
            let existing = &self.db.clients[idx];
            return Ok(client_view(existing));
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
        let permissions = match scope.as_str() {
            "full" => ClientScope::full(),
            _ => ClientScope::read(),
        };

        self.db.clients.push(IrohClientEntry {
            id,
            name,
            endpoint_id,
            totp_id: matched_totp_id,
            scope,
            permissions,
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
        let view = client_view(self.db.clients.last().unwrap());
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

    /// Build the combined invite payload for a given TOTP.
    /// The caller supplies the current endpoint_id and relay_url from the running
    /// iroh tunnel.  The payload contains everything a phone client needs to
    /// connect *and* authenticate in one QR scan.
    pub fn build_invite_payload(
        &self,
        totp_id: &str,
        endpoint_id: &str,
        relay_url: &str,
    ) -> Result<IrohInvitePayload, String> {
        let t = self
            .db
            .totp
            .iter()
            .find(|t| t.id == totp_id)
            .ok_or_else(|| format!("unknown totp id: {totp_id}"))?;
        if t.revoked_at.is_some() {
            return Err("that TOTP credential is revoked".into());
        }
        if endpoint_id.trim().is_empty() {
            return Err("endpoint_id must not be empty (is the iroh tunnel running?)".into());
        }
        if relay_url.trim().is_empty() {
            return Err("relay_url must not be empty (is the iroh tunnel running?)".into());
        }
        Ok(IrohInvitePayload {
            endpoint_id: endpoint_id.to_owned(),
            relay_url: relay_url.to_owned(),
            totp_id: t.id.clone(),
            secret_base32: t.secret_b32.clone(),
            name: t.name.clone(),
            created_at: unix_secs(),
        })
    }

    /// Return the display name of the client registered for `endpoint_id`,
    /// or `None` if no active (non-revoked) client matches.
    pub fn client_name_for_endpoint(&self, endpoint_id: &str) -> Option<String> {
        let endpoint_id = endpoint_id.trim().to_lowercase();
        self.db
            .clients
            .iter()
            .find(|c| c.revoked_at.is_none() && c.endpoint_id == endpoint_id)
            .map(|c| c.name.clone())
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
            .map(|c| effective_scope(c).scope)
    }

    /// Return the full [`ClientScope`] for an endpoint.
    pub fn client_scope_for_endpoint(&self, endpoint_id: &str) -> Option<ClientScope> {
        let endpoint_id = endpoint_id.trim().to_lowercase();
        self.db
            .clients
            .iter()
            .find(|c| c.revoked_at.is_none() && c.endpoint_id == endpoint_id)
            .map(effective_scope)
    }

    /// Check whether a specific command is allowed for an endpoint.
    pub fn is_command_allowed(&self, endpoint_id: &str, command: &str) -> bool {
        let Some(cs) = self.client_scope_for_endpoint(endpoint_id) else {
            return false;
        };
        crate::scope::is_allowed(&cs, command)
    }

    /// Set the scope for a client (simple read/full/custom).
    pub fn set_client_scope(&mut self, id: &str, scope: &str) -> Result<(), String> {
        let scope = normalize_scope(scope)?;
        let Some(c) = self.db.clients.iter_mut().find(|c| c.id == id) else {
            return Err(format!("unknown client id: {id}"));
        };
        c.scope = scope.clone();
        c.permissions.scope = scope;
        self.save()
    }

    /// Set granular permissions for a client.
    pub fn set_client_permissions(
        &mut self,
        id: &str,
        scope: &str,
        groups: Option<Vec<String>>,
        allow: Option<Vec<String>>,
        deny: Option<Vec<String>>,
    ) -> Result<ClientScope, String> {
        let scope = normalize_scope(scope)?;

        if let Some(ref gs) = groups {
            crate::scope::validate_groups(gs)?;
        }
        if let Some(ref cmds) = allow {
            crate::scope::validate_commands(cmds)?;
        }
        if let Some(ref cmds) = deny {
            crate::scope::validate_commands(cmds)?;
        }

        let Some(c) = self.db.clients.iter_mut().find(|c| c.id == id) else {
            return Err(format!("unknown client id: {id}"));
        };

        c.scope = scope.clone();
        c.permissions.scope = scope;
        if let Some(gs) = groups {
            c.permissions.groups = gs;
        }
        if let Some(cmds) = allow {
            c.permissions.allow = cmds;
        }
        if let Some(cmds) = deny {
            c.permissions.deny = cmds;
        }
        let result = c.permissions.clone();
        self.save()?;
        Ok(result)
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

/// Re-export: use [`crate::scope::normalize_scope`] for new code.
pub fn normalize_scope(scope: &str) -> Result<String, String> {
    crate::scope::normalize_scope(scope)
}

/// Build a [`ClientScope`] from the legacy `scope` string on an entry.
/// If the entry already has a populated `permissions` struct, use that;
/// otherwise fall back to the legacy string.
fn effective_scope(c: &IrohClientEntry) -> ClientScope {
    // If permissions.scope is populated, use it directly
    if !c.permissions.scope.is_empty() {
        return c.permissions.clone();
    }
    // Legacy migration: translate old "read"/"full" string
    match c.scope.as_str() {
        "full" => ClientScope::full(),
        _ => ClientScope::read(),
    }
}

/// Build an [`IrohClientView`] from an [`IrohClientEntry`].
fn client_view(c: &IrohClientEntry) -> IrohClientView {
    let perms = effective_scope(c);
    IrohClientView {
        id: c.id.clone(),
        name: c.name.clone(),
        endpoint_id: c.endpoint_id.clone(),
        totp_id: c.totp_id.clone(),
        scope: perms.scope.clone(),
        permissions: perms,
        created_at: c.created_at,
        revoked_at: c.revoked_at,
        last_connected_at: c.last_connected_at,
        last_remote_addr: c.last_remote_addr.clone(),
        last_ip: c.last_ip.clone(),
        last_country: c.last_country.clone(),
        last_city: c.last_city.clone(),
        last_locale: c.last_locale.clone(),
    }
}

fn make_id(prefix: &str) -> String {
    let now = unix_secs();
    let r: u64 = rand::random();
    format!("{prefix}_{now}_{r:016x}")
}

pub fn totp_from_entry(e: &IrohTotpEntry) -> Result<TOTP, String> {
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

    // ── TOTP lifecycle tests ─────────────────────────────────────────────

    #[test]
    fn create_totp_returns_valid_otpauth_url() {
        let (_td, mut s) = tmp_store();
        let (view, otpauth_url, qr) = s.create_totp("my-phone").expect("create");
        assert!(!view.id.is_empty());
        assert_eq!(view.name, "my-phone");
        assert!(view.revoked_at.is_none());
        assert!(otpauth_url.starts_with("otpauth://totp/"));
        assert!(otpauth_url.contains("secret="));
        assert!(qr.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn create_totp_empty_name_errors() {
        let (_td, mut s) = tmp_store();
        assert!(s.create_totp("").is_err());
        assert!(s.create_totp("   ").is_err());
    }

    #[test]
    fn list_totp_returns_all_entries() {
        let (_td, mut s) = tmp_store();
        s.create_totp("a").expect("a");
        s.create_totp("b").expect("b");
        let list = s.list_totp();
        assert_eq!(list.len(), 2);
        let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
    }

    #[test]
    fn revoke_totp_marks_revoked() {
        let (_td, mut s) = tmp_store();
        let (view, _, _) = s.create_totp("phone").expect("create");
        assert!(view.revoked_at.is_none());
        s.revoke_totp(&view.id).expect("revoke");
        let list = s.list_totp();
        let t = list.iter().find(|t| t.id == view.id).expect("find");
        assert!(t.revoked_at.is_some());
    }

    #[test]
    fn revoke_totp_cascades_to_clients() {
        let (_td, mut s) = tmp_store();
        let (tview, _, _) = s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let _c = s
            .register_client("ep1", &otp, Some(&tview.id), Some("dev"), None)
            .expect("reg");
        assert!(s.is_endpoint_allowed("ep1"));

        s.revoke_totp(&tview.id).expect("revoke totp");
        assert!(!s.is_endpoint_allowed("ep1"));
        let clients = s.list_clients();
        assert!(clients.iter().all(|c| c.revoked_at.is_some()));
    }

    #[test]
    fn revoke_nonexistent_totp_errors() {
        let (_td, mut s) = tmp_store();
        assert!(s.revoke_totp("fake_id").is_err());
    }

    #[test]
    fn totp_qr_returns_otpauth_and_png() {
        let (_td, mut s) = tmp_store();
        let (view, _, _) = s.create_totp("phone").expect("create");
        let (url, qr) = s.totp_qr(&view.id).expect("qr");
        assert!(url.starts_with("otpauth://"));
        assert!(qr.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn totp_qr_unknown_id_errors() {
        let (_td, s) = tmp_store();
        assert!(s.totp_qr("nonexistent").is_err());
    }

    // ── OTP verification tests ───────────────────────────────────────────

    #[test]
    fn verify_otp_valid_code_without_hint() {
        let (_td, mut s) = tmp_store();
        let (tview, _, _) = s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c = s.register_client("ep99", &otp, None, Some("dev"), None).expect("reg");
        assert_eq!(c.totp_id, tview.id);
    }

    #[test]
    fn verify_otp_invalid_code_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let result = s.register_client("ep1", "000000", None, None, None);
        assert!(result.is_err());
        assert!(result.expect_err("err").contains("invalid otp"));
    }

    #[test]
    fn verify_otp_revoked_totp_with_hint_errors() {
        let (_td, mut s) = tmp_store();
        let (tview, _, _) = s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        s.revoke_totp(&tview.id).expect("revoke");
        let result = s.register_client("ep1", &otp, Some(&tview.id), None, None);
        assert!(result.is_err());
        assert!(result.expect_err("err").contains("revoked"));
    }

    #[test]
    fn verify_otp_ambiguous_without_hint_errors() {
        let (_td, mut s) = tmp_store();
        let (t1, _, _) = s.create_totp("p1").expect("create p1");
        let (_t2, _, _) = s.create_totp("p2").expect("create p2");

        // Force both TOTPs to share the same secret
        let s0 = s.db.totp[0].secret_b32.clone();
        s.db.totp[1].secret_b32 = s0.clone();
        s.save().expect("save");

        let otp = totp_from_entry(&s.db.totp[0])
            .expect("totp")
            .generate_current()
            .expect("otp");
        let result = s.register_client("ep1", &otp, None, None, None);
        assert!(result.is_err());
        assert!(result.expect_err("err").contains("multiple"));

        // With hint -> should succeed
        let otp2 = totp_from_entry(&s.db.totp[0])
            .expect("totp")
            .generate_current()
            .expect("otp");
        let c = s
            .register_client("ep1", &otp2, Some(&t1.id), None, None)
            .expect("reg with hint");
        assert_eq!(c.totp_id, t1.id);
    }

    #[test]
    fn verify_otp_unknown_hint_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let result = s.register_client("ep1", &otp, Some("fake_totp_id"), None, None);
        assert!(result.is_err());
    }

    // ── Client registration tests ────────────────────────────────────────

    #[test]
    fn register_client_empty_endpoint_id_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        assert!(s.register_client("", "123456", None, None, None).is_err());
        assert!(s.register_client("  ", "123456", None, None, None).is_err());
    }

    #[test]
    fn register_client_empty_otp_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        assert!(s.register_client("ep1", "", None, None, None).is_err());
        assert!(s.register_client("ep1", "  ", None, None, None).is_err());
    }

    #[test]
    fn register_client_generates_default_name() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c = s
            .register_client("abcdef1234567890", &otp, Some(&tentry.id), None, None)
            .expect("reg");
        assert!(c.name.starts_with("client-"));
    }

    #[test]
    fn register_client_invalid_scope_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let result = s.register_client("ep1", &otp, Some(&tentry.id), None, Some("admin"));
        assert!(result.is_err());
        assert!(result.expect_err("err").contains("invalid scope"));
    }

    #[test]
    fn register_multiple_clients_per_totp_allowed() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();

        let otp1 = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        s.register_client("ep1", &otp1, Some(&tentry.id), Some("first"), None)
            .expect("first reg");

        let otp2 = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c2 = s
            .register_client("ep2", &otp2, Some(&tentry.id), Some("second"), None)
            .expect("second reg");
        assert_eq!(c2.name, "second");
        assert_eq!(s.list_clients().len(), 2);
    }

    #[test]
    fn register_reuse_endpoint_updates_existing() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();

        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c1 = s
            .register_client("ep1", &otp, Some(&tentry.id), Some("old-name"), Some("read"))
            .expect("first");
        assert_eq!(c1.name, "old-name");
        assert_eq!(c1.scope, "read");

        // Same endpoint_id with new name/scope should update in-place
        let otp2 = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c2 = s
            .register_client("ep1", &otp2, Some(&tentry.id), Some("new-name"), Some("full"))
            .expect("update");
        assert_eq!(c2.id, c1.id); // same client entry
        assert_eq!(c2.name, "new-name");
        assert_eq!(c2.scope, "full");

        assert_eq!(s.list_clients().len(), 1);
    }

    // ── Mark connected / geo tests ───────────────────────────────────────

    #[test]
    fn mark_client_connected_updates_fields() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        s.register_client("ep1", &otp, Some(&tentry.id), Some("dev"), None)
            .expect("reg");

        s.mark_client_connected(
            "ep1",
            "1.2.3.4:5678",
            Some(IrohGeo {
                ip: "1.2.3.4".into(),
                country: Some("Germany".into()),
                city: Some("Berlin".into()),
                locale: Some("en_US".into()),
            }),
        )
        .expect("mark");

        let clients = s.list_clients();
        let c = &clients[0];
        assert!(c.last_connected_at.is_some());
        assert_eq!(c.last_remote_addr.as_deref(), Some("1.2.3.4:5678"));
        assert_eq!(c.last_ip.as_deref(), Some("1.2.3.4"));
        assert_eq!(c.last_country.as_deref(), Some("Germany"));
        assert_eq!(c.last_city.as_deref(), Some("Berlin"));
        assert_eq!(c.last_locale.as_deref(), Some("en_US"));
    }

    #[test]
    fn mark_client_connected_without_geo() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        s.register_client("ep1", &otp, Some(&tentry.id), Some("dev"), None)
            .expect("reg");

        s.mark_client_connected("ep1", "relay://xyz", None).expect("mark");
        let clients = s.list_clients();
        assert!(clients[0].last_connected_at.is_some());
        assert_eq!(clients[0].last_remote_addr.as_deref(), Some("relay://xyz"));
        assert!(clients[0].last_ip.is_none());
    }

    #[test]
    fn mark_connected_revoked_client_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c = s
            .register_client("ep1", &otp, Some(&tentry.id), None, None)
            .expect("reg");
        s.revoke_client(&c.id).expect("revoke");
        assert!(s.mark_client_connected("ep1", "addr", None).is_err());
    }

    #[test]
    fn mark_connected_unknown_endpoint_errors() {
        let (_td, mut s) = tmp_store();
        assert!(s.mark_client_connected("nonexistent", "addr", None).is_err());
    }

    // ── Scope tests ──────────────────────────────────────────────────────

    #[test]
    fn set_client_scope_works() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c = s
            .register_client("ep1", &otp, Some(&tentry.id), None, Some("read"))
            .expect("reg");

        s.set_client_scope(&c.id, "full").expect("set full");
        assert_eq!(s.scope_for_endpoint("ep1").as_deref(), Some("full"));

        s.set_client_scope(&c.id, "readonly").expect("set readonly");
        assert_eq!(s.scope_for_endpoint("ep1").as_deref(), Some("read"));
    }

    #[test]
    fn set_client_scope_invalid_errors() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c = s
            .register_client("ep1", &otp, Some(&tentry.id), None, None)
            .expect("reg");
        assert!(s.set_client_scope(&c.id, "admin").is_err());
    }

    #[test]
    fn set_client_scope_unknown_id_errors() {
        let (_td, mut s) = tmp_store();
        assert!(s.set_client_scope("fake_id", "read").is_err());
    }

    // ── is_endpoint_allowed / scope_for_endpoint ─────────────────────────

    #[test]
    fn is_endpoint_allowed_case_insensitive() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        s.register_client("ABCDEF", &otp, Some(&tentry.id), None, None)
            .expect("reg");
        assert!(s.is_endpoint_allowed("abcdef"));
        assert!(s.is_endpoint_allowed("ABCDEF"));
        assert!(s.is_endpoint_allowed(" abcdef "));
    }

    #[test]
    fn scope_for_endpoint_none_when_missing() {
        let (_td, s) = tmp_store();
        assert!(s.scope_for_endpoint("nonexistent").is_none());
    }

    // ── Revoke client tests ──────────────────────────────────────────────

    #[test]
    fn revoke_client_marks_revoked() {
        let (_td, mut s) = tmp_store();
        s.create_totp("phone").expect("create");
        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        let c = s
            .register_client("ep1", &otp, Some(&tentry.id), None, None)
            .expect("reg");
        assert!(s.is_endpoint_allowed("ep1"));
        s.revoke_client(&c.id).expect("revoke");
        assert!(!s.is_endpoint_allowed("ep1"));
        // Double revoke is idempotent
        s.revoke_client(&c.id).expect("revoke again");
    }

    #[test]
    fn revoke_client_unknown_id_errors() {
        let (_td, mut s) = tmp_store();
        assert!(s.revoke_client("fake").is_err());
    }

    // ── Persistence tests ────────────────────────────────────────────────

    #[test]
    fn store_persists_and_reloads() {
        let td = tempfile::tempdir().expect("tempdir");
        {
            let mut s = IrohAuthStore::open(td.path());
            s.create_totp("phone").expect("create");
            let tentry = s.db.totp.first().expect("exists").clone();
            let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
            s.register_client("ep1", &otp, Some(&tentry.id), Some("dev"), Some("full"))
                .expect("reg");
        }

        // Reopen from disk
        let s2 = IrohAuthStore::open(td.path());
        assert_eq!(s2.list_totp().len(), 1);
        assert_eq!(s2.list_clients().len(), 1);
        let c = &s2.list_clients()[0];
        assert_eq!(c.name, "dev");
        assert_eq!(c.scope, "full");
        assert_eq!(c.endpoint_id, "ep1");
        assert!(s2.is_endpoint_allowed("ep1"));
    }

    #[test]
    fn store_handles_missing_file() {
        let td = tempfile::tempdir().expect("tempdir");
        let s = IrohAuthStore::open(td.path());
        assert!(s.list_totp().is_empty());
        assert!(s.list_clients().is_empty());
    }

    // ── build_invite_payload tests ──────────────────────────────────────

    #[test]
    fn build_invite_payload_includes_all_fields() {
        let (_td, mut s) = tmp_store();
        let (view, _, _) = s.create_totp("phone").expect("create");
        let invite = s
            .build_invite_payload(&view.id, "endpoint123", "https://relay.example.com/")
            .expect("invite");
        assert_eq!(invite.endpoint_id, "endpoint123");
        assert_eq!(invite.relay_url, "https://relay.example.com/");
        assert_eq!(invite.totp_id, view.id);
        assert!(!invite.secret_base32.is_empty());
        assert_eq!(invite.name, "phone");
        assert!(invite.created_at > 0);

        // Should round-trip through JSON
        let json = serde_json::to_string(&invite).expect("ser");
        let decoded: super::IrohInvitePayload = serde_json::from_str(&json).expect("deser");
        assert_eq!(decoded.endpoint_id, "endpoint123");
        assert_eq!(decoded.secret_base32, invite.secret_base32);
    }

    #[test]
    fn build_invite_payload_revoked_totp_errors() {
        let (_td, mut s) = tmp_store();
        let (view, _, _) = s.create_totp("phone").expect("create");
        s.revoke_totp(&view.id).expect("revoke");
        let err = s.build_invite_payload(&view.id, "ep", "relay").expect_err("err");
        assert!(err.contains("revoked"));
    }

    #[test]
    fn build_invite_payload_unknown_totp_errors() {
        let (_td, s) = tmp_store();
        assert!(s.build_invite_payload("fake", "ep", "relay").is_err());
    }

    #[test]
    fn build_invite_payload_empty_endpoint_errors() {
        let (_td, mut s) = tmp_store();
        let (view, _, _) = s.create_totp("phone").expect("create");
        assert!(s.build_invite_payload(&view.id, "", "relay").is_err());
        assert!(s.build_invite_payload(&view.id, "  ", "relay").is_err());
    }

    #[test]
    fn build_invite_payload_empty_relay_errors() {
        let (_td, mut s) = tmp_store();
        let (view, _, _) = s.create_totp("phone").expect("create");
        assert!(s.build_invite_payload(&view.id, "ep", "").is_err());
    }

    // ── last_used_at tracking ────────────────────────────────────────────

    #[test]
    fn otp_verification_updates_last_used_at() {
        let (_td, mut s) = tmp_store();
        let (tview, _, _) = s.create_totp("phone").expect("create");
        assert!(s
            .list_totp()
            .iter()
            .find(|t| t.id == tview.id)
            .expect("find")
            .last_used_at
            .is_none());

        let tentry = s.db.totp.first().expect("exists").clone();
        let otp = totp_from_entry(&tentry).expect("totp").generate_current().expect("otp");
        s.register_client("ep1", &otp, Some(&tview.id), None, None)
            .expect("reg");

        let totp_view = s.list_totp().into_iter().find(|t| t.id == tview.id).expect("find");
        assert!(totp_view.last_used_at.is_some());
    }
}
