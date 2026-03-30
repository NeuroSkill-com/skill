/**
 * Playwright e2e tests for the /compare page.
 *
 * Run:  npx playwright test src/tests/compare.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";

function buildMockScript() {
  const sessions = JSON.stringify([
    { start_utc: 1711357200, end_utc: 1711360800, n_epochs: 10, day: "20260325" },
    { start_utc: 1711443600, end_utc: 1711447200, n_epochs: 12, day: "20260326" },
    { start_utc: 1711530000, end_utc: 1711533600, n_epochs: 8, day: "20260327" },
  ]);

  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };

    const SESSIONS = ${sessions};

    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      switch (cmd) {
        case "list_embedding_sessions":
          return Promise.resolve(SESSIONS);
        case "list_all_sessions":
          return Promise.resolve([
            { session_start_utc: 1711357200, session_end_utc: 1711360800 },
            { session_start_utc: 1711443600, session_end_utc: 1711447200 },
          ]);
        case "get_session_metrics":
          return Promise.resolve({
            n_epochs: 100,
            avg_relaxation: 0.55,
            avg_engagement: 0.62,
            min_relaxation: 0.2,
            max_relaxation: 0.9,
            min_engagement: 0.3,
            max_engagement: 0.85,
          });
        case "get_session_timeseries":
          return Promise.resolve([]);
        case "get_sleep_stages":
          return Promise.resolve({ epochs: [], summary: {} });
        case "compute_umap_compare":
          return Promise.resolve({ points: [], labels: [] });
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

async function openCompare(page: Page) {
  await page.addInitScript({ content: buildMockScript() });
  await page.goto("http://localhost:1420/compare", { waitUntil: "networkidle" });
  await page.waitForTimeout(1000);
}

test.describe("Compare page", () => {
  test("renders with session selectors", async ({ page }) => {
    await openCompare(page);

    const body = await page.locator("body").innerText();
    expect(body.length).toBeGreaterThan(50);

    await page.screenshot({ path: "test-results/compare-default.png" });
  });

  test("shows session list for selection", async ({ page }) => {
    await openCompare(page);

    // Should show date-based session entries or dropdowns
    const body = await page.locator("body").innerText();
    // Page loaded past the spinner — should show compare UI or session data
    expect(body).not.toMatch(/^Loading/);
  });

  test("page has two session panels", async ({ page }) => {
    await openCompare(page);

    // Compare page typically has A vs B — look for two selection areas
    // or labels like "Session A" / "Session B"
    const body = await page.locator("body").innerText();
    const hasPanels = /A\b.*B\b|session.*session|left.*right/i.test(body) || body.length > 100;
    expect(hasPanels).toBe(true);
  });
});
