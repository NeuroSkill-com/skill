/**
 * Playwright e2e tests for the /search page.
 *
 * Run:  npx playwright test src/tests/search.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";

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
        case "search_labels_by_text":
          return Promise.resolve([]);
        case "search_screenshots_by_text":
          return Promise.resolve([]);
        case "get_screenshots_dir":
          return Promise.resolve(["/tmp/screenshots", 0]);
        case "get_screenshots_around":
          return Promise.resolve([]);
        case "stream_search_embeddings":
          return Promise.resolve();
        case "regenerate_interactive_svg":
        case "regenerate_interactive_dot":
          return Promise.resolve("<svg></svg>");
        case "save_svg_file":
        case "save_dot_file":
          return Promise.resolve("/tmp/graph.svg");
        case "find_session_for_timestamp":
          return Promise.resolve(null);
        case "poll_job":
          return Promise.resolve({ status: "not_found" });
        case "enqueue_umap_compare":
          return Promise.resolve({ job_id: 1, queue_position: 0 });

        case "show_main_window":
        case "show_toast_from_frontend":
          return Promise.resolve();
        case "get_app_name":
          return Promise.resolve("NeuroSkill Test");
        case "get_settings":
          return Promise.resolve({});
        case "get_ws_port":
          return Promise.resolve(8375);
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

async function openSearch(page: Page, mode?: string) {
  await page.addInitScript({ content: buildMockScript() });
  const url = mode ? `http://localhost:1420/search?mode=${mode}` : "http://localhost:1420/search";
  await page.goto(url, { waitUntil: "networkidle" });
  await page.waitForTimeout(1000);
}

test.describe("Search page", () => {
  test("renders with search UI", async ({ page }) => {
    await openSearch(page);

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/search-default.png" });
  });

  test("interactive mode renders", async ({ page }) => {
    await openSearch(page, "interactive");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/search-interactive.png" });
  });

  test("text mode renders", async ({ page }) => {
    await openSearch(page, "text");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/search-text.png" });
  });

  test("images mode renders", async ({ page }) => {
    await openSearch(page, "images");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/search-images.png" });
  });

  test("eeg mode renders", async ({ page }) => {
    await openSearch(page, "eeg");

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/search-eeg.png" });
  });

  test("has mode switcher buttons", async ({ page }) => {
    await openSearch(page);

    // Should have buttons or tabs for switching modes
    const hasInteractive = (await page.locator("text=/interactive/i").count()) > 0;
    const hasText = (await page.locator("text=/text/i").count()) > 0;
    const hasImages = (await page.locator("text=/image/i").count()) > 0;
    expect(hasInteractive || hasText || hasImages).toBe(true);
  });

  test("no infinite loop when switching modes", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    await openSearch(page, "interactive");

    // Switch modes rapidly to provoke any reactive loop
    for (const m of ["text", "eeg", "interactive", "images", "interactive"]) {
      await page.evaluate((mode) => {
        window.dispatchEvent(new CustomEvent("skill:search-set-mode", { detail: { mode } }));
      }, m);
      await page.waitForTimeout(200);
    }

    await page.waitForTimeout(1000);

    const loopErrors = errors.filter((e) => e.includes("effect_update_depth_exceeded") || e.includes("infinite"));
    expect(loopErrors).toHaveLength(0);

    await page.screenshot({ path: "test-results/search-no-infinite-loop.png" });
  });

  test("URL updates mode param without using native history API", async ({ page }) => {
    await openSearch(page, "interactive");

    await page.evaluate(() => {
      window.dispatchEvent(new CustomEvent("skill:search-set-mode", { detail: { mode: "text" } }));
    });
    await page.waitForTimeout(500);

    expect(page.url()).toContain("mode=text");
  });

  test("no replaceState error before router is initialized", async ({ page }) => {
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    // Navigate without waiting for networkidle to catch early init errors
    await page.addInitScript({ content: buildMockScript() });
    await page.goto("http://localhost:1420/search?mode=eeg");
    await page.waitForTimeout(2000);

    const routerErrors = errors.filter((e) => e.includes("replaceState") || e.includes("router is initialized"));
    expect(routerErrors).toHaveLength(0);
  });

  test("insights panel renders after interactive search", async ({ page }) => {
    const mockWithResults = buildMockScript().replace(
      'case "poll_job":',
      `case "interactive_search":
          return Promise.resolve({
            nodes: [
              { id: "q", kind: "query", text: "test", distance: 0, x: 0, y: 0 },
              { id: "t1", kind: "text_label", text: "hello world", distance: 0.3, x: 1, y: 1, timestamp_unix: 1711357200 },
              { id: "e1", kind: "eeg_point", text: "", distance: 0.5, x: 2, y: 2, timestamp_unix: 1711357260, eeg_metrics: { engagement: 0.7, relaxation: 0.4, snr: 5.0 } },
              { id: "e2", kind: "eeg_point", text: "", distance: 0.6, x: 3, y: 3, timestamp_unix: 1711360800, eeg_metrics: { engagement: 0.9, relaxation: 0.3, snr: 6.0 } },
              { id: "s1", kind: "screenshot", text: "", distance: 0.4, x: 4, y: 4, timestamp_unix: 1711357260, app_name: "Safari" },
            ],
            edges: [],
            dot: "digraph{}",
            svg: "<svg></svg>",
            svg_col: "<svg></svg>",
            perf: null,
            sessions: [{ session_id: "s1", epoch_count: 2, duration_secs: 3600, best: true, avg_engagement: 0.8, avg_snr: 5.5 }],
          });
        case "poll_job":`,
    );

    await page.addInitScript({ content: mockWithResults });
    await page.goto("http://localhost:1420/search?mode=interactive", { waitUntil: "networkidle" });
    await page.waitForTimeout(1000);

    const input = page.locator("textarea").first();
    await input.fill("test query");
    await page.locator('button:has-text("Interactive")').click();
    await page.waitForTimeout(1500);

    const insightsButton = page.locator("text=/Insights/i").first();
    const insightsVisible = await insightsButton.isVisible().catch(() => false);

    if (insightsVisible) {
      await insightsButton.click();
      await page.waitForTimeout(500);

      const panelText = await page.locator("body").innerText();
      const hasContent =
        panelText.includes("Peak engagement") || panelText.includes("Safari") || panelText.includes("engagement");
      expect(hasContent).toBe(true);
    }

    await page.screenshot({ path: "test-results/search-insights.png" });
  });

  test("insights shows pending message when EEG metrics not yet computed", async ({ page }) => {
    const mockPending = buildMockScript().replace(
      'case "poll_job":',
      `case "interactive_search":
          return Promise.resolve({
            nodes: [
              { id: "q", kind: "query", text: "test", distance: 0, x: 0, y: 0 },
              { id: "e1", kind: "eeg_point", text: "", distance: 0.5, x: 1, y: 1, timestamp_unix: 1711357260, eeg_metrics: {} },
              { id: "e2", kind: "eeg_point", text: "", distance: 0.6, x: 2, y: 2, timestamp_unix: 1711360800, eeg_metrics: null },
            ],
            edges: [],
            dot: "digraph{}",
            svg: "<svg></svg>",
            svg_col: "<svg></svg>",
            perf: null,
            sessions: [],
          });
        case "poll_job":`,
    );

    await page.addInitScript({ content: mockPending });
    await page.goto("http://localhost:1420/search?mode=interactive", { waitUntil: "networkidle" });
    await page.waitForTimeout(1000);

    const input = page.locator("textarea").first();
    await input.fill("test");
    await page.locator('button:has-text("Interactive")').click();
    await page.waitForTimeout(1500);

    const insightsButton = page.locator("text=/Insights/i").first();
    if (await insightsButton.isVisible().catch(() => false)) {
      await insightsButton.click();
      await page.waitForTimeout(500);

      const body = await page.locator("body").innerText();
      const hasFeedback =
        body.includes("still being computed") ||
        body.includes("metrics") ||
        body.includes("Not enough") ||
        body.includes("No EEG");
      expect(hasFeedback).toBe(true);
    }

    await page.screenshot({ path: "test-results/search-insights-pending.png" });
  });
});
