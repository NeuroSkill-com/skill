/**
 * Playwright e2e tests for the /settings page.
 *
 * Verifies that the settings page renders, all tabs are clickable,
 * and each tab renders its content without errors.
 *
 * Run:  npx playwright test src/tests/settings.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";

// ── Tauri IPC mock ───────────────────────────────────────────────────────────

function buildMockScript() {
  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };

    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      switch (cmd) {
        // ── Goals tab ────────────────────────────────────────────────────
        case "get_daily_goal":
          return Promise.resolve(30);
        case "get_daily_recording_mins":
          return Promise.resolve([["2026-03-25", 45], ["2026-03-26", 20], ["2026-03-27", 60]]);

        // ── Devices tab ──────────────────────────────────────────────────
        case "get_devices":
          return Promise.resolve([]);
        case "get_scanner_config":
          return Promise.resolve({ scan_duration_secs: 10, auto_connect: true });
        case "get_device_log":
          return Promise.resolve([]);
        case "is_session_live":
          return Promise.resolve(false);
        case "get_device_api_config":
          return Promise.resolve({ enabled: false, port: 8376 });
        case "get_cortex_ws_state":
          return Promise.resolve({ connected: false, headset_id: null });

        // ── ExG tab ──────────────────────────────────────────────────────
        case "get_openbci_config":
          return Promise.resolve({
            board: "ganglion",
            serial_port: "",
            wifi_shield_ip: "",
            wifi_port: 3000,
            scan_timeout_secs: 10,
          });
        case "list_serial_ports":
          return Promise.resolve([]);

        // ── Settings tab (General) ───────────────────────────────────────
        case "get_data_dir":
          return Promise.resolve(["/Users/test/.skill", "/Users/test/.skill"]);
        case "get_storage_format":
          return Promise.resolve("parquet");
        case "get_autostart_enabled":
          return Promise.resolve(false);
        case "get_main_window_auto_fit":
          return Promise.resolve(true);
        case "get_log_config":
          return Promise.resolve({ level: "info", file_logging: true });
        case "get_location_enabled":
          return Promise.resolve(false);

        // ── Appearance tab ───────────────────────────────────────────────
        case "get_settings":
          return Promise.resolve({});

        // ── Shortcuts tab ────────────────────────────────────────────────
        case "get_api_shortcut":
        case "get_calibration_shortcut":
        case "get_focus_timer_shortcut":
        case "get_help_shortcut":
        case "get_history_shortcut":
        case "get_label_shortcut":
        case "get_search_shortcut":
        case "get_settings_shortcut":
        case "get_theme_shortcut":
          return Promise.resolve("CmdOrCtrl+Shift+S");

        // ── LLM tab ─────────────────────────────────────────────────────
        case "get_llm_config":
          return Promise.resolve({
            enabled: false,
            model: null,
            ctx_size: 4096,
            gpu_layers: 99,
            port: 11435,
          });
        case "get_llm_catalog":
          return Promise.resolve({ families: [], models: [] });
        case "get_llm_logs":
          return Promise.resolve([]);
        case "get_model_hardware_fit":
          return Promise.resolve([]);
        case "get_gpu_stats":
          return Promise.resolve(null);

        // ── Tools tab ────────────────────────────────────────────────────
        case "list_skills":
          return Promise.resolve([]);
        case "get_skills_refresh_interval":
          return Promise.resolve(86400);
        case "get_skills_sync_on_launch":
          return Promise.resolve(true);
        case "get_skills_last_sync":
          return Promise.resolve(null);
        case "get_skills_license":
          return Promise.resolve(null);
        case "web_cache_stats":
          return Promise.resolve({ total_entries: 0, total_bytes: 0, search_entries: 0, fetch_entries: 0 });

        // ── Clients tab ──────────────────────────────────────────────────
        case "get_ws_config":
          return Promise.resolve(["127.0.0.1", 8375]);
        case "get_ws_port":
          return Promise.resolve(8375);
        case "get_api_token":
          return Promise.resolve("test-token-123");

        // ── Updates tab ──────────────────────────────────────────────────
        case "get_app_version":
          return Promise.resolve("0.0.78");
        case "get_update_check_interval":
          return Promise.resolve(86400);

        // ── Permissions tab ──────────────────────────────────────────────
        case "check_accessibility_permission":
          return Promise.resolve(true);
        case "check_screen_recording_permission":
          return Promise.resolve(true);
        case "get_calendar_permission_status":
          return Promise.resolve("authorized");
        case "get_location_permission_status":
          return Promise.resolve("authorized");
        case "get_active_window_tracking":
          return Promise.resolve(false);
        case "get_input_activity_tracking":
          return Promise.resolve(false);
        case "get_active_window":
          return Promise.resolve(null);
        case "get_last_input_activity":
          return Promise.resolve([0, 0]);
        case "get_dnd_config":
          return Promise.resolve({ enabled: false, threshold: 0.7, cooldown_secs: 300 });
        case "get_dnd_active":
          return Promise.resolve(false);
        case "list_focus_modes":
          return Promise.resolve([]);

        // ── Screenshots tab ──────────────────────────────────────────────
        case "get_screenshots_dir":
          return Promise.resolve(["/tmp/screenshots", 0]);

        // ── Embeddings tab ───────────────────────────────────────────────
        case "get_eeg_model_status":
          return Promise.resolve({
            encoder_loaded: false,
            embeddings_today: 0,
            weights_path: null,
          });

        // ── LSL tab ──────────────────────────────────────────────────────
        case "get_lsl_config":
          return Promise.resolve({ enabled: false, streams: [] });
        case "discover_lsl_streams":
          return Promise.resolve([]);

        // ── Sleep tab ────────────────────────────────────────────────────
        case "get_calendar_events":
          return Promise.resolve([]);

        // ── Common ───────────────────────────────────────────────────────
        case "show_main_window":
        case "show_toast_from_frontend":
          return Promise.resolve();
        case "get_app_name":
          return Promise.resolve("NeuroSkill Test");
        case "plugin:event|listen":
          return Promise.resolve(0);
        case "plugin:event|unlisten":
          return Promise.resolve();
        default:
          return Promise.resolve(null);
      }
    };
  `;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

async function openSettings(page: Page) {
  await page.addInitScript({ content: buildMockScript() });
  await page.goto("http://localhost:1420/settings", { waitUntil: "networkidle" });
  await page.waitForTimeout(1000);
}

async function clickTab(page: Page, tabName: RegExp | string) {
  const pattern = typeof tabName === "string" ? new RegExp(`^${tabName}$`, "i") : tabName;
  const tab = page.locator('[role="tab"]').filter({ hasText: pattern }).first();
  if (await tab.isVisible()) {
    await tab.click();
    await page.waitForTimeout(500);
    return true;
  }
  return false;
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("Settings page", () => {
  test("renders with tab sidebar", async ({ page }) => {
    await openSettings(page);

    // The sidebar should show tab labels
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/goals|devices|appearance|settings/i);

    await page.screenshot({ path: "test-results/settings-goals.png" });
  });

  test("Goals tab shows daily goal", async ({ page }) => {
    await openSettings(page);

    // Goals is the default tab — should show goal-related content
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/goal|minutes|daily/i);
  });
});

// ── Tab rendering tests ──────────────────────────────────────────────────────
// Each test navigates to a tab and verifies it renders without blank content.

const TABS_TO_TEST: [string, RegExp][] = [
  ["devices", /devices/i],
  ["appearance", /appearance/i],
  ["general", /Settings/],
  ["shortcuts", /shortcuts/i],
  ["llm", /llm/i],
  ["tools", /tools/i],
  ["updates", /updates/i],
  ["permissions", /permissions/i],
  ["voice", /Voice/],
  ["screenshots", /screenshots/i],
];

for (const [name, pattern] of TABS_TO_TEST) {
  test(`${name} tab renders`, async ({ page }) => {
    await openSettings(page);
    const clicked = await clickTab(page, pattern);
    expect(clicked).toBe(true);

    // Tab content should not be empty
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: `test-results/settings-${name}.png` });
  });
}
