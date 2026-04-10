// Cross-platform pre-update hook to stop the skill-daemon
const { execSync } = require('child_process');
const { platform } = require('os');

console.log('[pre-update] Stopping skill-daemon...');

try {
  if (platform() === 'darwin') {
    // macOS: Unload LaunchAgent
    execSync('launchctl unload ~/Library/LaunchAgents/com.neuroskill.skill-daemon.plist 2>/dev/null || true');
    console.log('[pre-update] Stopped macOS LaunchAgent');
  } else if (platform() === 'win32') {
    // Windows: Stop the service
    execSync('sc stop skill-daemon 2>nul || net stop skill-daemon 2>nul || taskkill /F /IM skill-daemon.exe 2>nul', { shell: true });
    console.log('[pre-update] Stopped Windows service');
  } else {
    // Linux: Stop systemd service
    execSync('systemctl --user stop skill-daemon 2>/dev/null || pkill -f skill-daemon 2>/dev/null || true', { shell: '/bin/bash' });
    console.log('[pre-update] Stopped Linux service');
  }
} catch (error) {
  console.error('[pre-update] Failed to stop daemon:', error.message);
}

console.log('[pre-update] Daemon stopped (or not running).');
