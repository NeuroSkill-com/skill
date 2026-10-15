//! Shared WiFi Shield communication utilities used by all WiFi-enabled boards.
//!
//! The OpenBCI WiFi Shield:
//!  1. Exposes an HTTP API (GET /board, POST /tcp, POST /command, GET /stream/start, GET /stream/stop).
//!  2. Connects to a TCP server we create on the host PC and streams 33-byte packets to it.
//!  3. Can be auto-discovered via SSDP on the local network, or reached at its
//!     default AP address (192.168.4.1).

use std::net::{TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

use crate::error::{OpenBciError, Result};

// ─────────────────────────────────────────────────────────────────────────────

/// Find the local network IP that can route to `remote_ip`.
///
/// We connect a UDP socket (no actual packet sent) and read back the
/// chosen local interface address — same trick as BrainFlow's C++ code.
pub fn local_ip_for(remote_ip: &str) -> Result<String> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.connect(format!("{}:80", remote_ip))?;
    let addr = sock.local_addr()?;
    Ok(addr.ip().to_string())
}

/// Attempt SSDP discovery of the OpenBCI WiFi Shield on the local network.
/// Falls back to the default AP-mode address `192.168.4.1` on failure.
pub fn discover_wifi_shield(timeout: Duration) -> String {
    let default_ip = "192.168.4.1".to_string();

    let Ok(sock) = UdpSocket::bind("0.0.0.0:0") else { return default_ip; };
    if sock.set_read_timeout(Some(Duration::from_secs(3))).is_err() { return default_ip; }
    if sock.set_broadcast(true).is_err() { return default_ip; }

    let msearch = concat!(
        "M-SEARCH * HTTP/1.1\r\n",
        "Host: 239.255.255.250:1900\r\n",
        "MAN: ssdp:discover\r\n",
        "ST: urn:schemas-upnp-org:device:Basic:1\r\n",
        "MX: 3\r\n\r\n\r\n",
    );

    if sock.send_to(msearch.as_bytes(), "239.255.255.250:1900").is_err() {
        return default_ip;
    }

    let deadline = std::time::Instant::now() + timeout;
    let mut buf = [0u8; 512];

    while std::time::Instant::now() < deadline {
        let Ok((n, _)) = sock.recv_from(&mut buf) else { break };
        let resp = String::from_utf8_lossy(&buf[..n]);
        if let Some(ip) = parse_ssdp_location_ip(&resp) {
            return ip;
        }
    }
    default_ip
}

/// Parse the `LOCATION:` header from an SSDP response and extract the IP address.
///
/// Looks for a line starting with `location:` (case-insensitive), then pulls
/// the host portion from the `http://…` URL.  Returns `None` if no valid IP
/// is found.
fn parse_ssdp_location_ip(resp: &str) -> Option<String> {
    // Look for "LOCATION: http://A.B.C.D"
    for line in resp.lines() {
        let low = line.to_lowercase();
        if low.starts_with("location:") {
            // extract http://IP
            if let Some(start) = line.find("http://") {
                let after_scheme = &line[start + 7..];
                // IP ends at '/' or end-of-word
                let end = after_scheme
                    .find(|c: char| c == '/' || c.is_whitespace())
                    .unwrap_or(after_scheme.len());
                let candidate = &after_scheme[..end];
                // Basic validation: must contain dots
                if candidate.contains('.') {
                    return Some(candidate.to_string());
                }
            }
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────

/// Low-level WiFi Shield connection configuration, used internally by all
/// WiFi-enabled board drivers.
///
/// Higher-level board configs (e.g. [`super::cyton_wifi::CytonWifiConfig`]) mirror
/// these fields and copy them into a `WifiShieldConfig` before calling
/// [`connect_wifi_shield`].
#[derive(Debug, Clone)]
pub struct WifiShieldConfig {
    /// Shield's IP address.  Empty string triggers SSDP auto-discovery; the
    /// resolved address is written back here after discovery.
    pub shield_ip: String,
    /// Local TCP port on which the host accepts incoming stream data from the
    /// shield.
    pub local_port: u16,
    /// Timeout in seconds for HTTP calls to the shield's REST API endpoints
    /// (`/board`, `/tcp`, `/command`, `/stream/start`, `/stream/stop`), and
    /// for the initial TCP `accept()` while waiting for the shield to connect.
    pub http_timeout: u64,
}

impl Default for WifiShieldConfig {
    fn default() -> Self {
        Self {
            shield_ip:    String::new(),
            local_port:   3000,
            http_timeout: 10,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Establish a connection to the WiFi Shield and return a `TcpStream` ready
/// for reading 33-byte data packets.
///
/// Steps:
///  1. Resolve / discover the shield's IP.
///  2. Determine our local IP reachable from the shield.
///  3. Start a `TcpListener` on `config.local_port`.
///  4. POST our TCP endpoint to the shield.
///  5. Wait for the shield to connect back (up to `http_timeout` seconds).
///  6. POST `/command ~4` to freeze the sampling rate at 1000 Hz (Cyton WiFi
///     default; callers can change it afterwards).
///  7. Return the connected `TcpStream`.
pub fn connect_wifi_shield(config: &mut WifiShieldConfig) -> Result<TcpStream> {
    // 1. Resolve shield IP
    if config.shield_ip.is_empty() {
        config.shield_ip = discover_wifi_shield(Duration::from_secs(config.http_timeout));
        log::info!("WiFi shield discovered at {}", config.shield_ip);
    }
    let ip = &config.shield_ip;

    // 2. Local IP
    let local_ip = local_ip_for(ip)?;
    log::info!("Local IP for WiFi communication: {}", local_ip);

    // 3. Bind TCP listener
    let listener = TcpListener::bind(format!("{}:{}", local_ip, config.local_port))?;
    listener.set_nonblocking(false)?;

    // Set a read timeout so accept() eventually gives up
    let accept_timeout = Duration::from_secs(config.http_timeout);

    // 4. Query board info
    let board_url = format!("http://{}/board", ip);
    ureq::get(&board_url)
        .timeout(Duration::from_secs(config.http_timeout))
        .call()
        .map_err(|e| OpenBciError::Wifi(e.to_string()))?;

    // POST our TCP endpoint to the shield
    let tcp_url = format!("http://{}/tcp", ip);
    let body = serde_json::json!({
        "ip":        local_ip,
        "port":      config.local_port,
        "output":    "raw",
        "delimiter": true,
        "latency":   10000
    });
    ureq::post(&tcp_url)
        .timeout(Duration::from_secs(config.http_timeout))
        .send_json(body)
        .map_err(|e| OpenBciError::Wifi(e.to_string()))?;

    // 5. Wait for shield connection (poll with non-blocking + sleep)
    listener.set_nonblocking(true)?;
    let deadline = std::time::Instant::now() + accept_timeout;
    loop {
        match listener.accept() {
            Ok((stream, _addr)) => {
                log::info!("WiFi shield connected to our TCP server");
                stream.set_nodelay(true)?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                return Ok(stream);
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    return Err(OpenBciError::Timeout);
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────

/// Send an HTTP command to the shield's `/command` endpoint.
pub fn send_wifi_command(shield_ip: &str, cmd: &str, http_timeout: u64) -> Result<()> {
    let url  = format!("http://{}/command", shield_ip);
    let body = serde_json::json!({ "command": cmd });
    ureq::post(&url)
        .timeout(Duration::from_secs(http_timeout))
        .send_json(body)
        .map_err(|e| OpenBciError::Wifi(e.to_string()))?;
    Ok(())
}

/// Tell the shield to start streaming (`GET /stream/start`).
pub fn wifi_start_stream(shield_ip: &str, http_timeout: u64) -> Result<()> {
    let url = format!("http://{}/stream/start", shield_ip);
    ureq::get(&url)
        .timeout(Duration::from_secs(http_timeout))
        .call()
        .map_err(|e| OpenBciError::Wifi(e.to_string()))?;
    Ok(())
}

/// Tell the shield to stop streaming (`GET /stream/stop`).
pub fn wifi_stop_stream(shield_ip: &str, http_timeout: u64) -> Result<()> {
    let url = format!("http://{}/stream/stop", shield_ip);
    ureq::get(&url)
        .timeout(Duration::from_secs(http_timeout))
        .call()
        .map_err(|e| OpenBciError::Wifi(e.to_string()))?;
    Ok(())
}
