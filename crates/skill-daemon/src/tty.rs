// SPDX-License-Identifier: GPL-3.0-only
//! PTY proxy for transparent terminal session recording.
//!
//! Spawns the user's shell on a new PTY pair and proxies stdin/stdout
//! between the controlling terminal and that PTY, while writing every byte
//! the shell produces to a log file. Unlike macOS's `script(1)`, this shim
//! correctly forwards SIGWINCH so TUI applications (vim, htop, claude code,
//! etc.) see resize events and re-render at the right size.
//!
//! Invoked as `skill-daemon tty <log-file>`. Runs synchronously, before
//! the daemon's tokio runtime is initialised.

#![cfg(unix)]

use std::ffi::CString;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the SIGWINCH handler; checked by the main I/O loop.
static SIGWINCH: AtomicBool = AtomicBool::new(false);
/// Set by SIGCHLD; main loop exits when set.
static SIGCHLD: AtomicBool = AtomicBool::new(false);

extern "C" fn on_sigwinch(_: libc::c_int) {
    SIGWINCH.store(true, Ordering::Relaxed);
}
extern "C" fn on_sigchld(_: libc::c_int) {
    SIGCHLD.store(true, Ordering::Relaxed);
}

/// Run the shim. Optional arg overrides the log path; otherwise the daemon
/// picks one inside `~/.skill/terminal-logs/`.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let log_path = match args.first() {
        Some(p) => std::path::PathBuf::from(p),
        None => default_log_path()?,
    };
    rotate_logs(log_path.parent(), &log_path);

    // BufWriter cuts syscalls per PTY-read from ~1 to ~0 (8 KB chunks fit
    // most TUI redraw bursts). Final flush happens via Drop, but we also
    // call .flush() explicitly before exit to surface I/O errors.
    let log_file = OpenOptions::new().create(true).append(true).open(&log_path)?;
    let mut log = BufWriter::with_capacity(8192, log_file);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());

    let stdin_fd = libc::STDIN_FILENO;
    let stdout_fd = libc::STDOUT_FILENO;

    // Initial winsize from the controlling tty.
    let mut winsize: libc::winsize = unsafe { std::mem::zeroed() };
    unsafe { libc::ioctl(stdout_fd, libc::TIOCGWINSZ as _, &mut winsize) };
    if winsize.ws_col == 0 {
        winsize.ws_col = 80;
    }
    if winsize.ws_row == 0 {
        winsize.ws_row = 24;
    }

    // Snapshot termios so we can restore it on exit.
    let mut original_termios: libc::termios = unsafe { std::mem::zeroed() };
    if unsafe { libc::tcgetattr(stdin_fd, &mut original_termios) } != 0 {
        return Err(anyhow::anyhow!("tcgetattr(stdin) failed: {}", last_err()));
    }

    // openpty() returns a master/slave pair. Caller owns both FDs.
    let mut master_fd: RawFd = -1;
    let mut slave_fd: RawFd = -1;
    let rc = unsafe {
        libc::openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut winsize,
        )
    };
    if rc != 0 {
        return Err(anyhow::anyhow!("openpty failed: {}", last_err()));
    }

    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(anyhow::anyhow!("fork failed: {}", last_err()));
    }

    if pid == 0 {
        // ── child: become session leader, attach slave as ctty, exec shell ──
        unsafe { libc::close(master_fd) };
        unsafe { libc::setsid() };
        // TIOCSCTTY makes `slave_fd` the controlling terminal of the new session.
        unsafe { libc::ioctl(slave_fd, libc::TIOCSCTTY as _, 0) };
        unsafe {
            libc::dup2(slave_fd, libc::STDIN_FILENO);
            libc::dup2(slave_fd, libc::STDOUT_FILENO);
            libc::dup2(slave_fd, libc::STDERR_FILENO);
            if slave_fd > 2 {
                libc::close(slave_fd);
            }
        }
        // Mark this shell as the wrapped one; the hook checks this env var.
        unsafe { libc::setenv(c"NEUROSKILL_RECORDING".as_ptr(), c"1".as_ptr(), 1) };

        // Login-style argv0: prefix with `-` so the shell reads its rc files.
        let basename = std::path::Path::new(&shell)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "shell".into());
        let argv0 = CString::new(format!("-{basename}")).unwrap();
        let path_c = CString::new(shell.clone()).unwrap();
        unsafe {
            libc::execlp(path_c.as_ptr(), argv0.as_ptr(), std::ptr::null::<libc::c_char>());
        }
        // execlp only returns on failure.
        let _ = std::io::stderr().write_all(b"skill-daemon tty: exec failed\n");
        unsafe { libc::_exit(127) };
    }

    // ── parent ──
    unsafe { libc::close(slave_fd) };

    // Emit OSC 2 immediately so the terminal tab/window title shows the cwd
    // instead of our argv (which would otherwise expose the log file path).
    // The inner shell's precmd will keep updating this on each prompt; this
    // line just covers the brief gap between exec and the first prompt.
    {
        let cwd = std::env::current_dir().unwrap_or_default();
        let cwd_str = cwd.to_string_lossy();
        let home = std::env::var("HOME").unwrap_or_default();
        let display = if !home.is_empty() && cwd_str.starts_with(&home) {
            format!("~{}", &cwd_str[home.len()..])
        } else {
            cwd_str.into_owned()
        };
        let osc = format!("\x1b]2;{display}\x07");
        let _ = write_all_fd(stdout_fd, osc.as_bytes());
    }

    // Restore original termios on every exit path.
    struct TermiosGuard(libc::termios);
    impl Drop for TermiosGuard {
        fn drop(&mut self) {
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &self.0) };
        }
    }
    let _termios_guard = TermiosGuard(original_termios);

    // Put stdin in raw mode so every byte (incl. Ctrl-C, escape sequences,
    // bracketed paste, mouse events) reaches the slave PTY untouched.
    let mut raw = original_termios;
    unsafe { libc::cfmakeraw(&mut raw) };
    unsafe { libc::tcsetattr(stdin_fd, libc::TCSAFLUSH, &raw) };

    install_signal_handler(libc::SIGWINCH, on_sigwinch)?;
    install_signal_handler(libc::SIGCHLD, on_sigchld)?;

    // Main I/O loop: select() on stdin and master_fd. Forward bytes both
    // ways. On SIGWINCH, re-query the outer terminal's size and apply it
    // to the master end of the PTY (which raises SIGWINCH inside the child).
    let mut buf = [0u8; 8192];
    loop {
        if SIGWINCH.swap(false, Ordering::Relaxed) {
            let mut new_size: libc::winsize = unsafe { std::mem::zeroed() };
            if unsafe { libc::ioctl(stdout_fd, libc::TIOCGWINSZ as _, &mut new_size) } == 0 {
                unsafe { libc::ioctl(master_fd, libc::TIOCSWINSZ as _, &new_size) };
            }
        }
        if SIGCHLD.load(Ordering::Relaxed) {
            // Drain any remaining output from the master before quitting.
            drain_master(master_fd, &mut buf, stdout_fd, &mut log);
            break;
        }

        let mut readfds: libc::fd_set = unsafe { std::mem::zeroed() };
        unsafe {
            libc::FD_ZERO(&mut readfds);
            libc::FD_SET(stdin_fd, &mut readfds);
            libc::FD_SET(master_fd, &mut readfds);
        }
        let nfds = master_fd.max(stdin_fd) + 1;
        let r = unsafe {
            libc::select(
                nfds,
                &mut readfds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if r < 0 {
            let errno = std::io::Error::last_os_error().raw_os_error();
            if errno == Some(libc::EINTR) {
                continue; // SIGWINCH/SIGCHLD interrupted us — handled at top of loop
            }
            break;
        }

        if unsafe { libc::FD_ISSET(stdin_fd, &readfds) } {
            let n = unsafe { libc::read(stdin_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            let _ = write_all_fd(master_fd, &buf[..n as usize]);
        }
        if unsafe { libc::FD_ISSET(master_fd, &readfds) } {
            let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            let bytes = &buf[..n as usize];
            let _ = write_all_fd(stdout_fd, bytes);
            let _ = log.write_all(bytes);
        }
    }

    let _ = log.flush();

    let mut status: libc::c_int = 0;
    unsafe { libc::waitpid(pid, &mut status, 0) };
    let exit_code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        1
    };
    drop(_termios_guard); // explicit so it runs before process::exit
    std::process::exit(exit_code);
}

fn install_signal_handler(sig: libc::c_int, handler: extern "C" fn(libc::c_int)) -> anyhow::Result<()> {
    let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
    action.sa_sigaction = handler as usize;
    action.sa_flags = libc::SA_RESTART;
    unsafe { libc::sigemptyset(&mut action.sa_mask) };
    let rc = unsafe { libc::sigaction(sig, &action, std::ptr::null_mut()) };
    if rc != 0 {
        return Err(anyhow::anyhow!("sigaction({sig}) failed: {}", last_err()));
    }
    Ok(())
}

fn write_all_fd(fd: RawFd, mut bytes: &[u8]) -> std::io::Result<()> {
    while !bytes.is_empty() {
        let n = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return Err(err);
        }
        bytes = &bytes[n as usize..];
    }
    Ok(())
}

fn drain_master<W: Write>(master_fd: RawFd, buf: &mut [u8], stdout_fd: RawFd, log: &mut W) {
    // Make master non-blocking so we can drain whatever's pending.
    let flags = unsafe { libc::fcntl(master_fd, libc::F_GETFL) };
    if flags >= 0 {
        unsafe { libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    }
    loop {
        let n = unsafe { libc::read(master_fd, buf.as_mut_ptr().cast(), buf.len()) };
        if n <= 0 {
            break;
        }
        let bytes = &buf[..n as usize];
        let _ = write_all_fd(stdout_fd, bytes);
        let _ = log.write_all(bytes);
    }
}

fn last_err() -> String {
    std::io::Error::last_os_error().to_string()
}

/// Default log path: `~/.skill/terminal-logs/<YYYYMMDD-HHMMSS>-<pid>.log`.
fn default_log_path() -> anyhow::Result<std::path::PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("no $HOME"))?;
    let dir = home.join(".skill").join("terminal-logs");
    std::fs::create_dir_all(&dir)?;
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let pid = std::process::id();
    Ok(dir.join(format!("{ts}-{pid}.log")))
}

/// Compress finished logs (whose PID is no longer alive) to `.log.zst`,
/// then enforce a 100-file retention cap on the combined `.log`/`.log.zst`
/// set. Skips `current_log` so we never touch the file we're about to write.
fn rotate_logs(dir: Option<&std::path::Path>, current_log: &std::path::Path) {
    let Some(dir) = dir else { return };

    // Phase 1: compress every uncompressed log whose owning PID has exited.
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path == current_log {
            continue;
        }
        if path.extension().is_none_or(|e| e != "log") {
            continue;
        }
        if pid_alive_for_log(&path) {
            continue; // another shim is still appending
        }
        let _ = compress_to_zst(&path);
    }

    // Phase 2: enforce retention. Compressed logs are tiny so we can keep
    // many more than the old uncompressed cap.
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut all: Vec<(std::path::PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|ext| ext == "log" || ext == "zst")
        })
        .filter_map(|e| Some((e.path(), e.metadata().ok()?.modified().ok()?)))
        .collect();
    all.sort_by_key(|(_, m)| std::cmp::Reverse(*m));
    for (path, _) in all.into_iter().skip(100) {
        let _ = std::fs::remove_file(path);
    }
}

/// Filenames are `<YYYYMMDD-HHMMSS>-<pid>.log` — extract the PID and check
/// whether that process still exists with `kill(pid, 0)`.
fn pid_alive_for_log(path: &std::path::Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return false;
    };
    let Some(pid_str) = stem.rsplit('-').next() else {
        return false;
    };
    let Ok(pid) = pid_str.parse::<libc::pid_t>() else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // kill(pid, 0) returns 0 when the signal could be delivered (process
    // exists and we have permission). Errno ESRCH means it's gone.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Stream-compress `src` into a sibling `<src>.zst`, then delete `src`.
/// Compression level 3 (zstd default) is fast on CPU and still yields ~10×
/// reduction for ANSI-heavy terminal output.
fn compress_to_zst(src: &std::path::Path) -> std::io::Result<()> {
    let dst = src.with_extension("log.zst");
    let input = std::fs::File::open(src)?;
    let output = std::fs::File::create(&dst)?;
    let mut encoder = zstd::Encoder::new(output, 3)?;
    let mut reader = std::io::BufReader::new(input);
    std::io::copy(&mut reader, &mut encoder)?;
    encoder.finish()?;
    std::fs::remove_file(src)?;
    Ok(())
}
