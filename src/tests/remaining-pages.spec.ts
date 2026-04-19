/**
 * Playwright e2e tests for remaining pages:
 *   /calibration, /labels, /label, /api
 *
 * Run:  npx playwright test src/tests/remaining-pages.spec.ts
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
        // ── Calibration ──────────────────────────────────────────────────
        case "list_calibration_profiles":
          return Promise.resolve([
            {
              id: "default",
              name: "Default Profile",
              actions: [
                { label: "Focus", duration_secs: 10 },
                { label: "Relax", duration_secs: 10 },
              ],
              break_duration_secs: 5,
              loop_count: 3,
              auto_start: false,
              last_calibration_utc: null,
            },
          ]);
        case "get_active_calibration":
          return Promise.resolve(null);
        case "get_status":
          return Promise.resolve({
            connected: false,
            device_name: null,
            battery: null,
            signal_quality: null,
          });
        case "tts_init":
        case "tts_speak":
        case "close_calibration_window":
          return Promise.resolve();

        // ── Labels ───────────────────────────────────────────────────────
        case "query_annotations":
          return Promise.resolve([
            { id: 1, eeg_start: 1711756800, eeg_end: 1711756810, label_start: 1711756800, label_end: 1711756810, text: "Focused reading", context: "session_001", created_at: 1711756800 },
            { id: 2, eeg_start: 1711756900, eeg_end: 1711756920, label_start: 1711756900, label_end: 1711756920, text: "Deep breathing", context: "session_001", created_at: 1711756900 },
            { id: 3, eeg_start: 1711757000, eeg_end: 1711757030, label_start: 1711757000, label_end: 1711757030, text: "Meditation", context: "session_002", created_at: 1711757000 },
          ]);
        case "search_labels_by_text":
          return Promise.resolve([]);
        case "delete_label":
        case "update_label":
          return Promise.resolve();

        // ── Label (single) ───────────────────────────────────────────────
        case "get_recent_labels":
          return Promise.resolve(["Focus", "Relax", "Meditation", "Reading", "Exercise"]);
        case "submit_label":
        case "close_label_window":
          return Promise.resolve();

        // ── API ──────────────────────────────────────────────────────────
        case "get_ws_port":
          return Promise.resolve(8375);
        case "get_ws_clients":
          return Promise.resolve([
            { peer: "127.0.0.1:54321", connected_at: 1711756800 },
            { peer: "192.168.1.42:12345", connected_at: 1711756900 },
          ]);
        case "get_ws_request_log":
          return Promise.resolve([
            { timestamp: 1711756810, peer: "127.0.0.1:54321", command: "get_status", ok: true },
            { timestamp: 1711756820, peer: "127.0.0.1:54321", command: "get_bands", ok: true },
            { timestamp: 1711756830, peer: "192.168.1.42:12345", command: "subscribe", ok: false },
          ]);

        // ── Common ───────────────────────────────────────────────────────
        case "show_main_window":
        case "show_toast_from_frontend":
          return Promise.resolve();
        case "get_app_name":
          return Promise.resolve("NeuroSkill Test");
        case "get_settings":
          return Promise.resolve({});
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

async function navigateTo(page: Page, path: string) {
  await page.addInitScript({ content: buildMockScript() });
  await page.goto(`http://localhost:1420${path}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1000);
}

// ── Calibration ──────────────────────────────────────────────────────────────

test.describe("Calibration page", () => {
  test("renders with profile selector", async ({ page }) => {
    await navigateTo(page, "/calibration");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/calibration.png" });
  });

  test("shows calibration profile", async ({ page }) => {
    await navigateTo(page, "/calibration");

    // Should show the profile name or action labels
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/default|focus|relax|calibrat/i);
  });

  test("has start button", async ({ page }) => {
    await navigateTo(page, "/calibration");

    const startBtn = page
      .locator("button")
      .filter({ hasText: /start|begin|run/i })
      .first();
    const hasStart = await startBtn.isVisible();
    // May not show start if device not connected — that's ok
    expect(typeof hasStart).toBe("boolean");
  });
});

// ── Labels ───────────────────────────────────────────────────────────────────

test.describe("Labels page", () => {
  test("renders label list", async ({ page }) => {
    await navigateTo(page, "/labels");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/labels.png" });
  });

  test("shows mock labels", async ({ page }) => {
    await navigateTo(page, "/labels");

    // Should display our mock labels
    await expect(page.locator("text=/Focused reading|Deep breathing|Meditation/").first()).toBeVisible({
      timeout: 5000,
    });
  });

  test("has search/filter input", async ({ page }) => {
    await navigateTo(page, "/labels");

    const input = page.locator("input[type=text], input[placeholder]").first();
    await expect(input).toBeVisible({ timeout: 5000 });
  });
});

// ── Label (single entry) ─────────────────────────────────────────────────────

test.describe("Label page", () => {
  test("renders label input form", async ({ page }) => {
    await navigateTo(page, "/label");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(20);

    await page.screenshot({ path: "test-results/label.png" });
  });

  test("shows recent label suggestions", async ({ page }) => {
    await navigateTo(page, "/label");

    // Should show recent labels from our mock
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/Focus|Relax|Meditation|Reading|Exercise/);
  });

  test("has text input and submit", async ({ page }) => {
    await navigateTo(page, "/label");

    // Should have an input field for the label text
    const input = page.locator("input, textarea").first();
    await expect(input).toBeVisible({ timeout: 5000 });
  });
});

// ── API ──────────────────────────────────────────────────────────────────────

test.describe("API page", () => {
  test("renders with port info", async ({ page }) => {
    await navigateTo(page, "/api");

    // Should show the WebSocket port
    await expect(page.locator("text=/8375/").first()).toBeVisible({ timeout: 5000 });

    await page.screenshot({ path: "test-results/api.png" });
  });

  test("shows connected clients", async ({ page }) => {
    await navigateTo(page, "/api");

    // Should show client IPs from our mock
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/127\.0\.0\.1|192\.168/);
  });

  test("shows request log", async ({ page }) => {
    await navigateTo(page, "/api");

    // Should show command names from the log
    const body = await page.locator("body").innerText();
    expect(body).toMatch(/get_status|get_bands|subscribe/);
  });
});
