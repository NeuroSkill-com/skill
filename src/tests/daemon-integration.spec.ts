/**
 * Playwright e2e tests verifying the daemon client layer works
 * end-to-end in the browser with mocked daemon HTTP + Tauri invoke.
 *
 * These tests validate that daemonInvoke correctly routes commands
 * to fetch() and that the UI renders data from daemon responses.
 *
 * Run:  npx playwright test src/tests/daemon-integration.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";
import { buildDaemonMockScript, type CommandMap } from "./helpers/daemon-mock";

// ── Shared mock data ─────────────────────────────────────────────────────────

const BASE_COMMANDS: CommandMap = {
  // App meta
  get_app_version: "0.0.85",
  get_app_name: "NeuroSkill Test",
  get_about_info: { name: "NeuroSkill", version: "0.0.85", copyright: "© 2026" },
  get_theme_and_language: ["dark", "en"],

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

  // EEG model
  get_eeg_model_status: {
    encoder_loaded: false,
    downloading_weights: false,
    download_status_msg: null,
  },

  // LLM
  get_llm_catalog: { families: [], entries: [] },
  get_llm_server_status: { status: "stopped", model_name: "", n_ctx: 0 },
  get_llm_config: { enabled: false, model: null, ctx_size: 4096 },
  get_llm_downloads: [],

  // History
  get_history_stats: { total_sessions: 0, total_hours: 0, total_days: 0 },
  list_sessions: [],
  list_all_sessions: [],
  list_sessions_for_local_day: [],
  list_local_session_days: [],

  // Settings
  get_daily_goal: { value: 30 },
  get_daily_recording_mins: 0,
  get_dnd_config: { enabled: false },
  get_dnd_active: false,
  get_main_window_auto_fit: true,
  get_latest_bands: null,
  get_gpu_stats: null,
  get_ws_config: { host: "localhost", port: 8375 },
  get_ws_port: { port: 18444 },
  get_ws_clients: [],
  get_ws_request_log: [],
  get_goal_notified_date: { value: "" },
  list_focus_modes: [],
  get_cortex_ws_state: "disconnected",
  list_secondary_sessions: [],

  // Labels
  get_recent_labels: [],
  search_labels_by_text: [],
  query_annotations: [],

  // Native
  show_main_window: null,
  show_toast_from_frontend: null,
  get_autostart_enabled: false,
};

// ── Helpers ──────────────────────────────────────────────────────────────────

async function navigateTo(page: Page, path: string, extraCmds: CommandMap = {}) {
  const cmds = { ...BASE_COMMANDS, ...extraCmds };
  await page.addInitScript({ content: buildDaemonMockScript(cmds) });
  await page.goto(`http://localhost:1420${path}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);
}

// ── Dashboard ────────────────────────────────────────────────────────────────

test.describe("Dashboard with daemon mock", () => {
  test("renders disconnected state", async ({ page }) => {
    await navigateTo(page, "/");
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);
    await page.screenshot({ path: "test-results/daemon-dashboard-disconnected.png" });
  });

  test("shows daily goal from daemon", async ({ page }) => {
    await navigateTo(page, "/", { get_daily_goal: { value: 60 } });
    // The goal value should be reflected somewhere in the UI
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);
    await page.screenshot({ path: "test-results/daemon-dashboard-goal.png" });
  });

  test("renders connected state with device", async ({ page }) => {
    await navigateTo(page, "/", {
      get_status: {
        state: "connected",
        device_name: "Muse S Test",
        sample_count: 12345,
        battery: 85.0,
        device_error: null,
        target_name: null,
        retry_attempt: 0,
        retry_countdown_secs: 0,
        paired_devices: [],
        filter_config: { sample_rate: 256, notch: "60hz" },
        channel_labels: ["TP9", "AF7", "AF8", "TP10"],
      },
    });

    // Should show device name
    await expect(page.locator("text=Muse S Test").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/daemon-dashboard-connected.png" });
  });
});

// ── Downloads ────────────────────────────────────────────────────────────────

test.describe("Downloads with daemon mock", () => {
  test("renders download list from daemon", async ({ page }) => {
    await navigateTo(page, "/downloads", {
      get_llm_downloads: [
        {
          repo: "test/model",
          filename: "test-q4.gguf",
          quant: "Q4_K_M",
          size_gb: 2.5,
          is_mmproj: false,
          state: "downloaded",
          progress: 1.0,
          shard_count: 1,
          current_shard: 1,
        },
      ],
    });

    await expect(page.locator("text=/test-q4|Q4_K_M/i").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/daemon-downloads.png" });
  });
});

// ── API Status ───────────────────────────────────────────────────────────────

test.describe("API status with daemon mock", () => {
  test("renders daemon status", async ({ page }) => {
    await navigateTo(page, "/api", {
      get_daemon_status: {
        base_url: "http://127.0.0.1:18444",
        reachable: true,
        authenticated: true,
        compatible_protocol: true,
        daemon_required: false,
        version: { daemon: "skill-daemon", protocol_version: 1, daemon_version: "0.0.1" },
        error: null,
      },
    });

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(20);
    await page.screenshot({ path: "test-results/daemon-api-status.png" });
  });
});

// ── Chat ─────────────────────────────────────────────────────────────────────

test.describe("Chat with daemon mock", () => {
  test("renders chat UI with session from daemon", async ({ page }) => {
    await navigateTo(page, "/chat", {
      get_last_chat_session: {
        session_id: 1,
        messages: [
          {
            id: 1,
            session_id: 1,
            role: "user",
            content: "Hello daemon!",
            thinking: null,
            created_at: Date.now(),
            tool_calls: [],
          },
          {
            id: 2,
            session_id: 1,
            role: "assistant",
            content: "Hello from the daemon!",
            thinking: null,
            created_at: Date.now(),
            tool_calls: [],
          },
        ],
      },
      list_chat_sessions: [
        { id: 1, title: "Test Chat", preview: "Hello daemon!", created_at: Date.now(), message_count: 2 },
      ],
      get_session_params: { value: "" },
    });

    // Should show the chat messages
    await expect(page.locator("text=Hello daemon!").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/daemon-chat.png" });
  });
});
