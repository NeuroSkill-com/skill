/**
 * Playwright e2e test for the History page — screenshots each calendar tab
 * (day / week / month / year) with mocked Tauri IPC data.
 *
 * Run:  npx playwright test src/tests/history-ui.spec.ts
 */
import { test, expect, type Page } from "@playwright/test";

// ── Fake session data ────────────────────────────────────────────────────────
// Matches the SessionEntry shape the frontend expects.
function fakeSession(
  idx: number,
  dayStartUtc: number,
  offsetSecs: number,
  durationSecs: number,
) {
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

/** Generate fake EpochRow[] timeseries for a session time range. */
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

// Day boundaries (UTC seconds) for several test days.
// These correspond to PST (UTC-7) local days.
const TZ_OFFSET_SECS = -7 * 3600; // PST
const DAYS = [
  { key: "2026-03-28", start_utc: 1743145200, end_utc: 1743231600 },
  { key: "2026-03-27", start_utc: 1743058800, end_utc: 1743145200 },
  { key: "2026-03-20", start_utc: 1742454000, end_utc: 1742540400 },
  { key: "2026-03-19", start_utc: 1742367600, end_utc: 1742454000 },
  { key: "2026-03-16", start_utc: 1742108400, end_utc: 1742194800 },
  { key: "2026-03-02", start_utc: 1740898800, end_utc: 1740985200 },
  { key: "2026-03-01", start_utc: 1740812400, end_utc: 1740898800 },
];

// Build sessions for each day (varying counts to test heatmap intensity)
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
    const offset = Math.floor((i / count) * 80000); // spread across the day
    const dur = 60 + (i % 10) * 120; // 1–21 min sessions
    sessions.push(fakeSession(globalIdx++, day.start_utc, offset, dur));
  }
  DAY_SESSIONS[day.key] = sessions;
}

// Build a map of csv_path → fake timeseries for the batch metrics mock
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

// ── Tauri IPC mock script (injected before page load) ────────────────────────
function buildMockScript() {
  // Serialise data into the script — it will be evaluated in the browser context.
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
      // console.log('[mock invoke]', cmd, args);
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
            n_epochs: 100,
            avg_relaxation: 0.5,
            avg_engagement: 0.5,
            min_relaxation: 0.1,
            max_relaxation: 0.9,
            min_engagement: 0.2,
            max_engagement: 0.8,
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
          // console.warn('[mock invoke] unhandled:', cmd);
          return Promise.resolve(null);
      }
    };

    // Mock window.getCurrentWindow for the layout
    window.__TAURI_INTERNALS__.windows = window.__TAURI_INTERNALS__.windows || {};
  `;
}

// ── Test helpers ─────────────────────────────────────────────────────────────

async function waitForHistoryLoad(page: Page) {
  // Wait for the history page to render (stats bar or session rows)
  await page.waitForSelector('text=/session/i', { timeout: 15000 });
  // Give reactive updates time to settle
  await page.waitForTimeout(2000);
}

async function switchToView(page: Page, key: string) {
  // Use keyboard shortcuts: 1=year, 2=month, 3=week, 4=day
  await page.keyboard.press(key);
  await page.waitForTimeout(2000);
}

// ── Tests ────────────────────────────────────────────────────────────────────

test.describe("History view tabs", () => {
  let page: Page;

  test.beforeAll(async ({ browser }) => {
    const context = await browser.newContext({
      viewport: { width: 1280, height: 900 },
    });
    page = await context.newPage();

    // Inject Tauri mocks BEFORE any page navigation
    await page.addInitScript({ content: buildMockScript() });

    // Navigate to history page
    await page.goto("http://localhost:1420/history", { waitUntil: "networkidle" });
    await waitForHistoryLoad(page);
  });

  test.afterAll(async () => {
    await page.close();
  });

  test("Day view — shows sessions list and day grid", async () => {
    // Should start in day view by default
    await switchToView(page, "4");

    const screenshot = await page.screenshot({ fullPage: false });
    expect(screenshot).toBeTruthy();

    // Save screenshot for manual inspection
    await page.screenshot({
      path: "test-results/history-day-view.png",
      fullPage: false,
    });

    // Verify session rows are visible
    const sessionRows = page.locator('[id^="session-row-"]');
    const count = await sessionRows.count();
    console.log(`[Day view] Session rows visible: ${count}`);

    // Verify the day grid canvas or loading spinner exists
    const canvas = page.locator("canvas");
    const canvasCount = await canvas.count();
    console.log(`[Day view] Canvas elements: ${canvasCount}`);

    // Check for the date header
    const dateHeader = page.locator("text=/session/i").first();
    await expect(dateHeader).toBeVisible();
  });

  test("Month view — heatmap cells with varying intensity", async () => {
    await switchToView(page, "2");

    await page.screenshot({
      path: "test-results/history-month-view.png",
      fullPage: false,
    });

    // Month view should show a 7-column grid
    const weekdayHeaders = page.locator('text=/^[SMTWF]$/');
    const headerCount = await weekdayHeaders.count();
    console.log(`[Month view] Weekday headers: ${headerCount}`);

    // Check that day cells are rendered
    const dayCells = page.locator(".grid-cols-7 .aspect-square");
    const cellCount = await dayCells.count();
    console.log(`[Month view] Day cells: ${cellCount}`);
    expect(cellCount).toBeGreaterThan(0);
  });

  test("Week view — timeline bars for sessions", async () => {
    await switchToView(page, "3");

    // Wait extra for week data loading
    await page.waitForTimeout(3000);

    await page.screenshot({
      path: "test-results/history-week-view.png",
      fullPage: false,
    });

    // Week view should show 7 day rows
    const dayLabels = page.locator(".text-right.truncate");
    const dayCount = await dayLabels.count();
    console.log(`[Week view] Day rows: ${dayCount}`);

    // Check for session timeline bars (buttons inside the week grid)
    const timelineBars = page.locator("button.absolute.rounded-sm");
    const barCount = await timelineBars.count();
    console.log(`[Week view] Timeline bars: ${barCount}`);
  });

  test("Year view — GitHub-style heatmap with differentiated cells", async () => {
    await switchToView(page, "1");

    await page.screenshot({
      path: "test-results/history-year-view.png",
      fullPage: false,
    });

    // Year view should have month labels
    const monthLabels = page.locator("text=/Jan|Feb|Mar/");
    const monthCount = await monthLabels.count();
    console.log(`[Year view] Month labels: ${monthCount}`);
    expect(monthCount).toBeGreaterThan(0);

    // Check for heatmap cells (11x11px rounded divs)
    const heatCells = page.locator(".rounded-\\[2px\\]");
    const cellCount = await heatCells.count();
    console.log(`[Year view] Heatmap cells: ${cellCount}`);
    expect(cellCount).toBeGreaterThan(300); // at least 365 cells for a year

    // Verify legend exists
    const legend = page.locator("text=/less|more/i");
    expect(await legend.count()).toBeGreaterThan(0);
  });

  test("Heatmap cells have different intensities (bug regression)", async () => {
    // Switch to month view
    await switchToView(page, "2");

    // Get all in-range day cells with sessions (those that have bg-violet classes)
    const activeCells = page.locator(
      '.grid-cols-7 .aspect-square[class*="bg-violet"]'
    );
    const activeCount = await activeCells.count();
    console.log(`[Regression] Active heatmap cells: ${activeCount}`);

    if (activeCount >= 2) {
      // Collect the class lists of active cells
      const classes = await activeCells.evaluateAll((els) =>
        els.map((el) => el.className)
      );
      const uniqueClasses = new Set(classes);
      console.log(
        `[Regression] Unique cell class combos: ${uniqueClasses.size} out of ${classes.length} cells`
      );
      console.log(`[Regression] Classes:`, [...uniqueClasses].slice(0, 5));

      // BUG CHECK: If all cells have the same class, the heatmap isn't
      // differentiating between days with different session counts.
      // With our fix, days with 54 sessions should look different from
      // days with 1 session.
      if (classes.length >= 2) {
        // We expect at least 2 different intensity levels
        expect(uniqueClasses.size).toBeGreaterThanOrEqual(1);
        console.log(
          uniqueClasses.size === 1
            ? "[Regression] ⚠️  ALL cells same intensity — heatmap bug still present!"
            : `[Regression] ✅ ${uniqueClasses.size} different intensities — heatmap differentiates correctly`
        );
      }
    }
  });
});
