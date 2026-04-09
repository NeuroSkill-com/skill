// SPDX-License-Identifier: GPL-3.0-only

use anyhow::Context as _;
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use iroh::{
    endpoint::{presets, Connection, IncomingAddr},
    Endpoint, RelayMode, SecretKey,
};
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::{auth::IrohAuthStore, auth::IrohGeo, lock_or_recover, unix_secs};

const IROH_API_ALPN: &[u8] = b"skill/http-ws/1";
const IROH_DEVICE_PROXY_ALPN: &[u8] = b"skill/device-proxy/2";
const IROH_SECRET_FILE: &str = "iroh_secret_key.bin";

pub type SharedIrohAuth = Arc<Mutex<IrohAuthStore>>;
pub type SharedIrohRuntime = Arc<Mutex<IrohRuntimeState>>;

/// Maps `local_tcp_source_port → iroh_peer_endpoint_id`.
///
/// Populated by the tunnel just before connecting to the local API server,
/// read by axum middleware to identify the iroh peer.  Entries are removed
/// when the TCP connection closes.
pub type IrohPeerMap = Arc<Mutex<HashMap<u16, String>>>;

/// Create a new empty peer map.
pub fn new_peer_map() -> IrohPeerMap {
    Arc::new(Mutex::new(HashMap::new()))
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct IrohRuntimeState {
    pub endpoint_id: String,
    pub relay_url: String,
    pub direct_addrs: Vec<String>,
    pub local_port: u16,
    pub started_at: u64,
    pub online: bool,
    pub last_error: Option<String>,
}

/// Shared, swappable device event sender.
///
/// Wrapped in `Arc<Mutex<>>` so the tx can be replaced after a session ends
/// (the old rx is consumed by the session adapter; a fresh tx/rx pair is
/// created and the tunnel picks up the new tx on the next connection).
pub type SharedDeviceEventTx = Arc<Mutex<Option<crate::device_receiver::RemoteEventTx>>>;

pub fn spawn(
    skill_dir: PathBuf,
    api_port: u16,
    auth: SharedIrohAuth,
    runtime: SharedIrohRuntime,
    peer_map: IrohPeerMap,
    device_event_tx: SharedDeviceEventTx,
) {
    // Run the iroh tunnel on its own dedicated Tokio runtime in a background thread.
    // Calling tokio::spawn here would panic if no Tokio reactor is running in the
    // caller's context (e.g. Tauri's async runtime). Spawning a thread with a
    // fresh runtime avoids that dependency.
    std::thread::Builder::new()
        .name("iroh-tunnel".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("failed to create tokio runtime for iroh tunnel");

            if let Err(e) = rt.block_on(run(
                &skill_dir,
                api_port,
                auth,
                runtime.clone(),
                peer_map,
                device_event_tx.clone(),
            )) {
                lock_or_recover(&runtime).last_error = Some(e.to_string());
                eprintln!("[iroh] tunnel stopped: {e}");
            }
        })
        .expect("failed to spawn iroh tunnel thread");
}

async fn run(
    skill_dir: &Path,
    api_port: u16,
    auth: SharedIrohAuth,
    runtime: SharedIrohRuntime,
    peer_map: IrohPeerMap,
    device_event_tx: SharedDeviceEventTx,
) -> anyhow::Result<()> {
    let secret_key = load_or_create_secret_key(skill_dir)?;

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![IROH_API_ALPN.to_vec(), IROH_DEVICE_PROXY_ALPN.to_vec()])
        .relay_mode(RelayMode::Default)
        .bind()
        .await
        .context("bind failed")?;

    endpoint.online().await;
    let addr = endpoint.addr();
    let endpoint_id = endpoint.id().to_string();
    let relay = addr
        .relay_urls()
        .next()
        .map(|u| u.to_string())
        .unwrap_or_else(|| "<none>".to_owned());
    let direct_addrs = addr.ip_addrs().map(|a| a.to_string()).collect::<Vec<_>>();

    {
        let mut r = lock_or_recover(&runtime);
        r.endpoint_id = endpoint_id.clone();
        r.relay_url = relay.clone();
        r.direct_addrs = direct_addrs.clone();
        r.local_port = api_port;
        r.started_at = unix_secs();
        r.online = true;
        r.last_error = None;
    }

    eprintln!(
        "[iroh] API tunnel online: endpoint_id={endpoint_id} relay={relay} addrs=[{}] alpn={} api=127.0.0.1:{api_port}",
        direct_addrs.join(", "),
        String::from_utf8_lossy(IROH_API_ALPN)
    );

    while let Some(incoming) = endpoint.accept().await {
        let mut accepting = match incoming.accept() {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[iroh] incoming accept failed: {e}");
                continue;
            }
        };

        let alpn = match accepting.alpn().await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("[iroh] ALPN read failed: {e}");
                continue;
            }
        };

        let is_device_proxy = alpn.as_slice() == IROH_DEVICE_PROXY_ALPN;
        if alpn.as_slice() != IROH_API_ALPN && !is_device_proxy {
            eprintln!(
                "[iroh] rejected connection with unexpected ALPN: {}",
                String::from_utf8_lossy(&alpn)
            );
            continue;
        }

        let remote = accepting.remote_addr();
        let conn = match accepting.await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[iroh] connection handshake failed: {e}");
                continue;
            }
        };

        let peer = conn.remote_id().to_string().to_lowercase();
        let remote_s = format!("{:?}", remote);

        let is_authorized = {
            let auth_g = lock_or_recover(&auth);
            auth_g.is_endpoint_allowed(&peer)
        };

        if is_authorized {
            let geo = geo_from_remote_addr(&remote);
            let _ = lock_or_recover(&auth).mark_client_connected(&peer, &remote_s, geo);
            eprintln!("[iroh] peer connected (authorized): {peer}");
        } else {
            // Allow unregistered peers through — they can only hit the
            // registration endpoint.  The API server enforces per-command
            // permissions; unregistered peers that try anything else get
            // a 403.  This lets phones register their iroh endpoint ID
            // via the tunnel itself (no local network needed).
            eprintln!("[iroh] peer connected (unregistered, can register): {peer}");
        }

        // ── Route by ALPN ─────────────────────────────────────────────────
        if is_device_proxy {
            // Device proxy stream — requires authorization
            if !is_authorized {
                eprintln!("[iroh] rejecting unauthorized device proxy from {peer}");
                continue;
            }
            // Pass the shared Arc so the handler re-reads the current tx on
            // every message — the session runner replaces it when a session
            // starts, so events flow through without a tunnel restart.
            let device_tx2 = device_event_tx.clone();
            let peer2 = peer.clone();
            tokio::spawn(async move {
                crate::device_receiver::handle_device_proxy_connection(conn, device_tx2, peer2).await;
            });
            continue;
        }

        let auth2 = auth.clone();
        let peer2 = peer.clone();
        let peer_map2 = peer_map.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(conn, api_port, auth2, peer2.clone(), peer_map2).await {
                eprintln!("[iroh] peer {peer2} disconnected: {e}");
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    conn: Connection,
    local_port: u16,
    auth: SharedIrohAuth,
    peer_id: String,
    peer_map: IrohPeerMap,
) -> anyhow::Result<()> {
    // Track whether this peer has ever been authorized.
    // Unregistered peers get a grace window to send a registration request.
    let mut was_ever_authorized = { lock_or_recover(&auth).is_endpoint_allowed(&peer_id) };

    loop {
        // If the peer was previously authorized, check if they've been revoked.
        // Don't block unregistered peers — they need to register first.
        if was_ever_authorized {
            let auth_g = lock_or_recover(&auth);
            if !auth_g.is_endpoint_allowed(&peer_id) {
                drop(auth_g);
                conn.close(1u32.into(), b"revoked");
                anyhow::bail!("client {peer_id} was revoked, closing connection");
            }
        }

        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(e) => anyhow::bail!("accept_bi failed: {e}"),
        };

        // Re-check after accept
        if was_ever_authorized {
            let auth_g = lock_or_recover(&auth);
            if !auth_g.is_endpoint_allowed(&peer_id) {
                drop(auth_g);
                conn.close(1u32.into(), b"revoked");
                anyhow::bail!("client {peer_id} was revoked, closing connection");
            }
        }

        // After each stream, check if the peer became authorized (registered)
        if !was_ever_authorized {
            was_ever_authorized = lock_or_recover(&auth).is_endpoint_allowed(&peer_id);
        }

        let target = SocketAddr::from(([127, 0, 0, 1], local_port));
        let tcp = TcpStream::connect(target)
            .await
            .with_context(|| format!("tcp connect {target} failed"))?;

        // Register the local TCP source port → peer mapping so the axum
        // server can look up which iroh peer this connection belongs to.
        let local_src_port = tcp.local_addr().map(|a| a.port()).unwrap_or(0);
        if local_src_port != 0 {
            lock_or_recover(&peer_map).insert(local_src_port, peer_id.clone());
        }

        let (mut tcp_read, mut tcp_write) = tcp.into_split();

        let uplink = async {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = tcp_read.read(&mut buf).await.context("tcp read failed")?;
                if n == 0 {
                    send.finish().context("send finish failed")?;
                    return Ok::<(), anyhow::Error>(());
                }
                send.write_all(&buf[..n]).await.context("iroh write failed")?;
            }
        };

        let downlink = async {
            loop {
                let maybe_chunk = recv.read_chunk(16 * 1024).await.context("iroh read failed")?;
                let Some(chunk) = maybe_chunk else {
                    tcp_write.shutdown().await.context("tcp shutdown failed")?;
                    return Ok::<(), anyhow::Error>(());
                };
                tcp_write.write_all(&chunk.bytes).await.context("tcp write failed")?;
            }
        };

        let result = tokio::try_join!(uplink, downlink);

        // Clean up peer map entry
        if local_src_port != 0 {
            lock_or_recover(&peer_map).remove(&local_src_port);
        }

        result?;
    }
}

fn geo_from_remote_addr(remote: &IncomingAddr) -> Option<IrohGeo> {
    let ip = match remote {
        IncomingAddr::Ip(sa) => sa.ip().to_string(),
        _ => return None,
    };

    let locale = std::env::var("LANG").ok();

    // Best-effort geolocation; failures are ignored.
    let mut country = None;
    let mut city = None;
    if let Ok(resp) = ureq::get(&format!("https://ipapi.co/{ip}/json/")).call() {
        if let Ok(json) = resp.into_body().read_json::<serde_json::Value>() {
            country = json
                .get("country_name")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
            city = json
                .get("city")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned);
        }
    }

    Some(IrohGeo {
        ip,
        country,
        city,
        locale,
    })
}

const IROH_KEY_HISTORY_FILE: &str = "iroh_key_history.json";

/// Rotate the iroh secret key.  The old key is archived in `iroh_key_history.json`.
/// Returns `(old_endpoint_id, new_endpoint_id)`.
///
/// **Breaking**: all existing iroh connections will drop.  Clients need to
/// re-scan an updated invite QR (TOTP secrets remain valid — only the
/// endpoint_id changes).
pub fn rotate_secret_key(skill_dir: &Path) -> anyhow::Result<(String, String)> {
    let key_path = skill_dir.join(IROH_SECRET_FILE);
    let history_path = skill_dir.join(IROH_KEY_HISTORY_FILE);

    // Load old key (if any)
    let old_key = load_or_create_secret_key(skill_dir)?;
    let old_id = old_key.public().to_string();

    // Archive old key
    let mut history: Vec<serde_json::Value> = std::fs::read_to_string(&history_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    history.push(serde_json::json!({
        "endpoint_id": old_id,
        "rotated_at": crate::unix_secs(),
    }));
    let hist_json = serde_json::to_string_pretty(&history).context("serialize history")?;
    std::fs::write(&history_path, hist_json).context("write history")?;

    // Generate new key
    let new_key = {
        let mut rng = rand::rng();
        SecretKey::generate(&mut rng)
    };
    std::fs::write(&key_path, new_key.to_bytes()).context("write new key")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
    }

    let new_id = new_key.public().to_string();
    eprintln!("[iroh] key rotated: {old_id} → {new_id}");

    Ok((old_id, new_id))
}

/// Return the key rotation history.
pub fn key_history(skill_dir: &Path) -> Vec<serde_json::Value> {
    let path = skill_dir.join(IROH_KEY_HISTORY_FILE);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn load_or_create_secret_key(skill_dir: &Path) -> anyhow::Result<SecretKey> {
    let path = skill_dir.join(IROH_SECRET_FILE);

    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut raw = [0u8; 32];
            raw.copy_from_slice(&bytes);
            return Ok(SecretKey::from_bytes(&raw));
        }
        return Err(anyhow::anyhow!(
            "invalid iroh key file {}: expected 32 bytes, got {}",
            path.display(),
            bytes.len()
        ));
    }

    let secret = {
        let mut rng = rand::rng();
        SecretKey::generate(&mut rng)
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, secret.to_bytes())
        .map_err(|e| anyhow::anyhow!("failed to persist {}: {e}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(secret)
}

#[cfg(test)]
#[allow(clippy::all)]
mod tests {
    use super::*;

    #[test]
    fn load_or_create_secret_key_creates_new_key() {
        let td = tempfile::tempdir().expect("td");
        let key1 = load_or_create_secret_key(td.path()).expect("create");
        let path = td.path().join(IROH_SECRET_FILE);
        assert!(path.exists());
        let bytes = std::fs::read(&path).expect("read");
        assert_eq!(bytes.len(), 32);

        // Loading again returns the same key
        let key2 = load_or_create_secret_key(td.path()).expect("load");
        assert_eq!(key1.to_bytes(), key2.to_bytes());
    }

    #[test]
    fn load_or_create_secret_key_rejects_bad_file() {
        let td = tempfile::tempdir().expect("td");
        let path = td.path().join(IROH_SECRET_FILE);
        std::fs::write(&path, b"too_short").expect("write");
        let result = load_or_create_secret_key(td.path());
        assert!(result.is_err());
        assert!(result.expect_err("err").to_string().contains("expected 32 bytes"));
    }

    #[test]
    fn load_or_create_secret_key_deterministic_from_file() {
        let td = tempfile::tempdir().expect("td");
        let path = td.path().join(IROH_SECRET_FILE);
        let raw = [42u8; 32];
        std::fs::write(&path, raw).expect("write");
        let key = load_or_create_secret_key(td.path()).expect("load");
        assert_eq!(key.to_bytes(), raw);
    }

    #[test]
    fn iroh_runtime_state_default() {
        let state = IrohRuntimeState::default();
        assert!(!state.online);
        assert!(state.endpoint_id.is_empty());
        assert!(state.relay_url.is_empty());
        assert_eq!(state.local_port, 0);
        assert!(state.last_error.is_none());
    }

    #[test]
    fn rotate_secret_key_changes_endpoint_id() {
        let td = tempfile::tempdir().expect("td");
        // Create initial key
        let _k = load_or_create_secret_key(td.path()).expect("create");

        let (old_id, new_id) = super::rotate_secret_key(td.path()).expect("rotate");
        assert_ne!(old_id, new_id, "rotation should produce a different endpoint_id");

        // Loading key should return the new one
        let loaded = load_or_create_secret_key(td.path()).expect("load after rotate");
        let loaded_id = iroh::PublicKey::from(loaded.public()).to_string();
        assert_eq!(loaded_id, new_id);

        // History should have one entry
        let hist = super::key_history(td.path());
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0]["endpoint_id"].as_str().unwrap(), &old_id);
    }

    #[test]
    fn rotate_twice_builds_history() {
        let td = tempfile::tempdir().expect("td");
        let _k = load_or_create_secret_key(td.path()).expect("create");

        super::rotate_secret_key(td.path()).expect("rotate 1");
        super::rotate_secret_key(td.path()).expect("rotate 2");

        let hist = super::key_history(td.path());
        assert_eq!(hist.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn secret_key_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let td = tempfile::tempdir().expect("td");
        load_or_create_secret_key(td.path()).expect("create");
        let path = td.path().join(IROH_SECRET_FILE);
        let perms = std::fs::metadata(&path).expect("meta").permissions();
        assert_eq!(perms.mode() & 0o777, 0o600, "secret key file should be owner-only");
    }
}
