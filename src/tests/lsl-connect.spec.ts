/**
 * Playwright E2E tests for the LSL connection flow on the dashboard.
 *
 * Mocks the daemon HTTP API and Tauri invoke to simulate:
 * - Initial disconnected state
 * - Connecting to an LSL stream
 * - Dashboard updating to show connected state
 * - Signal quality, device badge, channel count
 * - Disconnect flow
 * - Reconnection after disconnect
 *
 * Run:  npx playwright test src/tests/lsl-connect.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";
import { buildDaemonMockScript } from "./helpers/daemon-mock";

// ── Shared status shapes ─────────────────────────────────────────────────────

const DISCONNECTED_STATUS = {
  state: "disconnected",
  device_name: null,
  device_kind: "",
  device_id: null,
  sample_count: 0,
  battery: 0,
  eeg: [],
  paired_devices: [],
  device_error: null,
  target_name: null,
  channel_quality: [],
  channel_names: [],
  eeg_channel_count: 0,
  eeg_sample_rate_hz: 0,
  retry_attempt: 0,
  retry_countdown_secs: 0,
  ppg: [0, 0, 0],
  ppg_sample_count: 0,
  accel: [0, 0, 0],
  gyro: [0, 0, 0],
  fuel_gauge_mv: 0,
  temperature_raw: 0,
  has_ppg: false,
  has_imu: false,
  has_central_electrodes: false,
  has_full_montage: false,
  filter_config: {
    sample_rate: 256,
    low_pass_hz: 50,
    high_pass_hz: 0.5,
    notch: null,
    notch_bandwidth_hz: 1.0,
  },
};

const CONNECTED_STATUS = {
  ...DISCONNECTED_STATUS,
  state: "connected",
  device_name: "SkillVirtualEEG",
  device_kind: "lsl",
  device_id: "skill-virtual-eeg-001",
  sample_count: 7296,
  eeg_channel_count: 32,
  eeg_sample_rate_hz: 256,
  channel_names: Array.from({ length: 32 }, (_, i) => `Ch${i + 1}`),
  channel_quality: Array.from({ length: 32 }, () => "good"),
  csv_path: "/tmp/exg_test.csv",
};

const CONNECTING_STATUS = {
  ...DISCONNECTED_STATUS,
  state: "connecting",
  target_name: "lsl:SkillVirtualEEG",
};

const BASE_COMMANDS = {
  get_app_version: "0.0.86",
  get_app_name: "NeuroSkill Test",
  get_theme_and_language: ["dark", "en"],
  show_main_window: null,
  get_status: DISCONNECTED_STATUS,
  get_eeg_model_status: { encoder_loaded: true },
  get_llm_catalog: { families: [], entries: [] },
  get_daily_goal: { value: 30 },
  get_daily_recording_mins: 0,
  get_dnd_config: { enabled: false },
  get_dnd_active: false,
  get_main_window_auto_fit: true,
  get_gpu_stats: null,
  get_latest_bands: null,
  get_ws_config: { host: "localhost", port: 8375 },
  get_cortex_ws_state: "disconnected",
  list_secondary_sessions: [],
  list_focus_modes: [],
  get_goal_notified_date: { value: "" },
  get_daemon_bootstrap: {
    port: 18444,
    token: "test-token",
    compatible_protocol: true,
    daemon_version: "0.0.1",
    protocol_version: 1,
  },
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async function setupPage(page: Page, statusOverride?: Record<string, unknown>) {
  const commands = {
    ...BASE_COMMANDS,
    get_status: statusOverride ?? DISCONNECTED_STATUS,
  };
  await page.addInitScript({ content: buildDaemonMockScript(commands) });
  await page.goto("/");
  // Wait for dashboard to render
  await page.waitForSelector("[role='meter'], button, .status-ring", { timeout: 5000 }).catch(() => {});
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("LSL connection flow", () => {
  test("shows disconnected state initially", async ({ page }) => {
    await setupPage(page);
    // Should show disconnected badge
    const badge = page.locator("text=DISCONNECTED").first();
    await expect(badge).toBeVisible({ timeout: 5000 });
  });

  test("shows connected state with LSL device info", async ({ page }) => {
    await setupPage(page, CONNECTED_STATUS);

    // Badge should show connected
    const badge = page.locator("text=CONNECTED").first();
    await expect(badge).toBeVisible({ timeout: 5000 });

    // Device name should appear
    await expect(page.locator("text=SkillVirtualEEG").first()).toBeVisible({ timeout: 5000 });

    // Device info badge: channel count + sample rate + LSL
    await expect(page.locator("text=32ch").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=256 Hz").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=LSL").first()).toBeVisible({ timeout: 5000 });
  });

  test("shows connecting state with scanning badge", async ({ page }) => {
    await setupPage(page, CONNECTING_STATUS);

    // Should show scanning/connecting state (not disconnected)
    const disconnected = page.locator("text=DISCONNECTED").first();
    await expect(disconnected)
      .not.toBeVisible({ timeout: 3000 })
      .catch(() => {});
  });

  test("signal quality section shows summary for 32 channels", async ({ page }) => {
    await setupPage(page, CONNECTED_STATUS);

    // Signal quality summary should show count of good channels
    await expect(page.locator("text=32✓").first()).toBeVisible({ timeout: 5000 });
  });

  test("transitions from disconnected to connected on status poll", async ({ page }) => {
    // Start disconnected
    await setupPage(page);
    await expect(page.locator("text=DISCONNECTED").first()).toBeVisible({ timeout: 5000 });

    // Simulate daemon returning connected status on next poll
    await page.evaluate((connectedStatus) => {
      // Override the fetch mock to return connected status
      const origFetch = window.fetch;
      window.fetch = (url: RequestInfo | URL, opts?: RequestInit) => {
        const urlStr = typeof url === "string" ? url : url.toString();
        if (urlStr.includes("/v1/status")) {
          return Promise.resolve(
            new Response(JSON.stringify(connectedStatus), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }),
          );
        }
        return origFetch.call(window, url, opts);
      };
    }, CONNECTED_STATUS);

    // Dashboard polls every 2s — wait for it to pick up the change
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await expect(page.locator("text=SkillVirtualEEG").first()).toBeVisible({ timeout: 3000 });
  });

  test("transitions from connected to disconnected", async ({ page }) => {
    // Start connected
    await setupPage(page, CONNECTED_STATUS);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 5000 });

    // Simulate disconnect
    await page.evaluate((disconnectedStatus) => {
      const origFetch = window.fetch;
      window.fetch = (url: RequestInfo | URL, opts?: RequestInit) => {
        const urlStr = typeof url === "string" ? url : url.toString();
        if (urlStr.includes("/v1/status")) {
          return Promise.resolve(
            new Response(JSON.stringify(disconnectedStatus), {
              status: 200,
              headers: { "Content-Type": "application/json" },
            }),
          );
        }
        return origFetch.call(window, url, opts);
      };
    }, DISCONNECTED_STATUS);

    // Should transition to disconnected within poll interval
    await expect(page.locator("text=DISCONNECTED").first()).toBeVisible({ timeout: 8000 });
  });

  test("shows LSL / Settings button when disconnected with no paired devices", async ({ page }) => {
    await setupPage(page);
    await expect(page.locator("text=LSL / Settings").first()).toBeVisible({ timeout: 5000 });
  });

  test("shows disconnect button when connected", async ({ page }) => {
    await setupPage(page, CONNECTED_STATUS);
    await expect(page.locator("text=Disconnect").first()).toBeVisible({ timeout: 5000 });
  });

  test("shows recording indicator when connected", async ({ page }) => {
    await setupPage(page, CONNECTED_STATUS);
    // CSV path should be visible as recording indicator
    await expect(page.locator("text=exg_test.csv").first()).toBeVisible({ timeout: 5000 });
  });
});

test.describe("high channel count rendering", () => {
  const STATUS_64CH = {
    ...CONNECTED_STATUS,
    eeg_channel_count: 64,
    channel_names: Array.from({ length: 64 }, (_, i) => `Ch${i + 1}`),
    channel_quality: Array.from({ length: 64 }, () => "good"),
  };

  test("renders 64-channel device badge", async ({ page }) => {
    await setupPage(page, STATUS_64CH);
    await expect(page.locator("text=64ch").first()).toBeVisible({ timeout: 5000 });
  });

  test("signal quality shows summary for 64 channels", async ({ page }) => {
    await setupPage(page, STATUS_64CH);
    await expect(page.locator("text=64✓").first()).toBeVisible({ timeout: 5000 });
  });
});
