/**
 * Playwright e2e tests for the Virtual EEG tab.
 *
 * Run:  npx playwright test src/tests/virtual-eeg.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";
import { buildDaemonMockScript } from "./helpers/daemon-mock";

const COMMANDS = {
  get_app_version: "0.0.85",
  get_app_name: "NeuroSkill Test",
  get_theme_and_language: ["dark", "en"],
  show_main_window: null,
  get_status: { state: "disconnected", device_name: null, battery: 0, paired_devices: [] },
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
};

async function navigateTo(page: Page, path: string) {
  await page.addInitScript({ content: buildDaemonMockScript(COMMANDS) });
  await page.goto(`http://localhost:1420${path}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1500);
}

test.describe("Virtual EEG Tab", () => {
  test("renders the tab in settings", async ({ page }) => {
    await navigateTo(page, "/settings");

    // Find and click the Virtual EEG tab
    const tab = page.locator("text=/Virtual EEG/i").first();
    await expect(tab).toBeVisible({ timeout: 5000 });
    await tab.click();
    await page.waitForTimeout(500);

    // Should show the virtual EEG UI
    await expect(page.locator("text=/Signal Template/i").first()).toBeVisible({ timeout: 3000 });
    await expect(page.locator("text=/Sine waves/i").first()).toBeVisible();
    await expect(page.locator("text=/Good quality/i").first()).toBeVisible();

    await page.screenshot({ path: "test-results/virtual-eeg-tab.png" });
  });

  test("shows channel selection", async ({ page }) => {
    await navigateTo(page, "/settings");

    const tab = page.locator("text=/Virtual EEG/i").first();
    await tab.click();
    await page.waitForTimeout(500);

    // Should show channel buttons
    await expect(page.locator("text=/4ch/").first()).toBeVisible({ timeout: 3000 });
    await expect(page.locator("text=/8ch/").first()).toBeVisible();
    await expect(page.locator("text=/32ch/").first()).toBeVisible();

    // Should show channel labels
    await expect(page.locator("text=/TP9/").first()).toBeVisible();

    await page.screenshot({ path: "test-results/virtual-eeg-channels.png" });
  });

  test("shows signal quality options", async ({ page }) => {
    await navigateTo(page, "/settings");

    const tab = page.locator("text=/Virtual EEG/i").first();
    await tab.click();
    await page.waitForTimeout(500);

    await expect(page.locator("text=/Poor/i").first()).toBeVisible({ timeout: 3000 });
    await expect(page.locator("text=/Good/i").first()).toBeVisible();
    await expect(page.locator("text=/Excellent/i").first()).toBeVisible();
  });

  test("shows signal preview canvas", async ({ page }) => {
    await navigateTo(page, "/settings");

    const tab = page.locator("text=/Virtual EEG/i").first();
    await tab.click();
    await page.waitForTimeout(1000);

    // Canvas should exist
    const canvas = page.locator("canvas");
    await expect(canvas.first()).toBeVisible({ timeout: 3000 });

    await page.screenshot({ path: "test-results/virtual-eeg-preview.png" });
  });

  test("start/stop button toggles", async ({ page }) => {
    await navigateTo(page, "/settings");

    const tab = page.locator("text=/Virtual EEG/i").first();
    await tab.click();
    await page.waitForTimeout(500);

    // Should show Start button
    const startBtn = page.locator("button", { hasText: /^Start$/ });
    await expect(startBtn).toBeVisible({ timeout: 3000 });

    // Click Start
    await startBtn.click();
    await page.waitForTimeout(500);

    // Should now show Stop button and Running status
    await expect(page.locator("text=/Running/i").first()).toBeVisible({ timeout: 3000 });
    await expect(page.locator("button", { hasText: /^Stop$/ })).toBeVisible();

    await page.screenshot({ path: "test-results/virtual-eeg-running.png" });

    // Click Stop
    await page.locator("button", { hasText: /^Stop$/ }).click();
    await page.waitForTimeout(500);

    await expect(page.locator("text=/Stopped/i").first()).toBeVisible({ timeout: 3000 });
  });
});
