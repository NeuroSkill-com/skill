// Post-update hook: after the updater replaces the app bundle, mark the
// upgrade state as `pending` so the next app launch re-runs the failsafe
// upgrade flow (src-tauri/src/daemon_upgrade.rs) cleanly. We deliberately do
// NOT spawn the daemon here — the app does that on launch with full state
// tracking, hash verification, and rollback support.

const { platform } = require('os');
const path = require('path');
const fs = require('fs');

const home = process.env.HOME || process.env.USERPROFILE || '';

function configRoot() {
  if (platform() === 'darwin') return path.join(home, 'Library/Application Support/skill/daemon');
  if (platform() === 'win32') return path.join(process.env.APPDATA || home, 'skill', 'daemon');
  return path.join(process.env.XDG_CONFIG_HOME || path.join(home, '.config'), 'skill', 'daemon');
}

console.log('[post-update] preparing failsafe upgrade flow...');

try {
  const root = configRoot();
  fs.mkdirSync(root, { recursive: true });
  const statePath = path.join(root, 'state.json');

  // Load current state (may be missing on fresh installs).
  let state = {};
  try {
    state = JSON.parse(fs.readFileSync(statePath, 'utf8'));
  } catch {
    state = { version: 1, phase: 'ready' };
  }

  // Force a re-check on next launch by clearing the installed_hash so the
  // hash comparison fails and the state machine enters Upgrading.
  state.installed_hash = null;
  state.phase = 'upgrading';
  state.attempt_count = 0;
  state.last_error = null;
  state.updated_at = new Date().toISOString();

  fs.writeFileSync(statePath + '.tmp', JSON.stringify(state, null, 2));
  fs.renameSync(statePath + '.tmp', statePath);

  console.log('[post-update] state.json marked for upgrade — next app launch will reconcile.');
} catch (error) {
  console.error('[post-update] failed to prep state:', error.message);
}
