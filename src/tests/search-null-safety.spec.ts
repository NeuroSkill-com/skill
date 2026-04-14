// SPDX-License-Identifier: GPL-3.0-only
// Playwright test: search page null-safety for edge cases.
// Verifies the search page doesn't crash when results contain null/undefined fields.

import { expect, test } from "@playwright/test";
import { buildDaemonMockScript } from "./helpers/daemon-mock";

const MOCK_COMMANDS = {
  get_status: { state: "disconnected" },
  get_cortex_ws_state: { state: "disconnected" },
  get_ws_port: { port: 18445 },
  get_ws_clients: [],
  list_sessions: [],
  list_all_sessions: [],
  list_embedding_sessions: [],
  get_hooks: [],
  get_hook_statuses: {},
  get_dnd_config: { enabled: false },
  get_dnd_active: false,
  get_dnd_status: { active: false },
  list_focus_modes: [],
  get_daily_goal: { minutes: 60 },
  get_goal_notified_date: null,
  get_filter_config: {},
  get_embedding_overlap: { overlap_secs: 0 },
  get_exg_inference_device: { device: "cpu" },
  get_eeg_model_config: {
    model_backend: "zuna",
    hf_repo: "Zyphra/ZUNA",
    hnsw_m: 16,
    hnsw_ef_construction: 200,
    data_norm: 10,
  },
  get_eeg_model_status: { encoder_loaded: false, weights_found: false, downloading_weights: false },
  get_screenshot_config: { enabled: false },
  get_screenshot_metrics: {},
  get_screenshots_dir: ["/tmp/screenshots", 18445],
  get_reembed_config: {
    idle_reembed_enabled: false,
    idle_reembed_delay_secs: 1800,
    idle_reembed_gpu: true,
    gpu_precision: "f16",
    idle_reembed_throttle_ms: 10,
    batch_size: 10,
    batch_delay_ms: 50,
  },
  estimate_reembed: { total_epochs: 0, embeddings_needed: 0 },
  get_sleep_config: {},
  get_umap_config: {},
  get_gpu_stats: {},
  get_main_window_auto_fit: false,

  // EEG search: return results with some null fields to test null-safety.
  stream_search_embeddings: {
    start_utc: 1775784303,
    end_utc: 1775785018,
    k: 3,
    ef: 50,
    query_count: 2,
    searched_days: ["20260410"],
    results: [
      {
        timestamp: 1775784303000,
        timestamp_unix: 1775784303,
        neighbors: [
          {
            hnsw_id: 1,
            timestamp: 1775784400000,
            timestamp_unix: 1775784400,
            distance: 0.123,
            date: "20260410",
            device_id: null,
            device_name: null,
            labels: [],
            metrics: null,
          },
          {
            hnsw_id: 2,
            timestamp: 1775784500000,
            timestamp_unix: 1775784500,
            distance: 0.456,
            date: "20260410",
            device_id: null,
            device_name: "TestDevice",
            // labels intentionally missing (undefined)
            metrics: { faa: 0.5, mood: 50 },
          },
        ],
      },
      {
        timestamp: 1775784400000,
        timestamp_unix: 1775784400,
        // neighbors intentionally missing (undefined)
      },
    ],
  },

  // Text search: return results with some null fields.
  search_labels_by_text: {
    results: [
      {
        label_id: 1,
        text: "test label",
        context: null,
        eeg_start: 1775784303,
        eeg_end: 1775784400,
        created_at: 1775784303,
        distance: 0.1,
      },
      {
        label_id: 2,
        text: null,
        context: "test context",
        eeg_start: 0,
        eeg_end: 0,
        created_at: 0,
        distance: 0.5,
      },
    ],
  },

  // Interactive search: return nodes with null fields.
  interactive_search: {
    nodes: [
      { id: "q0", kind: "query", text: "test", distance: 0 },
      { id: "tl0", kind: "text_label", text: null, distance: 0.1, parent_id: "q0" },
      { id: "ep0", kind: "eeg_point", timestamp_unix: null, distance: 0, parent_id: "tl0" },
    ],
    edges: [
      { from_id: "q0", to_id: "tl0", distance: 0.1, kind: "text_sim" },
      { from_id: "tl0", to_id: "ep0", distance: 0, kind: "eeg_bridge" },
    ],
    dot: "",
    svg: "",
    svg_col: "",
  },
};

test.describe("search page null-safety", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript({ content: buildDaemonMockScript(MOCK_COMMANDS) });
  });

  test("EEG search renders without crashing on null neighbor fields", async ({ page }) => {
    await page.goto("/search?mode=eeg");
    await page.waitForLoadState("networkidle");

    // No console errors from null access.
    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    // Fill in a time range and trigger search.
    const startInput = page.locator('input[aria-label="Start time"]').first();
    const endInput = page.locator('input[aria-label="End time"]').first();
    if (await startInput.isVisible()) {
      await startInput.fill("2026-04-10T00:00");
      await endInput.fill("2026-04-10T01:00");
    }

    // Click search button.
    const searchBtn = page.getByRole("button", { name: /search/i }).first();
    if (await searchBtn.isVisible()) {
      await searchBtn.click();
      await page.waitForTimeout(1000);
    }

    // Check no TypeError from null access.
    const nullErrors = errors.filter(
      (e) => e.includes("Cannot access property") || e.includes("undefined is not an object"),
    );
    expect(nullErrors).toEqual([]);
  });

  test("text search renders without crashing on null label fields", async ({ page }) => {
    await page.goto("/search?mode=text");
    await page.waitForLoadState("networkidle");

    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    // Type a query and search.
    const input = page.locator('input[type="text"]').first();
    if (await input.isVisible()) {
      await input.fill("test");
      await input.press("Enter");
      await page.waitForTimeout(1000);
    }

    const nullErrors = errors.filter(
      (e) => e.includes("Cannot access property") || e.includes("undefined is not an object"),
    );
    expect(nullErrors).toEqual([]);
  });

  test("interactive search renders without crashing on null node fields", async ({ page }) => {
    await page.goto("/search?mode=interactive");
    await page.waitForLoadState("networkidle");

    const errors: string[] = [];
    page.on("pageerror", (err) => errors.push(err.message));

    // Type a query and search.
    const input = page.locator('input[type="text"]').first();
    if (await input.isVisible()) {
      await input.fill("test");
      await input.press("Enter");
      await page.waitForTimeout(1000);
    }

    const nullErrors = errors.filter(
      (e) => e.includes("Cannot access property") || e.includes("undefined is not an object"),
    );
    expect(nullErrors).toEqual([]);
  });
});
