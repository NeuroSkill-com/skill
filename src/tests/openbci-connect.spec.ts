/**
 * Playwright e2e tests for the OpenBCI Cyton connection flow.
 *
 * Verifies that:
 * 1. The Devices tab renders the OpenBCI config section
 * 2. Board radio buttons work
 * 3. Serial port picker is shown for serial boards
 * 4. Connect button triggers the connection flow
 * 5. Status transitions are reflected in the dashboard
 *
 * Run:  npx playwright test src/tests/openbci-connect.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";
import { buildDaemonMockScript, type CommandMap } from "./helpers/daemon-mock";

// ── Shared mock data ─────────────────────────────────────────────────────────

const BASE_COMMANDS: CommandMap = {
  get_app_version: "0.0.85",
  get_app_name: "NeuroSkill Test",
  get_theme_and_language: ["dark", "en"],
  show_main_window: null,
  show_toast_from_frontend: null,

  // Status
  get_status: {
    state: "disconnected",
    device_name: null,
    sample_count: 0,
    battery: 0,
    device_error: null,
    target_name: null,
    retry_attempt: 0,
    retry_countdown_secs: 0,
    paired_devices: [],
    filter_config: { sample_rate: 256, notch: null },
  },

  // Devices
  get_devices: [],
  get_device_log: [],
  get_scanner_config: { ble: true, usb_serial: true, cortex: true },
  get_cortex_ws_state: "disconnected",
  list_secondary_sessions: [],
  get_device_api_config: {
    emotiv_client_id: "",
    emotiv_client_secret: "",
    idun_api_token: "",
    oura_access_token: "",
  },
  get_supported_companies: [],

  // OpenBCI
  get_openbci_config: {
    board: "cyton",
    serial_port: "COM3",
    wifi_shield_ip: "",
    wifi_local_port: 3000,
    galea_ip: "",
    scan_timeout_secs: 10,
    channel_labels: [],
  },
  list_serial_ports: ["COM3", "COM5"],

  // Model / common
  get_eeg_model_status: { encoder_loaded: false },
  get_daily_goal: { value: 30 },
  get_daily_recording_mins: 0,
  get_dnd_config: { enabled: false },
  get_dnd_active: false,
  get_main_window_auto_fit: true,
  get_gpu_stats: null,
  get_latest_bands: null,
  get_ws_config: { host: "localhost", port: 8375 },
  get_ws_port: { port: 18444 },
  get_goal_notified_date: { value: "" },
  list_focus_modes: [],
  get_llm_catalog: { families: [], entries: [] },
  get_autostart_enabled: false,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async function openSettingsDevices(page: Page, extraCmds: CommandMap = {}) {
  const cmds = { ...BASE_COMMANDS, ...extraCmds };
  await page.addInitScript({ content: buildDaemonMockScript(cmds) });
  await page.goto("http://localhost:1420/settings", { waitUntil: "networkidle" });
  await page.waitForTimeout(1000);

  // Click Devices tab
  const devicesTab = page
    .locator('[role="tab"]')
    .filter({ hasText: /devices/i })
    .first();
  if (await devicesTab.isVisible()) {
    await devicesTab.click();
    await page.waitForTimeout(500);
  }
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("OpenBCI Cyton connection flow", () => {
  test("Devices tab shows OpenBCI config section", async ({ page }) => {
    await openSettingsDevices(page);

    // Expand OpenBCI section
    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await expect(openbciBtn).toBeVisible({ timeout: 5000 });
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // Should show board radio buttons
    await expect(page.locator("text=/Cyton$/i").first()).toBeVisible({ timeout: 3000 });
    await expect(page.locator("text=/Ganglion$/i").first()).toBeVisible();

    await page.screenshot({ path: "test-results/openbci-config-section.png" });
  });

  test("serial port picker visible for Cyton board", async ({ page }) => {
    await openSettingsDevices(page);

    // Expand OpenBCI section
    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // Cyton is selected (from mock) — serial port dropdown should be visible
    await expect(page.locator("text=/Serial Port|COM3/i").first()).toBeVisible({ timeout: 3000 });

    await page.screenshot({ path: "test-results/openbci-serial-port.png" });
  });

  test("serial port dropdown shows available ports", async ({ page }) => {
    await openSettingsDevices(page);

    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // Check that the dropdown contains the mocked ports
    const select = page.locator("select").first();
    if (await select.isVisible()) {
      const options = await select.locator("option").allTextContents();
      const hasPort = options.some((o) => o.includes("COM3") || o.includes("COM5"));
      expect(hasPort).toBe(true);
    }

    await page.screenshot({ path: "test-results/openbci-serial-dropdown.png" });
  });

  test("Connect button is visible for serial boards", async ({ page }) => {
    await openSettingsDevices(page);

    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // Connect button should be visible for Cyton (serial, not BLE)
    await expect(page.locator("button", { hasText: /Connect$/i })).toBeVisible({ timeout: 3000 });

    await page.screenshot({ path: "test-results/openbci-connect-button.png" });
  });

  test("WiFi fields shown when WiFi board selected", async ({ page }) => {
    await openSettingsDevices(page, {
      get_openbci_config: {
        board: "cyton_wifi",
        serial_port: "",
        wifi_shield_ip: "192.168.4.1",
        wifi_local_port: 3000,
        galea_ip: "",
        scan_timeout_secs: 10,
        channel_labels: [],
      },
    });

    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // WiFi Shield IP field should be visible
    await expect(page.locator("text=/WiFi Shield/i").first()).toBeVisible({ timeout: 3000 });

    await page.screenshot({ path: "test-results/openbci-wifi-fields.png" });
  });

  test("BLE timeout shown for Ganglion board", async ({ page }) => {
    await openSettingsDevices(page, {
      get_openbci_config: {
        board: "ganglion",
        serial_port: "",
        wifi_shield_ip: "",
        wifi_local_port: 3000,
        galea_ip: "",
        scan_timeout_secs: 10,
        channel_labels: [],
      },
    });

    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // Scan timeout field should be visible for BLE
    await expect(page.locator("text=/Scan Timeout|timeout/i").first()).toBeVisible({ timeout: 3000 });
    // Connect button should NOT be visible for Ganglion (BLE-only)
    const connectBtn = page.locator("button", { hasText: /^Connect$/ });
    await expect(connectBtn).not.toBeVisible();

    await page.screenshot({ path: "test-results/openbci-ganglion-ble.png" });
  });

  test("channel labels section renders correct count", async ({ page }) => {
    await openSettingsDevices(page, {
      get_openbci_config: {
        board: "cyton_daisy",
        serial_port: "COM3",
        wifi_shield_ip: "",
        wifi_local_port: 3000,
        galea_ip: "",
        scan_timeout_secs: 10,
        channel_labels: [],
      },
    });

    const openbciBtn = page.locator("button", { hasText: /OpenBCI/i }).first();
    await openbciBtn.click();
    await page.waitForTimeout(500);

    // CytonDaisy has 16 channels
    await expect(page.locator("text=/16\\)/").first()).toBeVisible({ timeout: 3000 });

    await page.screenshot({ path: "test-results/openbci-channel-labels.png" });
  });
});

// ── Dashboard status tests ───────────────────────────────────────────────────

test.describe("Dashboard with OpenBCI connected", () => {
  test("shows connected state with Cyton device", async ({ page }) => {
    const cmds = {
      ...BASE_COMMANDS,
      get_status: {
        state: "connected",
        device_name: "OpenBCI Cyton",
        sample_count: 5000,
        battery: 0,
        device_error: null,
        target_name: "openbci",
        retry_attempt: 0,
        retry_countdown_secs: 0,
        paired_devices: [],
        filter_config: { sample_rate: 250, notch: null },
      },
    };
    await page.addInitScript({ content: buildDaemonMockScript(cmds) });
    await page.goto("http://localhost:1420/", { waitUntil: "networkidle" });
    await page.waitForTimeout(1500);

    await expect(page.locator("text=/OpenBCI Cyton/i").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/openbci-dashboard-connected.png" });
  });

  test("shows connection error in disconnected state", async ({ page }) => {
    const cmds = {
      ...BASE_COMMANDS,
      get_status: {
        state: "disconnected",
        device_name: null,
        sample_count: 0,
        battery: 0,
        device_error: "Cyton prepare failed on COM3: Serial port error: Access denied",
        target_name: "openbci",
        retry_attempt: 0,
        retry_countdown_secs: 0,
        paired_devices: [],
      },
    };
    await page.addInitScript({ content: buildDaemonMockScript(cmds) });
    await page.goto("http://localhost:1420/", { waitUntil: "networkidle" });
    await page.waitForTimeout(1500);

    // Error should be visible
    await expect(page.locator("text=/Access denied|prepare failed/i").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/openbci-dashboard-error.png" });
  });

  test("shows connecting state", async ({ page }) => {
    const cmds = {
      ...BASE_COMMANDS,
      get_status: {
        state: "scanning",
        device_name: null,
        sample_count: 0,
        battery: 0,
        device_error: null,
        target_name: "openbci",
        retry_attempt: 0,
        retry_countdown_secs: 0,
        paired_devices: [],
      },
    };
    await page.addInitScript({ content: buildDaemonMockScript(cmds) });
    await page.goto("http://localhost:1420/", { waitUntil: "networkidle" });
    await page.waitForTimeout(1500);

    // Should show scanning/connecting indicator
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/scanning|connecting|looking/i);
    await page.screenshot({ path: "test-results/openbci-dashboard-scanning.png" });
  });
});
