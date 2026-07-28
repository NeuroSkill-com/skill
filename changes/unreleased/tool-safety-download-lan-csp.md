### Server

- **Tool safety defaults**: Default `require_bash_edit` to `true`, expand bash denylist coverage (`curl|bash`, interpreters), and require approval for secret home paths (`.ssh`, `.aws`, `.env`, `auth.token`) and Windows system directories.
- **`web_fetch` SSRF guardrails**: Refuse loopback, private, link-local, and cloud-metadata URLs.
- **LAN bind safety**: Refuse non-loopback `SKILL_DAEMON_ADDR` unless `SKILL_DAEMON_ALLOW_LAN=1`, warn on `0.0.0.0` WebSocket host usage, and document CORS plus loopback defaults.

### LLM

- **Model download integrity checks**: SHA-256 verify HF LFS blobs before promoting `.incomplete` to final blob, re-verify existing blobs, and delete mismatched files.

### Build

- **Desktop CSP hardening**: Replace `csp: null` with a localhost/daemon-aware Content-Security-Policy in `tauri.conf.json`.
