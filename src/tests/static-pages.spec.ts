/**
 * Playwright e2e tests for static / simple pages:
 *   /about, /help, /whats-new, /onboarding, /downloads, /focus-timer
 *
 * Run:  npx playwright test src/tests/static-pages.spec.ts
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
        // ── About ────────────────────────────────────────────────────────
        case "get_about_info":
          return Promise.resolve({
            name: "NeuroSkill",
            version: "0.0.78",
            tagline: "Brain-computer interface platform",
            website: "https://neuroskill.com",
            websiteLabel: "neuroskill.com",
            repoUrl: "https://github.com/NeuroSkill-com/skill",
            discordUrl: "https://discord.gg/neuroskill",
            license: "GPL-3.0-only",
            licenseName: "GNU General Public License v3.0",
            licenseUrl: "https://www.gnu.org/licenses/gpl-3.0.html",
            copyright: "© 2026 NeuroSkill.com",
            authors: [["NeuroSkill Team", "Engineering"]],
            acknowledgements: "Built with Tauri, Svelte, and Rust.",
            iconDataUrl: null,
          });

        // ── Help ─────────────────────────────────────────────────────────
        case "get_ws_port":
          return Promise.resolve(8375);
        case "get_app_name":
          return Promise.resolve("NeuroSkill Test");

        // ── What's New ───────────────────────────────────────────────────
        case "get_app_version":
          return Promise.resolve("0.0.78");
        case "dismiss_whats_new":
          return Promise.resolve();

        // ── Onboarding ───────────────────────────────────────────────────
        case "check_bluetooth_power":
          return Promise.resolve(true);
        case "get_status":
          return Promise.resolve({
            connected: false,
            device_name: null,
            battery: null,
            signal_quality: null,
          });
        case "list_calibration_profiles":
          return Promise.resolve([]);
        case "get_active_calibration":
          return Promise.resolve(null);
        case "get_llm_catalog":
          return Promise.resolve({ families: [], models: [] });
        case "get_onboarding_model_download_order":
          return Promise.resolve([]);
        case "check_screen_recording_permission":
          return Promise.resolve(true);
        case "check_ocr_models_ready":
          return Promise.resolve(true);
        case "complete_onboarding":
          return Promise.resolve();
        case "get_eeg_model_status":
          return Promise.resolve({
            encoder_loaded: false,
            embeddings_today: 0,
            weights_path: null,
          });

        // ── Downloads ────────────────────────────────────────────────────
        case "get_llm_downloads":
          return Promise.resolve([
            {
              repo: "TheBloke/test-model-7B-GGUF",
              filename: "test-model-7b-q4.gguf",
              quant: "Q4_K_M",
              size_gb: 4.2,
              description: "Test Model 7B",
              is_mmproj: false,
              state: "downloaded",
              status_msg: null,
              progress: 1.0,
              initiated_at_unix: 1711756800,
              local_path: "/tmp/test-model-7b-q4.gguf",
              shard_count: 1,
              current_shard: 1,
            },
            {
              repo: "TheBloke/another-3B-GGUF",
              filename: "another-3b-q8.gguf",
              quant: "Q8_0",
              size_gb: 3.1,
              description: "Another Model 3B",
              is_mmproj: false,
              state: "downloading",
              status_msg: "50%",
              progress: 0.5,
              initiated_at_unix: null,
              local_path: null,
              shard_count: 1,
              current_shard: 1,
            },
          ]);

        // ── Focus Timer ──────────────────────────────────────────────────
        case "get_neutts_config":
          return Promise.resolve({ enabled: false, voice: "jo" });
        case "tts_init":
          return Promise.resolve();

        // ── Common ───────────────────────────────────────────────────────
        case "show_main_window":
        case "show_toast_from_frontend":
          return Promise.resolve();
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

// ── Helpers ──────────────────────────────────────────────────────────────────

async function navigateTo(page: Page, path: string) {
  await page.addInitScript({ content: buildMockScript() });
  await page.goto(`http://localhost:1420${path}`, { waitUntil: "networkidle" });
  await page.waitForTimeout(1000);
}

// ── About ────────────────────────────────────────────────────────────────────

test.describe("About page", () => {
  test("renders app info and links", async ({ page }) => {
    await navigateTo(page, "/about");

    // App name and version visible
    await expect(page.getByRole("heading", { name: "NeuroSkill" })).toBeVisible();
    await expect(page.locator("text=0.0.78")).toBeVisible();

    // Links section
    await expect(page.getByRole("link", { name: /neuroskill\.com/ })).toBeVisible();

    await page.screenshot({ path: "test-results/about.png" });
  });
});

// ── Help ─────────────────────────────────────────────────────────────────────

test.describe("Help page", () => {
  test("renders with tabs", async ({ page }) => {
    await navigateTo(page, "/help");

    // Default tab (dashboard) should render
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/help-dashboard.png" });
  });

  test("FAQ tab renders", async ({ page }) => {
    await navigateTo(page, "/help");

    // Click FAQ tab
    const faqTab = page.locator("text=/FAQ/i").first();
    if (await faqTab.isVisible()) {
      await faqTab.click();
      await page.waitForTimeout(500);
      await page.screenshot({ path: "test-results/help-faq.png" });
    }
  });

  test("API tab renders", async ({ page }) => {
    await navigateTo(page, "/help");

    const apiTab = page.locator("text=/API/i").first();
    if (await apiTab.isVisible()) {
      await apiTab.click();
      await page.waitForTimeout(500);
      // Should show WebSocket port or API docs
      await page.screenshot({ path: "test-results/help-api.png" });
    }
  });
});

// ── What's New ───────────────────────────────────────────────────────────────

test.describe("What's New page", () => {
  test("renders changelog content", async ({ page }) => {
    await navigateTo(page, "/whats-new");

    // Should show version or changelog content
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(20);

    await page.screenshot({ path: "test-results/whats-new.png" });
  });
});

// ── Onboarding ───────────────────────────────────────────────────────────────

test.describe("Onboarding page", () => {
  test("renders welcome step", async ({ page }) => {
    await navigateTo(page, "/onboarding");

    // Welcome step should be visible
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(20);

    await page.screenshot({ path: "test-results/onboarding-welcome.png" });
  });

  test("can advance through steps", async ({ page }) => {
    await navigateTo(page, "/onboarding");

    // Look for a next/continue/skip button
    const nextBtn = page
      .locator("button")
      .filter({ hasText: /next|continue|skip|get started/i })
      .first();
    if (await nextBtn.isVisible()) {
      await nextBtn.click();
      await page.waitForTimeout(500);
      await page.screenshot({ path: "test-results/onboarding-step2.png" });
    }
  });
});

// ── Downloads ────────────────────────────────────────────────────────────────

test.describe("Downloads page", () => {
  test("renders download list", async ({ page }) => {
    await navigateTo(page, "/downloads");

    // Should show at least one filename from our mock
    await expect(page.locator("text=/test-model|another-3b|Q4_K_M|Q8_0/i").first()).toBeVisible({ timeout: 5000 });

    await page.screenshot({ path: "test-results/downloads.png" });
  });

  test("shows download progress", async ({ page }) => {
    await navigateTo(page, "/downloads");

    // The in-progress download should show some progress indicator
    const body = await page.locator("body").innerText();
    // Should contain size or percentage info
    expect(body).toMatch(/GB|MB|%|download/i);

    await page.screenshot({ path: "test-results/downloads-progress.png" });
  });
});

// ── Focus Timer ──────────────────────────────────────────────────────────────

test.describe("Focus Timer page", () => {
  test("renders timer UI", async ({ page }) => {
    await navigateTo(page, "/focus-timer");

    // Should show timer-related elements (time display, start/stop)
    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(10);

    // Look for a start button or timer display
    const hasTimerUI =
      (await page
        .locator("button")
        .filter({ hasText: /start|begin|focus/i })
        .count()) > 0 || (await page.locator("text=/\\d+:\\d+/").count()) > 0;
    expect(hasTimerUI).toBe(true);

    await page.screenshot({ path: "test-results/focus-timer.png" });
  });
});
