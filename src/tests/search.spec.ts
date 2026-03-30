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
});
