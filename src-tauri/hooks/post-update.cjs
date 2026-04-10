// Cross-platform post-update hook to restart the skill-daemon
const { execSync } = require('child_process');
const { platform } = require('os');
const path = require('path');
const fs = require('fs');

console.log('[post-update] Restarting skill-daemon...');

try {
  if (platform() === 'darwin') {
    // macOS: Load LaunchAgent
    const home = process.env.HOME || (process.env.USERPROFILE && process.env.USERPROFILE.replace(/\\/g, '/'));
    const plistDest = path.join(home, 'Library/LaunchAgents/com.neuroskill.skill-daemon.plist');
    
    // Copy plist from app bundle if not exists
    const appResources = path.join(
      path.dirname(process.execPath),
      '..',
      'Resources',
      'com.neuroskill.skill-daemon.plist'
    );
    
    if (fs.existsSync(appResources) && !fs.existsSync(plistDest)) {
      const launchAgentsDir = path.join(home, 'Library/LaunchAgents');
      if (!fs.existsSync(launchAgentsDir)) {
        fs.mkdirSync(launchAgentsDir, { recursive: true });
      }
      fs.copyFileSync(appResources, plistDest);
      console.log('[post-update] Copied LaunchAgent plist to ' + plistDest);
    }
    
    // Load the LaunchAgent (ignore errors if already loaded)
    try {
      execSync('launchctl load -w ' + plistDest);
      console.log('[post-update] Loaded macOS LaunchAgent');
    } catch (loadError) {
      console.log('[post-update] LaunchAgent already loaded or failed to load:', loadError.message);
    }
  } else if (platform() === 'win32') {
    // Windows: Start the service
    const daemonPath = path.join(path.dirname(process.execPath), 'skill-daemon.exe');
    if (fs.existsSync(daemonPath)) {
      execSync('sc start skill-daemon 2>nul || start "" "' + daemonPath + '"', { shell: true });
      console.log('[post-update] Started Windows service');
    }
  } else {
    // Linux: Start systemd service
    execSync('systemctl --user restart skill-daemon 2>/dev/null || nohup skill-daemon >/dev/null 2>&1 &', { shell: '/bin/bash' });
    console.log('[post-update] Started Linux service');
  }
} catch (error) {
  console.error('[post-update] Failed to restart daemon:', error.message);
}

console.log('[post-update] Daemon restart triggered.');
