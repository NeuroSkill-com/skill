// Pre-update hook: stop the running skill-daemon so the updater can replace
// the binary in place. Companion to the failsafe upgrade flow in
// src-tauri/src/daemon_upgrade.rs — kill by pidfile (precise) and unload the
// OS service WITHOUT `-w` so the plist stays enabled and the next launch can
// re-bootstrap it.

const { execSync } = require('child_process');
const { platform } = require('os');
const path = require('path');
const fs = require('fs');

const home = process.env.HOME || process.env.USERPROFILE || '';

function configRoot() {
  if (platform() === 'darwin') return path.join(home, 'Library/Application Support/skill/daemon');
  if (platform() === 'win32') return path.join(process.env.APPDATA || home, 'skill', 'daemon');
  return path.join(process.env.XDG_CONFIG_HOME || path.join(home, '.config'), 'skill', 'daemon');
}

function readPid() {
  try {
    const txt = fs.readFileSync(path.join(configRoot(), 'daemon.pid'), 'utf8').trim();
    const pid = parseInt(txt, 10);
    return Number.isFinite(pid) && pid > 0 ? pid : null;
  } catch {
    return null;
  }
}

function killByPid(pid) {
  if (!pid) return false;
  try {
    if (platform() === 'win32') {
      execSync(`taskkill /PID ${pid} /T`, { stdio: 'ignore' });
    } else {
      // SIGTERM, then SIGKILL after a short grace.
      try { process.kill(pid, 'SIGTERM'); } catch {}
      const deadline = Date.now() + 3000;
      while (Date.now() < deadline) {
        try { process.kill(pid, 0); } catch { return true; }
        execSync('sleep 0.1');
      }
      try { process.kill(pid, 'SIGKILL'); } catch {}
    }
    return true;
  } catch {
    return false;
  }
}

console.log('[pre-update] stopping skill-daemon...');

try {
  if (platform() === 'darwin') {
    const plist = path.join(home, 'Library/LaunchAgents/com.skill.daemon.plist');
    if (fs.existsSync(plist)) {
      // bootout cleanly stops the service without disabling the plist
      // (which `unload -w` would do, breaking next-boot autostart).
      try {
        const uid = execSync('id -u').toString().trim();
        execSync(`launchctl bootout gui/${uid} ${plist} 2>/dev/null || launchctl unload ${plist} 2>/dev/null || true`);
      } catch {}
    }
    killByPid(readPid());
    console.log('[pre-update] macOS: launchd unloaded, daemon stopped.');
  } else if (platform() === 'win32') {
    try { execSync('sc stop skill-daemon', { stdio: 'ignore' }); } catch {}
    killByPid(readPid());
    console.log('[pre-update] Windows: service stopped, daemon killed.');
  } else {
    try { execSync('systemctl --user stop skill-daemon.service', { stdio: 'ignore' }); } catch {}
    killByPid(readPid());
    console.log('[pre-update] Linux: systemd stopped, daemon killed.');
  }
} catch (error) {
  console.error('[pre-update] error stopping daemon:', error.message);
}

console.log('[pre-update] done.');
