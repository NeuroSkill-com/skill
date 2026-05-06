// SPDX-License-Identifier: GPL-3.0-only
//
// Failsafe daemon upgrade protocol.
//
// Goals: idempotent, atomic, observable. On every app launch we reconcile
// the running daemon against the binary bundled with this app. State lives
// in `~/.config/skill/daemon/state.json`; per-phase events are appended to
// `~/.config/skill/daemon/upgrade.log`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const STATE_VERSION: u32 = 1;
const KILL_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Ready,
    Upgrading,
    RollingBack,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeState {
    pub version: u32,
    pub installed_hash: Option<String>,
    pub installed_version: Option<String>,
    pub rollback_hash: Option<String>,
    pub rollback_version: Option<String>,
    pub phase: Phase,
    pub attempt_count: u32,
    pub last_error: Option<String>,
    pub updated_at: String,
}

impl Default for UpgradeState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            installed_hash: None,
            installed_version: None,
            rollback_hash: None,
            rollback_version: None,
            phase: Phase::Ready,
            attempt_count: 0,
            last_error: None,
            updated_at: now_iso(),
        }
    }
}

// ─── Paths ───────────────────────────────────────────────────────────────────

fn config_root() -> PathBuf {
    // Test/sandbox escape hatch — keeps the upgrade state and pidfile path
    // overridable without touching HOME/XDG_CONFIG_HOME (which would affect
    // unrelated libs). The daemon binary itself reads the same variable.
    if let Ok(p) = std::env::var("SKILL_DAEMON_CONFIG_ROOT") {
        return PathBuf::from(p);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("skill")
        .join("daemon")
}

pub fn state_path() -> PathBuf {
    config_root().join("state.json")
}

pub fn upgrade_log_path() -> PathBuf {
    config_root().join("upgrade.log")
}

pub fn pidfile_path() -> PathBuf {
    config_root().join("daemon.pid")
}

pub fn rollback_bin_path() -> PathBuf {
    let name = if cfg!(target_os = "windows") {
        "skill-daemon.rollback.exe"
    } else {
        "skill-daemon.rollback"
    };
    config_root().join("bin").join(name)
}

// ─── State load/save ─────────────────────────────────────────────────────────

pub fn load_state() -> UpgradeState {
    let path = state_path();
    let Ok(bytes) = fs::read(&path) else {
        return UpgradeState::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

pub fn save_state(state: &mut UpgradeState) {
    state.version = STATE_VERSION;
    state.updated_at = now_iso();
    let path = state_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if let Ok(bytes) = serde_json::to_vec_pretty(state) {
        if fs::write(&tmp, &bytes).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

// ─── Logging ─────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct LogEntry<'a> {
    ts: String,
    phase: &'a str,
    event: &'a str,
    detail: Option<&'a str>,
}

pub fn log_event(phase: &str, event: &str, detail: Option<&str>) {
    eprintln!(
        "[upgrade] {phase}/{event}{}",
        detail.map(|d| format!(": {d}")).unwrap_or_default()
    );
    let entry = LogEntry {
        ts: now_iso(),
        phase,
        event,
        detail,
    };
    let Ok(mut line) = serde_json::to_string(&entry) else {
        return;
    };
    line.push('\n');
    let path = upgrade_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = f.write_all(line.as_bytes());
    }
}

// ─── Hashing ─────────────────────────────────────────────────────────────────

pub fn sha256_file(path: &Path) -> Option<String> {
    let mut f = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

// ─── PID-based kill ──────────────────────────────────────────────────────────

pub fn read_pidfile() -> Option<u32> {
    let txt = fs::read_to_string(pidfile_path()).ok()?;
    txt.trim().parse::<u32>().ok()
}

pub fn process_alive(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        // On Linux, /proc/<pid>/status reports State: Z for zombies, which
        // are not "alive" for our purposes — they've been killed and are
        // just waiting to be reaped by the parent. `kill(pid, 0)` would
        // return 0 (alive) for them, masking successful kills in tests
        // where the parent doesn't immediately waitpid.
        if let Ok(s) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
            for line in s.lines() {
                if let Some(rest) = line.strip_prefix("State:") {
                    let state = rest.trim().chars().next().unwrap_or(' ');
                    return state != 'Z' && state != 'X';
                }
            }
        }
        // Fall through to kill(0) when /proc isn't readable.
    }
    #[cfg(unix)]
    {
        // signal 0: existence check, no actual signal sent.
        return unsafe { libc::kill(pid as libc::pid_t, 0) == 0 };
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output();
        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()),
            Err(_) => false,
        }
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = pid;
        false
    }
}

/// Kill the daemon by its pidfile. SIGTERM first, escalate to SIGKILL after
/// `KILL_GRACE`. Returns `true` if the process was killed (or wasn't running).
pub fn kill_pidfile_daemon() -> bool {
    let Some(pid) = read_pidfile() else {
        log_event("stop", "no_pidfile", None);
        return true;
    };
    if !process_alive(pid) {
        log_event("stop", "pid_not_alive", Some(&pid.to_string()));
        let _ = fs::remove_file(pidfile_path());
        return true;
    }

    log_event("stop", "sigterm", Some(&pid.to_string()));
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGTERM);
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .output();
    }

    let deadline = Instant::now() + KILL_GRACE;
    while Instant::now() < deadline {
        if !process_alive(pid) {
            let _ = fs::remove_file(pidfile_path());
            log_event("stop", "exited_after_sigterm", Some(&pid.to_string()));
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    log_event("stop", "sigkill", Some(&pid.to_string()));
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as libc::pid_t, libc::SIGKILL);
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .output();
    }
    std::thread::sleep(Duration::from_millis(200));
    let dead = !process_alive(pid);
    if dead {
        let _ = fs::remove_file(pidfile_path());
    }
    dead
}

/// Best-effort: kill whatever process is bound to `port`.
pub fn kill_port_owner(port: u16) {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if let Ok(output) = std::process::Command::new("lsof")
            .args(["-t", "-i", &format!("tcp:{port}")])
            .output()
        {
            for pid in String::from_utf8_lossy(&output.stdout).split_whitespace() {
                log_event("stop", "kill_port_owner", Some(pid));
                let _ = std::process::Command::new("kill")
                    .args(["-9", pid])
                    .output();
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("netstat")
            .args(["-ano", "-p", "TCP"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let needle = format!(":{port}");
            for line in text.lines() {
                if line.contains(&needle) && line.contains("LISTENING") {
                    if let Some(pid) = line.split_whitespace().last() {
                        log_event("stop", "kill_port_owner", Some(pid));
                        let _ = std::process::Command::new("taskkill")
                            .args(["/PID", pid, "/F"])
                            .output();
                    }
                }
            }
        }
    }
}

pub fn wait_for_port_free(port: u16, timeout: Duration) -> bool {
    let addr: std::net::SocketAddr = ([127, 0, 0, 1], port).into();
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        // A successful connect means someone is still listening.
        if std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(150)).is_err() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    false
}

// ─── OS service management (no -w, idempotent) ───────────────────────────────

#[cfg(target_os = "macos")]
fn launch_agent_plist() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library/LaunchAgents/com.skill.daemon.plist")
}

pub fn unload_os_service_best_effort() {
    #[cfg(target_os = "macos")]
    {
        let plist = launch_agent_plist();
        if !plist.exists() {
            return;
        }
        // bootout cleanly stops & unloads without disabling the plist
        // (which `launchctl unload -w` would do). Falls back to plain `unload`
        // on macOS versions where bootout is unavailable.
        let uid_str = format!("gui/{}", unsafe { libc::getuid() });
        let bootout_ok = std::process::Command::new("launchctl")
            .args(["bootout", &uid_str])
            .arg(&plist)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !bootout_ok {
            let _ = std::process::Command::new("launchctl")
                .arg("unload")
                .arg(&plist)
                .output();
        }
        log_event("stop", "launchd_unloaded", None);
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "stop", "skill-daemon.service"])
            .output();
        log_event("stop", "systemd_stopped", None);
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("sc")
            .args(["stop", "skill-daemon"])
            .output();
        log_event("stop", "sc_stopped", None);
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn copy_atomic(src: &Path, dst: &Path) -> std::io::Result<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dst.with_extension("tmp");
    fs::copy(src, &tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755));
    }
    fs::rename(&tmp, dst)?;
    Ok(())
}

// ─── Linux end-to-end tests against real subprocesses ────────────────────────
//
// These tests exercise the failsafe primitives (kill, port wait, state, hash,
// atomic copy) against actual /bin/python3 stub processes, fresh tmpdirs, and
// real OS signals. They run in CI via Dockerfile.upgrade-test.
//
// Each test isolates state via SKILL_DAEMON_CONFIG_ROOT pointing at a unique
// tmpdir and grabs the env-var lock so parallel tests don't trample each
// other's $TEST_PORT / config root.
#[cfg(all(test, target_os = "linux"))]
mod linux_e2e {
    use super::*;
    use std::process::{Command, Stdio};
    use std::sync::Mutex;
    use std::time::Instant;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Embedded Python stub: writes pidfile, binds 127.0.0.1:$PORT, optionally
    /// installs a SIGTERM-ignoring handler. Stays alive accepting connections
    /// until killed.
    const STUB: &str = r#"
import os, sys, signal, socket
pidfile = sys.argv[1]
port = int(sys.argv[2])
ignore_term = "--ignore-sigterm" in sys.argv
s = socket.socket(); s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", port)); s.listen(8)
with open(pidfile, "w") as f: f.write(str(os.getpid()))
if ignore_term:
    signal.signal(signal.SIGTERM, lambda *_: None)
print("READY", flush=True)
while True:
    try:
        c, _ = s.accept(); c.close()
    except KeyboardInterrupt:
        break
"#;

    fn spawn_stub(pidfile: &Path, port: u16, ignore_term: bool) -> std::process::Child {
        let mut cmd = Command::new("python3");
        cmd.arg("-u")
            .arg("-c")
            .arg(STUB)
            .arg(pidfile)
            .arg(port.to_string());
        if ignore_term {
            cmd.arg("--ignore-sigterm");
        }
        let mut child = cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn python3 stub");

        // Wait for "READY\n" on stdout — guarantees pidfile is written and port bound.
        use std::io::{BufRead, BufReader};
        let stdout = child.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) > 0 && line.starts_with("READY") {
                return child;
            }
        }
        child.kill().ok();
        panic!("stub did not become ready");
    }

    fn fresh_root() -> tempfile::TempDir {
        tempfile::tempdir().expect("tmpdir")
    }

    fn set_root(root: &Path) {
        std::env::set_var("SKILL_DAEMON_CONFIG_ROOT", root);
    }

    fn pick_port() -> u16 {
        // Bind to 0 to grab a free port, then close — Linux kernel won't reuse
        // it for a second or two, which is enough for the test to claim it.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    }

    #[test]
    fn kill_pidfile_terminates_responsive_process() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());
        let port = pick_port();
        let pf = pidfile_path();
        let mut child = spawn_stub(&pf, port, false);

        let pid_in_file = read_pidfile().expect("pidfile");
        assert_eq!(pid_in_file, child.id());
        assert!(process_alive(pid_in_file));

        let start = Instant::now();
        assert!(kill_pidfile_daemon(), "kill should succeed");
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_secs(2),
            "responsive process should die fast, took {elapsed:?}"
        );
        assert!(!process_alive(pid_in_file));
        // Pidfile is removed on successful kill.
        assert!(read_pidfile().is_none(), "pidfile should be cleaned up");
        let _ = child.wait();
    }

    #[test]
    fn kill_pidfile_escalates_to_sigkill_when_sigterm_ignored() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());
        let port = pick_port();
        let pf = pidfile_path();
        let mut child = spawn_stub(&pf, port, true); // ignore SIGTERM

        let pid_in_file = read_pidfile().expect("pidfile");
        let start = Instant::now();
        assert!(kill_pidfile_daemon(), "SIGKILL should still finish the job");
        let elapsed = start.elapsed();
        // SIGTERM grace = 3 s, then SIGKILL + 200 ms; allow some slack.
        assert!(
            elapsed >= Duration::from_secs(3),
            "should wait full SIGTERM grace; was {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "SIGKILL should land within 5s; was {elapsed:?}"
        );
        assert!(!process_alive(pid_in_file));
        let _ = child.wait();
    }

    #[test]
    fn wait_for_port_free_blocks_then_releases() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());
        let port = pick_port();
        let pf = pidfile_path();
        let mut child = spawn_stub(&pf, port, false);

        // While bound: should time out fast.
        let start = Instant::now();
        let freed = wait_for_port_free(port, Duration::from_millis(500));
        assert!(!freed, "port should still be bound");
        assert!(start.elapsed() >= Duration::from_millis(450));

        kill_pidfile_daemon();
        let _ = child.wait();

        // After kill: should detect free quickly.
        assert!(
            wait_for_port_free(port, Duration::from_secs(2)),
            "port should free after kill"
        );
    }

    #[test]
    fn process_alive_correct_for_running_and_dead() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());
        let port = pick_port();
        let pf = pidfile_path();
        let mut child = spawn_stub(&pf, port, false);
        let pid = child.id();
        assert!(process_alive(pid));

        let _ = child.kill();
        let _ = child.wait();
        // Reaped — should now report dead.
        assert!(!process_alive(pid));
    }

    #[test]
    fn state_atomic_round_trip_under_concurrent_reads() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());

        // 50 alternating writes from another thread; main thread reads continuously
        // and must never observe a torn / non-deserializable file.
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_clone = stop.clone();
        let writer = std::thread::spawn(move || {
            for i in 0..50 {
                let mut s = load_state();
                s.installed_hash = Some(format!("hash-{i:08}"));
                save_state(&mut s);
                std::thread::sleep(Duration::from_millis(2));
            }
            stop_clone.store(true, std::sync::atomic::Ordering::Relaxed);
        });

        let mut bad_reads = 0u32;
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            let path = state_path();
            if let Ok(bytes) = std::fs::read(&path) {
                if !bytes.is_empty() && serde_json::from_slice::<UpgradeState>(&bytes).is_err() {
                    bad_reads += 1;
                }
            }
        }
        writer.join().unwrap();
        assert_eq!(bad_reads, 0, "atomic rename should prevent torn reads");

        let final_state = load_state();
        assert!(final_state
            .installed_hash
            .as_deref()
            .map(|h| h.starts_with("hash-"))
            .unwrap_or(false));
    }

    #[test]
    fn sha256_detects_content_change_at_same_path() {
        let td = fresh_root();
        let p = td.path().join("bin");
        std::fs::write(&p, b"v1-bytes").unwrap();
        let h1 = sha256_file(&p).unwrap();
        std::fs::write(&p, b"v2-different-bytes").unwrap();
        let h2 = sha256_file(&p).unwrap();
        assert_ne!(h1, h2);
        // Stable: rewriting same bytes yields same hash.
        std::fs::write(&p, b"v1-bytes").unwrap();
        assert_eq!(h1, sha256_file(&p).unwrap());
    }

    #[test]
    fn copy_atomic_sets_executable_bit_and_replaces_existing() {
        use std::os::unix::fs::PermissionsExt;
        let td = fresh_root();
        let src = td.path().join("src");
        let dst = td.path().join("dst");
        std::fs::write(&src, b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::write(&dst, b"old-content").unwrap();

        copy_atomic(&src, &dst).expect("copy");
        assert_eq!(std::fs::read(&dst).unwrap(), b"#!/bin/sh\necho hi\n");
        let mode = std::fs::metadata(&dst).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "executable bit should be set");

        // No leftover .tmp file.
        assert!(!td.path().join("tmp").exists());
        assert!(!std::fs::read_dir(td.path()).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }

    #[test]
    fn kill_pidfile_returns_true_when_no_pidfile() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());
        // Pristine config root → no pidfile.
        assert!(kill_pidfile_daemon());
    }

    #[test]
    fn kill_pidfile_cleans_stale_entry_when_pid_already_dead() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let td = fresh_root();
        set_root(td.path());
        // Spawn + immediately kill so we have a known-dead PID.
        let port = pick_port();
        let pf = pidfile_path();
        let mut child = spawn_stub(&pf, port, false);
        let dead_pid = child.id();
        let _ = child.kill();
        let _ = child.wait();
        // Re-write the pidfile so kill sees the stale PID (kill_pidfile clears
        // it on responsive exit; we intentionally restore it).
        std::fs::write(&pf, dead_pid.to_string()).unwrap();
        assert!(kill_pidfile_daemon(), "stale pidfile should be cleaned up");
        assert!(read_pidfile().is_none());
    }
}

// ─── Linux end-to-end tests of the orchestrator (Scope B) ────────────────────
//
// These exercise the full ensure_daemon_runtime_ready state machine against a
// Python stub that mimics the contract the orchestrator actually relies on:
//   • binds 127.0.0.1:$port
//   • writes its PID to $SKILL_DAEMON_CONFIG_ROOT/daemon.pid
//   • answers GET /v1/version with PROTOCOL_VERSION=1
//
// We don't use the real skill-daemon binary because it pulls llama-cpp-sys
// (libclang/bindgen/several minutes of C++ compile) which is out of scope for
// upgrade-flow validation. The orchestrator never inspects daemon behavior
// beyond /v1/version, so a stub gives identical coverage with a 5s test run.
#[cfg(all(test, target_os = "linux"))]
mod orchestrator_linux_e2e {
    use super::*;
    use std::process::Command;
    use std::sync::Mutex;
    use std::time::Instant;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// Python HTTP stub that satisfies the orchestrator's protocol contract.
    /// Reads $SKILL_DAEMON_ADDR + $SKILL_DAEMON_CONFIG_ROOT (set by spawn).
    const DAEMON_STUB: &str = r#"#!/usr/bin/env python3
import os, sys, signal, socket
from http.server import BaseHTTPRequestHandler, HTTPServer

addr = os.environ.get("SKILL_DAEMON_ADDR", "127.0.0.1:18444")
host, port = addr.rsplit(":", 1); port = int(port)
cfg_root = os.environ.get("SKILL_DAEMON_CONFIG_ROOT", "/tmp")
os.makedirs(cfg_root, exist_ok=True)
with open(os.path.join(cfg_root, "daemon.pid"), "w") as f:
    f.write(str(os.getpid()))

class H(BaseHTTPRequestHandler):
    def log_message(self, *a, **k): pass  # silence access log
    def do_GET(self):
        if self.path == "/v1/version":
            body = b'{"daemon":"skill-daemon","protocol_version":1,"daemon_version":"stub-1.0"}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404); self.end_headers()

srv = HTTPServer((host, port), H)
signal.signal(signal.SIGTERM, lambda *_: (srv.shutdown(), srv.server_close(), sys.exit(0)))
srv.serve_forever()
"#;

    fn write_stub_daemon(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("stub-daemon");
        std::fs::write(&p, DAEMON_STUB).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn pick_port() -> u16 {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p = l.local_addr().unwrap().port();
        drop(l);
        p
    }

    /// Set up an isolated config root + daemon address + auth token. The
    /// orchestrator and daemon both honor SKILL_DAEMON_CONFIG_ROOT for state,
    /// pidfile, and auth.token.
    struct E2eEnv {
        _root: tempfile::TempDir,
        root_path: PathBuf,
        port: u16,
        token: String,
    }

    impl E2eEnv {
        fn new() -> Self {
            let _root = tempfile::tempdir().unwrap();
            let root_path = _root.path().to_path_buf();
            let port = pick_port();
            let token = "e2e-test-token".to_string();

            std::env::set_var("SKILL_DAEMON_CONFIG_ROOT", &root_path);
            std::env::set_var("SKILL_DAEMON_ADDR", format!("127.0.0.1:{port}"));
            std::env::set_var("SKILL_DAEMON_TOKEN", &token);
            // Skip OS-service install in the test (no systemd in container).
            std::env::set_var("SKILL_DAEMON_SERVICE_AUTOINSTALL", "0");

            // Pre-write the auth token at the path the orchestrator reads.
            std::fs::write(root_path.join("auth.token"), format!("{token}\n")).unwrap();

            Self {
                _root,
                root_path,
                port,
                token,
            }
        }

        fn set_bundled(&self, path: &Path) {
            std::env::set_var("SKILL_DAEMON_BIN", path);
        }

        fn cleanup_daemon(&self) {
            // Best-effort: kill whatever the orchestrator left running on our
            // port, so the next test doesn't see a stale process.
            let _ = kill_pidfile_daemon();
            kill_port_owner(self.port);
            // Give kernel time to release the port.
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    impl Drop for E2eEnv {
        fn drop(&mut self) {
            self.cleanup_daemon();
            std::env::remove_var("SKILL_DAEMON_CONFIG_ROOT");
            std::env::remove_var("SKILL_DAEMON_ADDR");
            std::env::remove_var("SKILL_DAEMON_TOKEN");
            std::env::remove_var("SKILL_DAEMON_BIN");
            std::env::remove_var("SKILL_DAEMON_SERVICE_AUTOINSTALL");
        }
    }

    /// Write a wrapper script (different SHA256, identical behavior) that
    /// execs into the stub daemon. Used to simulate "new version installed".
    fn write_wrapper(dir: &Path, name: &str, label: &str, stub: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(
            &p,
            format!(
                "#!/usr/bin/env bash\n# wrapper: {label}\nexec {} \"$@\"\n",
                stub.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// A wrapper that exits 1 immediately — used to simulate a broken
    /// upgrade where the new daemon binary fails to come up.
    fn write_broken_wrapper(dir: &Path, name: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join(name);
        std::fs::write(&p, "#!/usr/bin/env bash\nexit 1\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    fn wait_for_state_phase(deadline: Instant, want: Phase) -> UpgradeState {
        loop {
            let s = load_state();
            if s.phase == want {
                return s;
            }
            if Instant::now() > deadline {
                return s;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    #[test]
    fn fresh_install_sets_state_ready_and_records_hash() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = E2eEnv::new();
        let stub = write_stub_daemon(&env.root_path);

        // Bundled = a wrapper around the stub. The wrapper gives us a stable,
        // hashable artifact distinct from the stub itself, so swapping
        // wrappers later (in_place_upgrade test) reliably triggers an upgrade.
        let bundled = write_wrapper(&env.root_path, "bundled", "v1", &stub);
        env.set_bundled(&bundled);
        let bundled_hash = sha256_file(&bundled).unwrap();

        crate::daemon_cmds::ensure_daemon_runtime_ready();

        let state = load_state();
        assert_eq!(
            state.phase,
            Phase::Ready,
            "phase should be Ready on success"
        );
        assert_eq!(
            state.installed_hash.as_deref(),
            Some(bundled_hash.as_str()),
            "installed_hash should match the bundled binary"
        );
        assert!(
            state.rollback_hash.is_some(),
            "rollback snapshot should have been written"
        );

        // Daemon really is answering on the configured port.
        let url = format!("http://127.0.0.1:{}/v1/version", env.port);
        let out = Command::new("curl")
            .args([
                "-sf",
                "-H",
                &format!("Authorization: Bearer {}", env.token),
                &url,
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "/v1/version should respond");
        let body = String::from_utf8_lossy(&out.stdout);
        assert!(body.contains("protocol_version"), "body: {body}");
    }

    #[test]
    fn in_place_upgrade_swaps_installed_hash_and_keeps_daemon_alive() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = E2eEnv::new();
        let stub = write_stub_daemon(&env.root_path);

        // First launch: v1 wrapper.
        let v1 = write_wrapper(&env.root_path, "bundled-v1", "v1", &stub);
        env.set_bundled(&v1);
        let v1_hash = sha256_file(&v1).unwrap();
        crate::daemon_cmds::ensure_daemon_runtime_ready();
        assert_eq!(
            load_state().installed_hash.as_deref(),
            Some(v1_hash.as_str())
        );

        // Second launch: bundled binary content has changed (different label
        // → different SHA256). The orchestrator must detect, kill v1, spawn
        // v2, and update installed_hash + rollback_hash to v2.
        let v2 = write_wrapper(&env.root_path, "bundled-v2", "v2", &stub);
        env.set_bundled(&v2);
        let v2_hash = sha256_file(&v2).unwrap();
        assert_ne!(v1_hash, v2_hash, "wrappers must hash differently");

        crate::daemon_cmds::ensure_daemon_runtime_ready();

        let state = load_state();
        assert_eq!(state.phase, Phase::Ready);
        assert_eq!(state.installed_hash.as_deref(), Some(v2_hash.as_str()));
        assert_eq!(state.rollback_hash.as_deref(), Some(v2_hash.as_str()));
    }

    #[test]
    fn broken_bundled_falls_back_to_rollback() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = E2eEnv::new();
        let stub = write_stub_daemon(&env.root_path);

        // Phase 1 — establish a known-good rollback snapshot via a fresh install.
        let v1 = write_wrapper(&env.root_path, "bundled-v1", "v1", &stub);
        env.set_bundled(&v1);
        let v1_hash = sha256_file(&v1).unwrap();
        crate::daemon_cmds::ensure_daemon_runtime_ready();
        assert_eq!(
            load_state().rollback_hash.as_deref(),
            Some(v1_hash.as_str())
        );

        // Phase 2 — point bundled at a broken script (exit 1). The orchestrator
        // should fail twice, then roll back to the v1 snapshot and end Ready.
        let broken = write_broken_wrapper(&env.root_path, "bundled-broken");
        env.set_bundled(&broken);
        crate::daemon_cmds::ensure_daemon_runtime_ready();

        let state = wait_for_state_phase(Instant::now() + Duration::from_secs(3), Phase::Ready);
        assert_eq!(state.phase, Phase::Ready, "rollback should restore Ready");
        assert_eq!(
            state.installed_hash.as_deref(),
            Some(v1_hash.as_str()),
            "installed_hash should be the rollback snapshot's hash"
        );
        // Daemon really is the rolled-back one.
        let url = format!("http://127.0.0.1:{}/v1/version", env.port);
        let out = Command::new("curl")
            .args([
                "-sf",
                "-H",
                &format!("Authorization: Bearer {}", env.token),
                &url,
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
    }

    #[test]
    fn terminal_failure_when_no_rollback_and_bundled_broken() {
        // unwrap_or_else recovers a poisoned lock so one panicking test
        // doesn't cascade-fail every test that follows.
        let _g = env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let env = E2eEnv::new();

        // Bundled is broken from the start AND there is no rollback snapshot.
        let broken = write_broken_wrapper(&env.root_path, "bundled-broken");
        env.set_bundled(&broken);

        crate::daemon_cmds::ensure_daemon_runtime_ready();

        let state = load_state();
        assert_eq!(state.phase, Phase::Failed, "phase should be Failed");
        assert!(state.last_error.is_some(), "last_error should be populated");
        // Nothing should be bound to the port.
        assert!(wait_for_port_free(env.port, Duration::from_secs(1)));
    }
}
