// SPDX-License-Identifier: GPL-3.0-only
//! Tests for bash and path safety checks.

use skill_tools::exec::{check_bash_safety, check_path_safety};
use std::path::Path;

// ── Bash safety — dangerous patterns ─────────────────────────────────────────

#[test]
fn detects_rm_command() {
    assert!(check_bash_safety("rm -rf /tmp/foo").is_some());
    assert!(check_bash_safety("rm file.txt").is_some());
}

#[test]
fn detects_sudo() {
    assert!(check_bash_safety("sudo apt install foo").is_some());
}

#[test]
fn detects_dd() {
    assert!(check_bash_safety("dd if=/dev/zero of=/dev/sda").is_some());
}

#[test]
fn detects_fork_bomb() {
    assert!(check_bash_safety(":(){ :|:& };:").is_some());
}

#[test]
fn detects_shutdown() {
    assert!(check_bash_safety("shutdown -h now").is_some());
    assert!(check_bash_safety("reboot").is_some());
}

#[test]
fn detects_sensitive_paths() {
    assert!(check_bash_safety("cat > /etc/passwd").is_some());
    assert!(check_bash_safety("echo x > /boot/grub.cfg").is_some());
}

#[test]
fn detects_kill_commands() {
    assert!(check_bash_safety("kill -9 1234").is_some());
    assert!(check_bash_safety("killall firefox").is_some());
    assert!(check_bash_safety("pkill chrome").is_some());
}

#[test]
fn detects_chmod_chown() {
    assert!(check_bash_safety("chmod 777 /tmp/file").is_some());
    assert!(check_bash_safety("chown root:root /tmp/file").is_some());
}

// ── Bash safety — safe commands ──────────────────────────────────────────────

#[test]
fn allows_safe_commands() {
    assert!(check_bash_safety("ls -la").is_none());
    assert!(check_bash_safety("cat file.txt").is_none());
    assert!(check_bash_safety("echo hello world").is_none());
    assert!(check_bash_safety("grep -r pattern src/").is_none());
    assert!(check_bash_safety("cargo test").is_none());
    assert!(check_bash_safety("git status").is_none());
}

#[test]
fn no_false_positive_on_skill() {
    // "skill" contains "kill" but should NOT trigger
    assert!(check_bash_safety("cargo test -p skill").is_none());
    assert!(check_bash_safety("./skill --version").is_none());
}

#[test]
fn no_false_positive_on_embedded_words() {
    // "rmdir" is dangerous but "mkdir" shouldn't trigger (no boundary before "rmdir")
    // "kill" in "skill" shouldn't trigger
    assert!(check_bash_safety("skill --version").is_none());
    // "rm " embedded after non-boundary char
    assert!(check_bash_safety("inform user").is_none());
}

#[test]
fn dangerous_in_pipe() {
    assert!(check_bash_safety("echo foo | rm -rf /").is_some());
    assert!(check_bash_safety("ls && sudo rm -rf /").is_some());
}

#[test]
fn dangerous_in_subshell() {
    assert!(check_bash_safety("$(sudo whoami)").is_some());
    assert!(check_bash_safety("echo `rm file`").is_some());
}

// ── Path safety ──────────────────────────────────────────────────────────────

#[test]
fn sensitive_paths_detected() {
    assert!(check_path_safety(Path::new("/etc/passwd")).is_some());
    assert!(check_path_safety(Path::new("/boot/grub.cfg")).is_some());
    assert!(check_path_safety(Path::new("/usr/bin/python")).is_some());
    assert!(check_path_safety(Path::new("/var/log/syslog")).is_some());
    assert!(check_path_safety(Path::new("/proc/1/status")).is_some());
    assert!(check_path_safety(Path::new("/sys/class/net")).is_some());
}

#[test]
fn safe_paths_allowed() {
    assert!(check_path_safety(Path::new("/tmp/test.txt")).is_none());
    assert!(check_path_safety(Path::new("/home/user/file.rs")).is_none());
    assert!(check_path_safety(Path::new("relative/path.txt")).is_none());
    assert!(check_path_safety(Path::new("./local")).is_none());
}
