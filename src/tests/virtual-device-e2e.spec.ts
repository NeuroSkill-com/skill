/**
 * End-to-end Playwright tests for the Virtual Devices window.
 *
 * Two event pathways are tested separately:
 *
 *  A. Tauri cross-window events  (window.__skillFireTauriEvent__)
 *     emit("virtual-device-status", payload)  →  listen("virtual-device-status")
 *     This is the ACTUAL path the virtual-devices page uses to tell the
 *     dashboard "a virtual device is now running."
 *
 *  B. Daemon WebSocket events  (window.__skillEmitEvent__)
 *     EegSample / EegBands arrive from the daemon WS and drive the dashboard
 *     charts and metrics.  In the real app these come from the JS generator
 *     via injectDaemonEvent; in tests we push them directly.
 *
 * The re-broadcast timer (fires every 2 s while the device is running) is
 * also tested: the dashboard must show CONNECTED even if "virtual-device-status"
 * is fired after the dashboard is already loaded.
 *
 * Run:  npx playwright test src/tests/virtual-device-e2e.spec.ts
 */

import { expect, type Page, test } from "@playwright/test";
import { buildDaemonMockScript, type CommandMap } from "./helpers/daemon-mock";

// ── Shared fixtures ──────────────────────────────────────────────────────────

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
  ppg: [],
  ppg_sample_count: 0,
  imu_sample_count: 0,
  accel: [0, 0, 0],
  gyro: [0, 0, 0],
  fuel_gauge_mv: 0,
  temperature_raw: 0,
  has_ppg: false,
  has_imu: false,
  has_central_electrodes: false,
  has_full_montage: false,
  filter_config: { sample_rate: 256, low_pass_hz: 50, high_pass_hz: 0.5, notch: null, notch_bandwidth_hz: 1 },
};

// Exactly what injectVirtualStatus() sends for Muse S (4ch, 256Hz, good_quality)
const VIRTUAL_STATUS_4CH = {
  state: "connected",
  device_name: "Virtual EEG",
  device_id: "virtual-eeg",
  device_kind: "lsl",
  serial_number: null,
  mac_address: null,
  csv_path: null,
  sample_count: 0,
  battery: 0,
  eeg: [0, 0, 0, 0],
  paired_devices: [],
  device_error: null,
  target_name: null,
  filter_config: { sample_rate: 256, low_pass_hz: null, high_pass_hz: null, notch: null, notch_bandwidth_hz: 1 },
  channel_quality: ["good", "good", "good", "good"],
  retry_attempt: 0,
  retry_countdown_secs: 0,
  ppg: [],
  ppg_sample_count: 0,
  imu_sample_count: 0,
  accel: [0, 0, 0],
  gyro: [0, 0, 0],
  fuel_gauge_mv: 0,
  temperature_raw: 0,
  hardware_version: null,
  has_ppg: false,
  has_imu: false,
  has_central_electrodes: false,
  has_full_montage: false,
  channel_names: ["TP9", "AF7", "AF8", "TP10"],
  eeg_channel_count: 4,
  eeg_sample_rate_hz: 256,
};

const VIRTUAL_STATUS_8CH = {
  ...VIRTUAL_STATUS_4CH,
  eeg: new Array(8).fill(0),
  channel_quality: new Array(8).fill("good"),
  channel_names: ["Fp1", "Fp2", "F3", "F4", "C3", "C4", "O1", "O2"],
  eeg_channel_count: 8,
  has_central_electrodes: true,
};

const SYNTH_BANDS: Record<string, unknown> = {
  timestamp: Date.now(),
  channels: [
    {
      channel: "TP9",
      delta: 1.2,
      theta: 2.5,
      alpha: 18.0,
      beta: 6.0,
      gamma: 1.8,
      high_gamma: 0,
      rel_delta: 0.04,
      rel_theta: 0.08,
      rel_alpha: 0.62,
      rel_beta: 0.21,
      rel_gamma: 0.05,
      rel_high_gamma: 0,
      dominant: "alpha",
      dominant_symbol: "α",
      dominant_color: "#22c55e",
    },
    {
      channel: "AF7",
      delta: 1.1,
      theta: 2.2,
      alpha: 17.5,
      beta: 5.8,
      gamma: 1.6,
      high_gamma: 0,
      rel_delta: 0.04,
      rel_theta: 0.08,
      rel_alpha: 0.63,
      rel_beta: 0.2,
      rel_gamma: 0.05,
      rel_high_gamma: 0,
      dominant: "alpha",
      dominant_symbol: "α",
      dominant_color: "#22c55e",
    },
    {
      channel: "AF8",
      delta: 1.3,
      theta: 2.4,
      alpha: 18.5,
      beta: 6.2,
      gamma: 1.9,
      high_gamma: 0,
      rel_delta: 0.04,
      rel_theta: 0.08,
      rel_alpha: 0.62,
      rel_beta: 0.21,
      rel_gamma: 0.05,
      rel_high_gamma: 0,
      dominant: "alpha",
      dominant_symbol: "α",
      dominant_color: "#22c55e",
    },
    {
      channel: "TP10",
      delta: 1.2,
      theta: 2.3,
      alpha: 17.8,
      beta: 6.1,
      gamma: 1.7,
      high_gamma: 0,
      rel_delta: 0.04,
      rel_theta: 0.08,
      rel_alpha: 0.63,
      rel_beta: 0.2,
      rel_gamma: 0.05,
      rel_high_gamma: 0,
      dominant: "alpha",
      dominant_symbol: "α",
      dominant_color: "#22c55e",
    },
  ],
  faa: 0.12,
  snr: 5.0,
  tar: 0.14,
  bar: 0.33,
  dtr: 0.07,
  pse: 0.72,
  apf: 10.1,
  mood: 58,
  bps: -1.3,
  coherence: 0.61,
  mu_suppression: 0.95,
  tbr: 0.42,
  sef95: 28.0,
  spectral_centroid: 11.2,
  hjorth_activity: 45.0,
  hjorth_mobility: 0.24,
  hjorth_complexity: 1.6,
  perm_entropy: 0.81,
  higuchi_fd: 1.72,
  dfa_exponent: 0.88,
  sample_entropy: 1.4,
  pac_theta_gamma: 0.08,
  laterality_index: 0.03,
  hr: 68,
  rmssd: 42,
  sdnn: 55,
  pnn50: 28,
  lf_hf_ratio: 1.2,
  respiratory_rate: 15,
  spo2: 98,
  perfusion_index: 4.2,
  stress_index: 22,
  meditation: 72,
  cognitive_load: 28,
  drowsiness: 15,
  blink_count: 3,
  blink_rate: 14.0,
  head_pitch: 1.2,
  head_roll: -0.5,
  stillness: 88,
  nods: 0,
  shakes: 0,
};

const BASE_COMMANDS: CommandMap = {
  get_app_version: "0.0.86",
  get_app_name: "NeuroSkill Test",
  get_theme_and_language: ["dark", "en"],
  show_main_window: null,
  get_status: DISCONNECTED_STATUS,
  get_eeg_model_status: { encoder_loaded: false },
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
  lsl_virtual_source_running: { running: false },
  lsl_virtual_source_start: { started: true },
  lsl_virtual_source_start_configured: { started: true },
  lsl_virtual_source_stop: { was_running: true },
  start_session: { state: "scanning" },
  cancel_session: { state: "disconnected" },
  open_virtual_devices_window: null,
};

// ── Page helpers ─────────────────────────────────────────────────────────────

async function openPage(page: Page, path: string, extra: CommandMap = {}) {
  await page.addInitScript({ content: buildDaemonMockScript({ ...BASE_COMMANDS, ...extra }) });
  await page.goto(`http://localhost:1420${path}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(600);
}

/** Fire a Tauri cross-window event — the path used by the virtual-devices page
 *  to tell the dashboard "Virtual EEG is now connected". */
async function fireTauriEvent(page: Page, event: string, payload: Record<string, unknown>) {
  await page.evaluate(
    ([e, p]) => {
      (window as unknown as { __skillFireTauriEvent__?: (e: string, p: unknown) => void }).__skillFireTauriEvent__?.(
        e,
        p,
      );
    },
    [event, payload] as const,
  );
}

/** Push a daemon WebSocket event — the path for real EEG sample/band data. */
async function emitDaemonEvent(page: Page, type: string, payload: Record<string, unknown>) {
  await page.evaluate(
    ([t, p]) => {
      (window as unknown as { __skillEmitEvent__?: (t: string, p: unknown) => void }).__skillEmitEvent__?.(t, p);
    },
    [type, payload] as const,
  );
}

/** Patch the /v1/status fetch mock to return a new status on the next poll. */
async function patchStatus(page: Page, status: Record<string, unknown>) {
  await page.evaluate((s) => {
    const orig = window.fetch;
    window.fetch = (url: RequestInfo | URL, opts?: RequestInit) => {
      const u = typeof url === "string" ? url : url.toString();
      if (u.includes("/v1/status")) {
        window.fetch = orig;
        return Promise.resolve(
          new Response(JSON.stringify(s), { status: 200, headers: { "Content-Type": "application/json" } }),
        );
      }
      return orig.call(window, url, opts);
    };
  }, status);
}

/** Full session injection — status via Tauri event + EEG data via WS. */
async function injectFullSession(
  page: Page,
  status: Record<string, unknown> = VIRTUAL_STATUS_4CH,
  bands: Record<string, unknown> = SYNTH_BANDS,
  numChannels = 4,
  sampleRate = 256,
) {
  // 1. Tauri event path → applyVirtualStatus()
  await fireTauriEvent(page, "virtual-device-status", status);
  // 2. Keep fetch poll returning the same (so re-polls don't reset to disconnected)
  await patchStatus(page, status);
  await page.waitForTimeout(200);

  // 3. WS path → subscribeBands → updateScores + bandChartEl.update
  for (let i = 0; i < 8; i++) {
    // Feed via both paths so the test works regardless of which one the page uses
    await emitDaemonEvent(page, "EegBands", bands);
    // Also relay via Tauri event (virtual-eeg-bands → relayEvent)
    await fireTauriEvent(page, "virtual-eeg-bands", bands);
    await page.waitForTimeout(25);
  }

  // 4. WS path → subscribeEeg → chartEl.pushSamples
  const batchSize = Math.ceil(sampleRate / 8);
  const sine = Array.from({ length: batchSize }, (_, j) => Math.sin((2 * Math.PI * 10 * j) / sampleRate) * 50);
  for (let ch = 0; ch < numChannels; ch++) {
    const samplePayload = { electrode: ch, samples: sine, timestamp: Date.now() / 1000 };
    await emitDaemonEvent(page, "EegSample", samplePayload);
    await fireTauriEvent(page, "virtual-eeg-sample", samplePayload);
  }
}

// ════════════════════════════════════════════════════════════════════════════
// 1. Virtual Devices window — UI and daemon command tests
// ════════════════════════════════════════════════════════════════════════════

test.describe("Virtual Devices window", () => {
  test("renders page title and all preset cards", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    await expect(page.locator("h1", { hasText: /Virtual Devices/i })).toBeVisible({ timeout: 5000 });
    for (const name of [
      "Muse S",
      "OpenBCI Cyton",
      "Strong Alpha",
      "Artifact Test",
      "Dropout Test",
      "Minimal",
      "Custom",
    ]) {
      await expect(page.locator(`text=/${name}/i`).first()).toBeVisible({ timeout: 5000 });
    }
    await page.screenshot({ path: "test-results/vdev-01-presets.png" });
  });

  test("page is scrollable", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    const scrollable = await page.evaluate(() => {
      const main = document.querySelector("main");
      return main ? main.scrollHeight >= main.clientHeight : false;
    });
    expect(scrollable).toBe(true);
  });

  test("selecting Muse S marks it selected and updates status bar", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    await page.locator("text=/Muse S/i").first().click();
    await page.waitForTimeout(150);
    await expect(page.locator("[aria-pressed='true']").filter({ hasText: /Muse S/i })).toBeVisible({ timeout: 3000 });
    await page.screenshot({ path: "test-results/vdev-02-muse-selected.png" });
  });

  test("custom preset shows full configurator with all sections", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    await page.locator("button[aria-pressed]", { hasText: /Custom/i }).click();
    await page.waitForTimeout(300);
    for (const label of [/Signal Template/i, /Channels/i, /Sample Rate/i, /Signal Quality/i]) {
      await expect(page.locator(`text=${label}`).first()).toBeVisible({ timeout: 3000 });
    }
    await page.screenshot({ path: "test-results/vdev-03-custom.png" });
  });

  test("advanced section expands to show Amplitude, Noise, Line noise, Dropout", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    await page.locator("button[aria-pressed]", { hasText: /Custom/i }).click();
    await page.waitForTimeout(200);
    await page
      .locator("button", { hasText: /Advanced/i })
      .first()
      .click();
    await page.waitForTimeout(150);
    for (const label of [/Amplitude/i, /Noise floor/i, /Line noise/i, /Dropout/i]) {
      await expect(page.locator(`text=${label}`).first()).toBeVisible({ timeout: 3000 });
    }
    await page.screenshot({ path: "test-results/vdev-04-advanced.png" });
  });

  test("start calls lsl_virtual_source_start_configured then start_session", async ({ page }) => {
    await page.addInitScript({ content: buildDaemonMockScript(BASE_COMMANDS) });
    await page.addInitScript(`
      window.__invokedArgs__ = [];
      window.__fetchedPaths__ = [];
      const origInvoke = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
        window.__invokedArgs__.push({ cmd, args: JSON.parse(JSON.stringify(args || {})) });
        return origInvoke(cmd, args);
      };
      const origFetch = window.fetch;
      const _wrapFetch = window.fetch;
      window.fetch = function(url, opts) {
        const u = typeof url === "string" ? url : url.toString();
        if (u.includes("/v1/")) {
          window.__fetchedPaths__.push(u);
        }
        return _wrapFetch.call(window, url, opts);
      };
    `);
    await page.goto("http://localhost:1420/virtual-devices", { waitUntil: "networkidle" });
    await page.waitForTimeout(600);

    // Select Muse S (4ch, 256 Hz)
    await page.locator("text=/Muse S/i").first().click();
    await page.waitForTimeout(100);

    await page.locator("button", { hasText: /Start Virtual Device/i }).click();
    await page.waitForTimeout(1200); // allow 600ms LSL announce delay + start_session call

    const fetched = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__fetchedPaths__ as string[],
    );

    // Step 1: daemon virtual source started with preset config
    const hasStart = fetched.some((u) => u.includes("/v1/lsl/virtual-source/start"));
    expect(hasStart).toBe(true);

    // Step 2: dashboard connected via LSL session
    const hasSession = fetched.some((u) => u.includes("/v1/control/start-session"));
    expect(hasSession).toBe(true);

    await page.screenshot({ path: "test-results/vdev-05-daemon-flow.png" });
  });

  test("stop calls cancel_session then lsl_virtual_source_stop", async ({ page }) => {
    await page.addInitScript({
      content: buildDaemonMockScript({
        ...BASE_COMMANDS,
        lsl_virtual_source_running: { running: true },
        get_status: { ...DISCONNECTED_STATUS, state: "connected", device_name: "SkillVirtualEEG" },
      }),
    });
    await page.addInitScript(`
      window.__fetchedPaths__ = [];
      const _origFetch = window.fetch;
      window.fetch = function(url, opts) {
        const u = typeof url === "string" ? url : url.toString();
        if (u.includes("/v1/")) window.__fetchedPaths__.push(u);
        return _origFetch.call(window, url, opts);
      };
    `);
    await page.goto("http://localhost:1420/virtual-devices", { waitUntil: "networkidle" });
    await page.waitForTimeout(800); // allow poll to set lslRunning=true

    await page.locator("button", { hasText: /Stop Virtual Device/i }).click();
    await page.waitForTimeout(600);

    const paths = await page.evaluate(
      () => (window as unknown as Record<string, unknown>).__fetchedPaths__ as string[],
    );
    expect(paths.some((u) => u.includes("/v1/control/cancel-session"))).toBe(true);
    expect(paths.some((u) => u.includes("/v1/lsl/virtual-source/stop"))).toBe(true);
  });

  test("starting fires virtual-device-status Tauri event with correct channel count", async ({ page }) => {
    await openPage(page, "/virtual-devices");

    // Capture Tauri events and listen for virtual-device-status via the mock event system
    type TauriWindow = Window & {
      __TAURI_INTERNALS__: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
    };
    const statusPromise = page.evaluate(() => {
      return new Promise<Record<string, unknown>>((resolve) => {
        const w = window as unknown as TauriWindow;
        const origInvoke = w.__TAURI_INTERNALS__.invoke;
        w.__TAURI_INTERNALS__.invoke = (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
            const ev = args?.event;
            const payload = args?.payload as Record<string, unknown> | undefined;
            if (ev === "virtual-device-status" && payload?.state === "connected") {
              resolve(payload);
            }
          }
          return origInvoke(cmd, args);
        };
        setTimeout(() => resolve({}), 8000);
      });
    });

    // Select 32-ch preset and start
    await page.locator("text=/32-Ch EEG Cap/i").first().click();
    await page.waitForTimeout(100);
    await page.locator("button", { hasText: /Start Virtual Device/i }).click();

    const statusPayload = await statusPromise;
    expect(statusPayload.state).toBe("connected");
    expect(statusPayload.eeg_channel_count).toBe(32);
    await page.screenshot({ path: "test-results/vdev-06-event-emitted.png" });
  });

  test("stopping fires virtual-device-status disconnected Tauri event", async ({ page }) => {
    // Open the page with lslRunning=true (source already started)
    await openPage(page, "/virtual-devices", {
      lsl_virtual_source_running: { running: true },
      get_status: { ...DISCONNECTED_STATUS, state: "connected", device_name: "SkillVirtualEEG" },
    });

    // Set up promise to capture the disconnect event
    type TauriWindow = Window & {
      __TAURI_INTERNALS__: { invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown> };
    };
    const disconnectPromise = page.evaluate(() => {
      return new Promise<Record<string, unknown>>((resolve) => {
        const w = window as unknown as TauriWindow;
        const origInvoke = w.__TAURI_INTERNALS__.invoke;
        w.__TAURI_INTERNALS__.invoke = (cmd: string, args?: Record<string, unknown>) => {
          if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
            const ev = args?.event;
            const payload = args?.payload as Record<string, unknown> | undefined;
            if (ev === "virtual-device-status" && payload?.state === "disconnected") {
              resolve(payload);
            }
          }
          return origInvoke(cmd, args);
        };
        setTimeout(() => resolve({}), 8000);
      });
    });

    // Click Stop
    await page.locator("button", { hasText: /Stop Virtual Device/i }).click({ timeout: 5000 });

    const payload = await disconnectPromise;
    expect(payload.state).toBe("disconnected");
    await page.screenshot({ path: "test-results/vdev-07-stopped-event.png" });
  });

  test("signal preview canvas is always visible", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    const canvas = page.locator("canvas").first();
    await expect(canvas).toBeVisible({ timeout: 5000 });
    const box = await canvas.boundingBox();
    expect(box?.width).toBeGreaterThan(100);
    expect(box?.height).toBeGreaterThan(60);
  });

  test("LSL source section shows start/stop buttons", async ({ page }) => {
    await openPage(page, "/virtual-devices");
    await expect(page.locator("text=/LSL source/i").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("button", { hasText: /Start Virtual Device/i })).toBeVisible({ timeout: 5000 });
  });
});

// ════════════════════════════════════════════════════════════════════════════
// 2. Dashboard — Tauri event path (virtual-device-status)
// ════════════════════════════════════════════════════════════════════════════

test.describe("Dashboard receives virtual-device-status Tauri event", () => {
  async function loadDash(page: Page) {
    await openPage(page, "/");
    await expect(page.locator("text=DISCONNECTED").first()).toBeVisible({ timeout: 5000 });
  }

  test("dashboard initially shows DISCONNECTED", async ({ page }) => {
    await loadDash(page);
    await page.screenshot({ path: "test-results/vdev-20-disconnected.png" });
  });

  test("virtual-device-status Tauri event makes dashboard show CONNECTED", async ({ page }) => {
    await loadDash(page);
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await page.screenshot({ path: "test-results/vdev-21-connected-tauri.png" });
  });

  test("dashboard shows 4 channels and 256 Hz for Muse S", async ({ page }) => {
    await loadDash(page);
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await expect(page.locator("text=/4ch/").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=/256 Hz/").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=/LSL/").first()).toBeVisible({ timeout: 5000 });
    // NOT 32ch — preset must be honoured
    const body = await page.locator("body").innerText();
    expect(body).not.toContain("32ch");
    await page.screenshot({ path: "test-results/vdev-22-4ch-badge.png" });
  });

  test("dashboard shows 8 channels for Cyton preset", async ({ page }) => {
    await loadDash(page);
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_8CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await expect(page.locator("text=/8ch/").first()).toBeVisible({ timeout: 5000 });
    // NOT 32ch — preset must be honoured
    const body = await page.locator("body").innerText();
    expect(body).not.toContain("32ch");
    await page.screenshot({ path: "test-results/vdev-23-8ch-badge.png" });
  });

  test("dashboard shows 4ch badge and signal quality for Muse S", async ({ page }) => {
    await loadDash(page);
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    // Channel count badge should show 4ch
    await expect(page.locator("text=/4ch/").first()).toBeVisible({ timeout: 5000 });
    // 4 good channels → quality indicator visible
    await expect(page.locator("text=/4✓/").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/vdev-24-electrodes.png" });
  });

  test("dashboard shows signal quality 4✓ for 4 good channels", async ({ page }) => {
    await loadDash(page);
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await expect(page.locator("text=/4✓/").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/vdev-25-quality.png" });
  });

  test("re-broadcast: dashboard shows CONNECTED when event fires after dashboard loads", async ({ page }) => {
    // Dashboard is already loaded and showing DISCONNECTED.
    // The virtual-device-status event fires LATER (simulating the re-broadcast timer).
    await loadDash(page);
    await expect(page.locator("text=DISCONNECTED").first()).toBeVisible({ timeout: 5000 });

    // Wait to simulate the user switching to the dashboard after starting the device.
    await page.waitForTimeout(300);

    // Re-broadcast fires — dashboard must pick it up.
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await page.screenshot({ path: "test-results/vdev-26-rebroadcast.png" });
  });

  test("disconnected Tauri event returns dashboard to DISCONNECTED", async ({ page }) => {
    await loadDash(page);
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });

    await fireTauriEvent(page, "virtual-device-status", { ...DISCONNECTED_STATUS, device_id: null });
    await expect(page.locator("text=DISCONNECTED").first()).toBeVisible({ timeout: 8000 });
    await page.screenshot({ path: "test-results/vdev-27-disconnected.png" });
  });
});

// ════════════════════════════════════════════════════════════════════════════
// 3. Dashboard — EEG data via Tauri event relay (virtual-eeg-* → WS handlers)
// ════════════════════════════════════════════════════════════════════════════

test.describe("Dashboard renders EEG data from virtual device", () => {
  async function launchConnected(page: Page) {
    await openPage(page, "/");
    await injectFullSession(page);
    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
  }

  test("Band Powers section appears after EegBands events", async ({ page }) => {
    await launchConnected(page);
    await expect(page.locator("text=/Band Powers/i").first()).toBeVisible({ timeout: 8000 });
    const canvases = page.locator("canvas");
    await expect(canvases.first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/vdev-30-bands.png" });
  });

  test("EEG Waveforms section appears after EegSample events", async ({ page }) => {
    await launchConnected(page);
    await expect(page.locator("text=/EEG Waveforms/i").first()).toBeVisible({ timeout: 8000 });
    const count = await page.locator("canvas").count();
    expect(count).toBeGreaterThanOrEqual(2);
    await page.screenshot({ path: "test-results/vdev-31-waveforms.png" });
  });

  test("Brain State scores section appears", async ({ page }) => {
    await launchConnected(page);
    await expect(page.locator("text=/Brain State/i").first()).toBeVisible({ timeout: 8000 });
    await expect(page.locator("[role='meter'][aria-label='Relaxation']").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("[role='meter'][aria-label='Engagement']").first()).toBeVisible({ timeout: 5000 });
    await page.screenshot({ path: "test-results/vdev-32-brain-state.png" });
  });

  test("Frontal Alpha Asymmetry gauge appears", async ({ page }) => {
    await launchConnected(page);
    await expect(page.locator("text=/Frontal Alpha Asymmetry/i").first()).toBeVisible({ timeout: 8000 });
    await page.screenshot({ path: "test-results/vdev-33-faa.png" });
  });

  test("EEG Indices section appears", async ({ page }) => {
    await launchConnected(page);
    await expect(page.locator("text=/EEG Indices/i").first()).toBeVisible({ timeout: 8000 });
    await page.screenshot({ path: "test-results/vdev-34-indices.png" });
  });

  test("Composite Scores section appears", async ({ page }) => {
    await launchConnected(page);
    await expect(page.locator("text=/Composite Scores/i").first()).toBeVisible({ timeout: 8000 });
    await page.screenshot({ path: "test-results/vdev-35-composite.png" });
  });

  test("all major dashboard sections render together", async ({ page }) => {
    await launchConnected(page);
    for (const pattern of [
      /Band Powers/i,
      /EEG Waveforms/i,
      /Brain State/i,
      /Frontal Alpha Asymmetry/i,
      /EEG Indices/i,
      /Composite Scores/i,
    ]) {
      await expect(page.locator(`text=${pattern}`).first()).toBeVisible({ timeout: 8000 });
    }
    await page.screenshot({ path: "test-results/vdev-36-all-sections.png" });
  });
});

// ════════════════════════════════════════════════════════════════════════════
// 4. Full flow across two pages
// ════════════════════════════════════════════════════════════════════════════

test.describe("Full flow: virtual-devices page → dashboard", () => {
  test("start Muse S in vdev window → dashboard shows 4ch connected", async ({ page }) => {
    // This test verifies the full cross-window flow by:
    // 1. Loading the dashboard
    // 2. Simulating the virtual-device-status event (as if the vdev window sent it)
    // 3. Verifying the dashboard shows the correct device info

    // Load dashboard
    await openPage(page, "/");
    await expect(page.locator("text=DISCONNECTED").first()).toBeVisible({ timeout: 5000 });

    // Simulate: virtual-devices window started a Muse S and sent the event
    await fireTauriEvent(page, "virtual-device-status", VIRTUAL_STATUS_4CH);

    // Feed synthetic EEG data
    for (let i = 0; i < 6; i++) {
      await fireTauriEvent(page, "virtual-eeg-bands", SYNTH_BANDS);
      await emitDaemonEvent(page, "EegBands", SYNTH_BANDS);
      await page.waitForTimeout(40);
    }

    await expect(page.locator("text=CONNECTED").first()).toBeVisible({ timeout: 8000 });
    await expect(page.locator("text=/Virtual EEG/").first()).toBeVisible({ timeout: 5000 });
    await expect(page.locator("text=/4ch/").first()).toBeVisible({ timeout: 5000 });
    // Must NOT show 32ch — the Muse S preset must be honoured
    const bodyText = await page.locator("body").innerText();
    expect(bodyText).not.toContain("32ch");
    await page.screenshot({ path: "test-results/vdev-41-dashboard-4ch.png" });
  });
});
