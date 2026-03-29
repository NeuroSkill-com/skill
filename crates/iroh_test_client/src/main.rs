use serde::Deserialize;
use std::str::FromStr;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
struct Opt {
    /// Path to JSON payload or literal JSON string containing endpoint_id, relay_url, and otpauth_url or secret_base32
    #[structopt(long)]
    payload: String,

    /// Optional client display name
    #[structopt(long)]
    name: Option<String>,

    /// Scope: read or full
    #[structopt(long, default_value = "read")]
    scope: String,
}

#[derive(Deserialize)]
struct Payload {
    endpoint_id: String,
    relay_url: String,
    // either otpauth_url (otpauth://...) or secret_base32
    otpauth_url: Option<String>,
    secret_base32: Option<String>,
}

const IROH_ALPN: &[u8] = b"skill/http-ws/1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    let opt = Opt::from_args();

    let payload_json = if std::path::Path::new(&opt.payload).exists() {
        std::fs::read_to_string(&opt.payload)?
    } else {
        opt.payload.clone()
    };
    let p: Payload = serde_json::from_str(&payload_json)?;

    // Determine secret bytes: prefer otpauth_url secret param, else secret_base32
    let secret_bytes = if let Some(url) = p.otpauth_url.as_ref() {
        let parsed = url::Url::parse(url).map_err(|e| anyhow::anyhow!("invalid otpauth url: {}", e))?;
        let mut secret: Option<String> = None;
        for (k, v) in parsed.query_pairs() {
            if k == "secret" {
                secret = Some(v.into_owned());
                break;
            }
        }
        let b32 = secret.ok_or_else(|| anyhow::anyhow!("otpauth url missing secret param"))?;
        base32::decode(base32::Alphabet::RFC4648 { padding: false }, &b32)
            .ok_or_else(|| anyhow::anyhow!("invalid base32 secret in otpauth url"))?
    } else if let Some(b32) = p.secret_base32.as_ref() {
        base32::decode(base32::Alphabet::RFC4648 { padding: false }, b32)
            .ok_or_else(|| anyhow::anyhow!("invalid base32 secret"))?
    } else {
        anyhow::bail!("payload must include otpauth_url or secret_base32");
    };

    // Create local iroh endpoint (builder will generate a secret key if not provided)
    let endpoint = iroh::Endpoint::builder(iroh::endpoint::presets::N0)
        .relay_mode(iroh::RelayMode::Default)
        .bind()
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind endpoint: {}", e))?;

    endpoint.online().await;

    println!("Local endpoint id: {}", endpoint.id());

    // Parse remote endpoint id and relay
    let remote_id = iroh_base::PublicKey::from_str(&p.endpoint_id)?;
    let relay_url: iroh_base::RelayUrl = p
        .relay_url
        .parse()
        .map_err(|e| anyhow::anyhow!("invalid relay url: {}", e))?;

    let addr =
        iroh_base::EndpointAddr::from_parts(remote_id, std::iter::once(iroh_base::TransportAddr::Relay(relay_url)));

    println!("Connecting to remote...");
    let conn = endpoint
        .connect(addr, IROH_ALPN)
        .await
        .map_err(|e| anyhow::anyhow!("connect failed: {:?}", e))?;
    println!("Connected to remote: {}", conn.remote_id());

    // Generate current TOTP using totp-rs
    let totp = totp_rs::TOTP::new(
        totp_rs::Algorithm::SHA1,
        6,
        1,
        30,
        secret_bytes,
        Some("Skill".to_string()),
        opt.name.clone().unwrap_or_else(|| "client".to_string()),
    )
    .unwrap();
    let code = totp.generate_current().unwrap();

    // Prepare HTTP POST to /v1/iroh/clients/register
    let body = serde_json::json!({
        "endpoint_id": endpoint.id().to_string(),
        "otp": code,
        "name": opt.name.clone().unwrap_or_else(|| "iroh-test-client".to_string()),
        "scope": opt.scope.clone(),
    })
    .to_string();

    let req = format!(
        "POST /v1/iroh/clients/register HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        body.len(), body
    );

    // Open a bi-directional stream
    let (mut send, mut recv) = conn
        .open_bi()
        .await
        .map_err(|e| anyhow::anyhow!("open_bi failed: {:?}", e))?;

    // Write request
    send.write_all(req.as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!("send failed: {}", e))?;
    send.finish()
        .map_err(|e| anyhow::anyhow!("send finish failed: {:?}", e))?;

    // Read response using iroh recv.read_to_end
    let response = recv
        .read_to_end(16 * 1024)
        .await
        .map_err(|e| anyhow::anyhow!("read_to_end failed: {:?}", e))?;

    println!("Server response:\n{}", String::from_utf8_lossy(&response));

    Ok(())
}
