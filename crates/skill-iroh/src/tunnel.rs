// SPDX-License-Identifier: GPL-3.0-only

use std::{
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
const IROH_SECRET_FILE: &str = "iroh_secret_key.bin";

pub type SharedIrohAuth = Arc<Mutex<IrohAuthStore>>;
pub type SharedIrohRuntime = Arc<Mutex<IrohRuntimeState>>;

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

pub fn spawn(
    skill_dir: PathBuf,
    full_port: u16,
    read_only_port: u16,
    auth: SharedIrohAuth,
    runtime: SharedIrohRuntime,
) {
    tokio::spawn(async move {
        if let Err(e) = run(&skill_dir, full_port, read_only_port, auth, runtime.clone()).await {
            lock_or_recover(&runtime).last_error = Some(e.clone());
            eprintln!("[iroh] tunnel stopped: {e}");
        }
    });
}

async fn run(
    skill_dir: &Path,
    full_port: u16,
    read_only_port: u16,
    auth: SharedIrohAuth,
    runtime: SharedIrohRuntime,
) -> Result<(), String> {
    let secret_key = load_or_create_secret_key(skill_dir)?;

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![IROH_API_ALPN.to_vec()])
        .relay_mode(RelayMode::Default)
        .bind()
        .await
        .map_err(|e| format!("bind failed: {e}"))?;

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
        r.local_port = full_port;
        r.started_at = unix_secs();
        r.online = true;
        r.last_error = None;
    }

    eprintln!(
        "[iroh] API tunnel online: endpoint_id={endpoint_id} relay={relay} addrs=[{}] alpn={} full=127.0.0.1:{full_port} read=127.0.0.1:{read_only_port}",
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
        if alpn.as_slice() != IROH_API_ALPN {
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

        let scope = {
            let auth_g = lock_or_recover(&auth);
            if !auth_g.is_endpoint_allowed(&peer) {
                eprintln!("[iroh] rejecting unauthorized endpoint: {peer}");
                continue;
            }
            auth_g.scope_for_endpoint(&peer).unwrap_or_else(|| "read".to_string())
        };

        let geo = geo_from_remote_addr(&remote);
        let _ = lock_or_recover(&auth).mark_client_connected(&peer, &remote_s, geo);

        let target_port = if scope == "full" { full_port } else { read_only_port };
        eprintln!("[iroh] peer connected: {peer} (scope={scope}, target_port={target_port})");
        tokio::spawn(async move {
            if let Err(e) = handle_connection(conn, target_port).await {
                eprintln!("[iroh] peer {peer} disconnected: {e}");
            }
        });
    }

    Ok(())
}

async fn handle_connection(conn: Connection, local_port: u16) -> Result<(), String> {
    loop {
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(e) => return Err(format!("accept_bi failed: {e}")),
        };

        let target = SocketAddr::from(([127, 0, 0, 1], local_port));
        let tcp = TcpStream::connect(target)
            .await
            .map_err(|e| format!("tcp connect {target} failed: {e}"))?;
        let (mut tcp_read, mut tcp_write) = tcp.into_split();

        let uplink = async {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = tcp_read
                    .read(&mut buf)
                    .await
                    .map_err(|e| format!("tcp read failed: {e}"))?;
                if n == 0 {
                    send.finish().map_err(|e| format!("send finish failed: {e}"))?;
                    return Ok::<(), String>(());
                }
                send.write_all(&buf[..n])
                    .await
                    .map_err(|e| format!("iroh write failed: {e}"))?;
            }
        };

        let downlink = async {
            loop {
                let maybe_chunk = recv
                    .read_chunk(16 * 1024)
                    .await
                    .map_err(|e| format!("iroh read failed: {e}"))?;
                let Some(chunk) = maybe_chunk else {
                    tcp_write
                        .shutdown()
                        .await
                        .map_err(|e| format!("tcp shutdown failed: {e}"))?;
                    return Ok::<(), String>(());
                };
                tcp_write
                    .write_all(&chunk.bytes)
                    .await
                    .map_err(|e| format!("tcp write failed: {e}"))?;
            }
        };

        let _ = tokio::try_join!(uplink, downlink)?;
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

fn load_or_create_secret_key(skill_dir: &Path) -> Result<SecretKey, String> {
    let path = skill_dir.join(IROH_SECRET_FILE);

    if let Ok(bytes) = std::fs::read(&path) {
        if bytes.len() == 32 {
            let mut raw = [0u8; 32];
            raw.copy_from_slice(&bytes);
            return Ok(SecretKey::from_bytes(&raw));
        }
        return Err(format!(
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
    std::fs::write(&path, secret.to_bytes()).map_err(|e| format!("failed to persist {}: {e}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    Ok(secret)
}
