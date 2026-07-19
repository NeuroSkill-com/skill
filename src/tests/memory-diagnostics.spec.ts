/**
 * Playwright e2e tests for the new memory-diagnostics + RSS backpressure UI.
 *
 * Covers:
 *   1. EmbeddingsTab renders the per-backend "On-disk footprint" block
 *      derived from the new `memory` field on /labels/index/stats.
 *   2. EegModelTab renders the amber "deferred — system memory at X%"
 *      banner when idle_reembed.memory_throttled is true.
 *   3. EegModelTab renders the max_resident_memory_percent slider
 *      and reflects the value coming from get_reembed_config.
 *
 * The Tauri IPC is fully mocked via an init script so this runs against
 * `vite dev` without needing a real daemon.
 *
 * Run:  npx playwright test src/tests/memory-diagnostics.spec.ts
 */
import { expect, type Page, test } from "@playwright/test";

// ── Tauri IPC mock ──────────────────────────────────────────────────────────

interface MockOverrides {
  reembedConfig?: Record<string, unknown>;
  /** Full replacement for label index stats. When omitted, a default with a
   * `memory` block is used. Set to an object without `memory` to simulate a
   * legacy daemon. */
  labelIndexStats?: Record<string, unknown> | null;
  reembedEstimate?: Record<string, unknown>;
}

function buildMockScript(overrides: MockOverrides = {}) {
  // Serialize overrides into the page-side script via JSON.
  const reembedConfig = JSON.stringify({
    auto_labels: false,
    auto_eeg: false,
    auto_screenshots: false,
    batch_size: 10,
    batch_delay_ms: 50,
    idle_reembed_enabled: true,
    idle_reembed_delay_secs: 1800,
    idle_reembed_gpu: true,
    gpu_precision: "f16",
    idle_reembed_throttle_ms: 200,
    max_resident_memory_percent: 85,
    ...(overrides.reembedConfig ?? {}),
  });
  const labelIndexStats = JSON.stringify(
    overrides.labelIndexStats !== undefined
      ? overrides.labelIndexStats
      : {
          preferred_backend: "hnsw",
          hnsw: { text_nodes: 1234, context_nodes: 567, eeg_nodes: 89 },
          turbovec: { text_nodes: 1234, context_nodes: 567, eeg_nodes: 89 },
          memory: {
            hnsw: { text_bytes: 5_242_880, context_bytes: 2_097_152, eeg_bytes: 1_048_576 },
            turbovec: { text_bytes: 524_288, context_bytes: 262_144, eeg_bytes: 131_072 },
            total_bytes: 9_306_112,
          },
        },
  );
  const reembedEstimate = JSON.stringify({
    total_epochs: 1000,
    embedded: 500,
    missing: 500,
    date_dirs: 3,
    coverage_pct: 50,
    avg_embed_ms: 12,
    eta_secs: 60,
    per_day: [],
    idle_reembed: {
      active: false,
      idle_secs: 600,
      delay_secs: 1800,
      total: 0,
      done: 0,
      current_day: "",
      memory_throttled: false,
      memory_percent: 60,
    },
    ...(overrides.reembedEstimate ?? {}),
  });

  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };

    const REEMBED_CONFIG = ${reembedConfig};
    const LABEL_INDEX_STATS = ${labelIndexStats};
    const REEMBED_ESTIMATE = ${reembedEstimate};

    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      switch (cmd) {
        // ── Embeddings tab ────────────────────────────────────────────────
        case "get_label_index_backend":
          return Promise.resolve({ backend: "hnsw" });
        case "get_label_index_stats":
          return Promise.resolve(LABEL_INDEX_STATS);
        case "get_reembed_config":
          return Promise.resolve(REEMBED_CONFIG);
        case "get_daemon_watchdog":
          return Promise.resolve({ enabled: true, timeout_secs: 10 });
        case "set_reembed_config":
        case "set_daemon_watchdog":
        case "set_label_index_backend":
        case "rebuild_label_index":
        case "trigger_reembed":
          return Promise.resolve({ ok: true });
        case "list_embedding_models":
          return Promise.resolve([]);
        case "get_embedding_model":
          return Promise.resolve({ code: "nomic-ai/nomic-embed-text-v1.5", dim: 768 });
        case "label_embedding_status":
          return Promise.resolve({ current_model: "nomic-ai/nomic-embed-text-v1.5", total: 0, stale: 0, models: {} });

        // ── EXG + EEG Model tab ──────────────────────────────────────────
        case "get_filter_config":
          return Promise.resolve({
            sample_rate: 250,
            low_pass_hz: null,
            high_pass_hz: null,
            notch: "Hz60",
            notch_bandwidth_hz: 4,
          });
        case "get_embedding_overlap":
          return Promise.resolve(0);
        case "get_exg_inference_device":
          return Promise.resolve({ value: "auto" });
        case "get_eeg_model_config":
          return Promise.resolve({
            hf_repo: "Zyphra/ZUNA",
            hnsw_m: 16,
            hnsw_ef_construction: 200,
            data_norm: 10,
            model_backend: "zuna",
            luna_variant: "base",
            luna_hf_repo: "PulpBio/LUNA",
          });
        case "get_eeg_model_status":
          return Promise.resolve({
            encoder_loaded: true,
            embeddings_today: 0,
            weights_found: true,
            weights_path: "/tmp/weights",
            active_model_backend: "zuna",
            last_embed_ms: 0,
            avg_embed_ms: 0,
            daily_db_path: "",
            daily_hnsw_path: "",
            downloading_weights: false,
            download_progress: 0,
            download_status_msg: null,
            download_needs_restart: false,
            download_retry_attempt: 0,
            download_retry_in_secs: 0,
          });
        case "estimate_reembed":
          return Promise.resolve(REEMBED_ESTIMATE);

        // ── Common ───────────────────────────────────────────────────────
        case "get_app_name":
          return Promise.resolve("NeuroSkill Test");
        case "show_main_window":
        case "show_toast_from_frontend":
          return Promise.resolve();
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

async function gotoTab(page: Page, tab: string, overrides: MockOverrides = {}) {
  await page.addInitScript({ content: buildMockScript(overrides) });
  await page.goto(`http://localhost:1420/settings?tab=${tab}`, { waitUntil: "networkidle" });
  // EmbeddingsTab + EegModelTab kick off several async loads on mount; give the
  // event loop room to apply them before assertions.
  await page.waitForTimeout(1500);
}

// ── Embeddings tab — memory footprint block ─────────────────────────────────

test.describe("Embeddings tab — label index footprint", () => {
  test("renders per-backend on-disk footprint with formatted byte values", async ({ page }) => {
    await gotoTab(page, "embeddings");

    const body = await page.locator("body").innerText();

    // Section header from i18n key embeddings.indexMemory
    expect(body).toMatch(/on-disk footprint/i);

    // Total bytes — 9_306_112 bytes ≈ 8.9 MiB
    expect(body).toMatch(/Total:\s*8\.9 MiB/i);

    // HNSW row: 5 MiB text + 2 MiB context + 1 MiB eeg = 8 MiB total.
    // fmtBytes stays in the same unit until the value reaches 1024.
    expect(body).toMatch(/HNSW:\s*8\.0 MiB/i);

    // TurboQuant row: 512 KiB + 256 KiB + 128 KiB = 896 KiB total.
    // The total stays in KiB because 896 < 1024.
    expect(body).toMatch(/TurboQuant:\s*896 KiB/i);

    await page.screenshot({ path: "test-results/memory-footprint-embeddings.png", fullPage: true });
  });

  test("hides the footprint block when memory field is absent (legacy daemon)", async ({ page }) => {
    // Full replacement of stats — no `memory` field, simulating an old daemon.
    await gotoTab(page, "embeddings", {
      labelIndexStats: {
        preferred_backend: "hnsw",
        hnsw: { text_nodes: 0, context_nodes: 0, eeg_nodes: 0 },
        turbovec: { text_nodes: 0, context_nodes: 0, eeg_nodes: 0 },
      },
    });

    const body = await page.locator("body").innerText();
    // The section heading must NOT appear because the `{#if memory}` block guards it.
    expect(body).not.toMatch(/on-disk footprint/i);
  });
});

// ── EXG tab — idle reembed throttle banner + slider ─────────────────────────

test.describe("EXG tab — RSS backpressure UI", () => {
  test("renders memory throttle banner when daemon reports memory_throttled", async ({ page }) => {
    await gotoTab(page, "exg", {
      reembedEstimate: {
        total_epochs: 1000,
        embedded: 500,
        missing: 500,
        date_dirs: 3,
        coverage_pct: 50,
        avg_embed_ms: 12,
        eta_secs: 60,
        per_day: [],
        idle_reembed: {
          active: false,
          idle_secs: 9999, // past delay
          delay_secs: 1800,
          total: 0,
          done: 0,
          current_day: "",
          memory_throttled: true,
          memory_percent: 92,
        },
      },
    });

    const body = await page.locator("body").innerText();

    // Banner text from model.idleReembedMemoryThrottled
    expect(body).toMatch(/deferred.*system memory.*92%.*limit.*85%/i);

    // The plain "Starts after Xs idle" banner must NOT appear (throttle wins).
    expect(body).not.toMatch(/starts after \d+s idle/i);

    await page.screenshot({ path: "test-results/memory-throttle-banner.png", fullPage: true });
  });

  test("does not render throttle banner when memory_throttled is false", async ({ page }) => {
    await gotoTab(page, "exg", {
      reembedEstimate: {
        total_epochs: 1000,
        embedded: 500,
        missing: 500,
        date_dirs: 3,
        coverage_pct: 50,
        avg_embed_ms: 12,
        eta_secs: 60,
        per_day: [],
        idle_reembed: {
          active: false,
          idle_secs: 600,
          delay_secs: 1800,
          total: 0,
          done: 0,
          current_day: "",
          memory_throttled: false,
          memory_percent: 60,
        },
      },
    });

    const body = await page.locator("body").innerText();
    expect(body).not.toMatch(/deferred.*system memory/i);
    // The normal countdown banner should still render since idle_secs < delay_secs.
    expect(body).toMatch(/starts after \d+s idle/i);
  });

  test("renders max system memory slider with value from config", async ({ page }) => {
    await gotoTab(page, "exg", {
      reembedConfig: { max_resident_memory_percent: 75 },
    });

    const body = await page.locator("body").innerText();

    // Label from model.maxResidentMemory
    expect(body).toMatch(/max system memory/i);

    // The numeric value should display next to the slider.
    expect(body).toMatch(/75%/);

    // The slider input itself with the right value.
    const slider = page.locator('input[type="range"][aria-label="Max system memory"]');
    await expect(slider).toBeVisible();
    await expect(slider).toHaveValue("75");

    await page.screenshot({ path: "test-results/memory-throttle-slider.png", fullPage: true });
  });

  test("slider at 100% shows the 'off' label", async ({ page }) => {
    await gotoTab(page, "exg", {
      reembedConfig: { max_resident_memory_percent: 100 },
    });

    const body = await page.locator("body").innerText();
    expect(body).toMatch(/max system memory/i);
    // model.maxResidentMemoryDisabled = "off"
    expect(body).toMatch(/\boff\b/i);
    // The literal "100%" should NOT appear in the slider value box.
    const sliderValue = page.locator('input[type="range"][aria-label="Max system memory"] + span');
    await expect(sliderValue).toHaveText(/off/i);
  });
});
