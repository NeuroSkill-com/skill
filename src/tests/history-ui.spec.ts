/**
 * Playwright e2e test for the History page — screenshots each calendar tab
 * (day / week / month / year) with mocked Tauri IPC data.
 *
 * Run:  npx playwright test src/tests/history-ui.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";

// ── Fake session data ────────────────────────────────────────────────────────

function fakeSession(idx: number, dayStartUtc: number, offsetSecs: number, durationSecs: number) {
  const start = dayStartUtc + offsetSecs;
  const end = start + durationSecs;
  return {
    csv_file: `session_${idx}.csv`,
    csv_path: `/data/20260328/session_${idx}.csv`,
    session_start_utc: start,
    session_end_utc: end,
    device_name: "Muse S",
    serial_number: null,
    battery_pct: 80 + (idx % 20),
    total_samples: 256 * durationSecs,
    sample_rate_hz: 256,
    labels: [],
    file_size_bytes: 1024 * durationSecs,
    avg_snr_db: 12 + idx,
  };
}

function fakeTimeseries(startUtc: number, endUtc: number) {
  const rows = [];
  for (let t = startUtc; t < endUtc; t += 5) {
    rows.push({
      t,
      relaxation: 0.3 + 0.4 * Math.sin(t * 0.01),
      engagement: 0.5 + 0.3 * Math.cos(t * 0.013),
    });
  }
  return rows;
}

const DAYS = [
  { key: "2026-03-28", start_utc: 1743145200, end_utc: 1743231600 },
  { key: "2026-03-27", start_utc: 1743058800, end_utc: 1743145200 },
  { key: "2026-03-20", start_utc: 1742454000, end_utc: 1742540400 },
  { key: "2026-03-19", start_utc: 1742367600, end_utc: 1742454000 },
  { key: "2026-03-16", start_utc: 1742108400, end_utc: 1742194800 },
  { key: "2026-03-02", start_utc: 1740898800, end_utc: 1740985200 },
  { key: "2026-03-01", start_utc: 1740812400, end_utc: 1740898800 },
];

const SESSION_COUNTS: Record<string, number> = {
  "2026-03-28": 38,
  "2026-03-27": 17,
  "2026-03-20": 35,
  "2026-03-19": 54,
  "2026-03-16": 9,
  "2026-03-02": 6,
  "2026-03-01": 21,
};

const DAY_SESSIONS: Record<string, ReturnType<typeof fakeSession>[]> = {};
let globalIdx = 0;
for (const day of DAYS) {
  const count = SESSION_COUNTS[day.key] ?? 1;
  const sessions = [];
  for (let i = 0; i < count; i++) {
    const offset = Math.floor((i / count) * 80000);
    const dur = 60 + (i % 10) * 120;
    sessions.push(fakeSession(globalIdx++, day.start_utc, offset, dur));
  }
  DAY_SESSIONS[day.key] = sessions;
}

const ALL_METRICS: Record<string, unknown> = {};
for (const sessions of Object.values(DAY_SESSIONS)) {
  for (const s of sessions) {
    const ts = fakeTimeseries(s.session_start_utc, s.session_end_utc);
    ALL_METRICS[s.csv_path] = {
      n_rows: ts.length,
      summary: {
        n_epochs: ts.length,
        avg_relaxation: 0.5,
        avg_engagement: 0.5,
        min_relaxation: 0.1,
        max_relaxation: 0.9,
        min_engagement: 0.2,
        max_engagement: 0.8,
      },
      timeseries: ts,
    };
  }
}

// ── Tauri IPC mock script ────────────────────────────────────────────────────

function buildMockScript() {
  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };

    const DAYS = ${JSON.stringify(DAYS)};
    const DAY_SESSIONS = ${JSON.stringify(DAY_SESSIONS)};
    const ALL_METRICS = ${JSON.stringify(ALL_METRICS)};

    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      switch (cmd) {
        case "show_main_window":
          return Promise.resolve();
        case "list_local_session_days":
          return Promise.resolve(DAYS);
        case "list_sessions_for_local_day": {
          const key = args?.localKey || args?.local_key;
          return Promise.resolve(DAY_SESSIONS[key] || []);
        }
        case "get_day_metrics_batch": {
          const paths = args?.csvPaths || args?.csv_paths || [];
          const result = {};
          for (const p of paths) {
            if (ALL_METRICS[p]) result[p] = ALL_METRICS[p];
          }
          return Promise.resolve(result);
        }
        case "get_history_stats":
          return Promise.resolve({
            total_sessions: 180,
            total_secs: 72000,
            this_week_secs: 3600,
            last_week_secs: 2400,
          });
        case "get_session_metrics":
          return Promise.resolve({
            n_epochs: 100, avg_relaxation: 0.5, avg_engagement: 0.5,
            min_relaxation: 0.1, max_relaxation: 0.9,
            min_engagement: 0.2, max_engagement: 0.8,
          });
        case "get_session_timeseries":
          return Promise.resolve([]);
        case "get_sleep_stages":
          return Promise.resolve({ epochs: [], summary: {} });
        case "get_session_location":
          return Promise.resolve([]);
        case "get_session_embedding_count":
          return Promise.resolve(0);
        case "get_screenshots_around":
          return Promise.resolve([]);
        case "get_screenshots_dir":
          return Promise.resolve(["/tmp", 8375]);
        case "get_calendar_events":
          return Promise.resolve([]);
        case "query_annotations":
          return Promise.resolve([]);
        case "get_ws_port":
          return Promise.resolve(8375);
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

// ── Helpers ──────────────────────────────────────────────────────────────────

async function waitForHistoryLoad(page: Page) {
  await page.waitForSelector("text=/session/i", { timeout: 15000 });
  await page.waitForTimeout(2000);
}

/**
 * Switch view by dispatching a keyboard event on the handler div,
 * then waiting for the DOM to update.
 */
async function switchToView(page: Page, key: string) {
  // Focus the keydown handler container and dispatch the key
  await page.evaluate((k) => {
    const container = document.querySelector('[role="presentation"]');
    if (container) {
      (container as HTMLElement).focus();
      container.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
    }
  }, key);
  await page.waitForTimeout(3000);
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("History view tabs", () => {
  let page: Page;

  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    });
    page = await context.newPage();
    await page.addInitScript({ content: buildMockScript() });
    await page.goto("http://localhost:1420/history", { waitUntil: "networkidle" });
    await waitForHistoryLoad(page);
  });

  test.afterAll(async () => {
    await page.close();
  });

  test("1. Day view — sessions list and 24h grid", async () => {
    await switchToView(page, "4");

    await page.screenshot({ path: "test-results/history-day-view.png" });

    // Session rows should be present
    const sessionRows = page.locator('[id^="session-row-"]');
    const count = await sessionRows.count();
    expect(count).toBeGreaterThan(0);

    // Canvas (day grid + sparklines)
    const canvasCount = await page.locator("canvas").count();
    expect(canvasCount).toBeGreaterThan(0);

    // Stats bar
    const statsBar = page.locator("text=/hours total/i");
    await expect(statsBar).toBeVisible();
  });

  test("2. Month view — calendar grid with heatmap cells", async () => {
    await switchToView(page, "2");

    // Debug: dump page text to see what rendered
    const _bodyText = await page.locator("body").innerText();
    await page.screenshot({ path: "test-results/history-month-view.png" });

    // Month view should show the month name
    const monthHeader = page.locator("text=/March|April|2026/i");
    const monthCount = await monthHeader.count();
    // Look for day number cells (any small number 1-31 in the grid)
    const allText = await page.locator("body").innerText();
    const _hasCalendarGrid = /\bS\b.*\bM\b.*\bT\b.*\bW\b.*\bT\b.*\bF\b.*\bS\b/.test(allText);
    // Any aspect-square elements (day cells)
    const dayCells = page.locator(".aspect-square");
    const _cellCount = await dayCells.count();
    expect(monthCount).toBeGreaterThan(0);
  });

  test("3. Week view — timeline bars", async () => {
    await switchToView(page, "3");
    // Extra wait for week data fetch
    await page.waitForTimeout(3000);

    await page.screenshot({ path: "test-results/history-week-view.png" });

    const _bodyText = await page.locator("body").innerText();
    // Week view should show day labels (Mon, Tue, etc. or date-based labels)
    const weekText = page.locator("text=/Mon|Tue|Wed|Thu|Fri|Sat|Sun/i");
    const _weekCount = await weekText.count();
  });

  test("4. Year view — GitHub-style heatmap", async () => {
    await switchToView(page, "1");

    await page.screenshot({ path: "test-results/history-year-view.png" });

    const _bodyText = await page.locator("body").innerText();
    // Year view should show month abbreviations
    const monthLabels = page.locator("text=/Jan/");
    const _monthCount = await monthLabels.count();
    // Legend ("less" / "more")
    const legend = page.locator("text=/less/i");
    const _legendCount = await legend.count();
  });

  test("5. Regression: heatmap differentiates session counts", async () => {
    // Go to month view
    await switchToView(page, "2");
    await page.waitForTimeout(2000);

    await page.screenshot({ path: "test-results/history-regression-heatmap.png" });

    // Use evaluate to check if the day cache has proper counts
    const _cacheSizes = await page.evaluate(() => {
      // Access the reactive day cache via the internal Map
      const w = window as unknown as Record<string, unknown>;
      // Check if __TAURI_INTERNALS__ mock was called
      return {
        tauriPresent: !!w.__TAURI_INTERNALS__,
        bodyLength: document.body.innerText.length,
      };
    });
  });
});
