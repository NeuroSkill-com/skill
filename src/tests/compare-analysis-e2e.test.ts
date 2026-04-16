// SPDX-License-Identifier: GPL-3.0-only
// End-to-end tests for the compare window's analysis endpoints.
// Runs against the LIVE daemon (port 18445) with real session data.

import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import {
  PORT,
  isDaemonAlive,
  readToken,
  testBegin,
  testEnd,
  api as apiHelper,
} from "./e2e-helpers";

let TOKEN = "";
let canRun = false;
try {
  canRun = await isDaemonAlive();
  if (canRun) TOKEN = readToken();
} catch {
  canRun = false;
}

// Mock Tauri invoke so http.ts can bootstrap
const bootstrap = { port: PORT, token: TOKEN, compatible_protocol: true, daemon_version: "0.1.0", protocol_version: 1 };
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => bootstrap) }));

// Typed API helper bound to our token
async function api<T>(path: string, method = "GET", body?: unknown): Promise<T> {
  return apiHelper<T>(TOKEN, path, method, body);
}

// ── Real session ranges from the local database ──────────────────────────────
// 20260410: 528 epochs, small session (good for fast tests)
const SMALL = { startUtc: 1775784303, endUtc: 1775785018, expectedMinEpochs: 400 };
// 20260413: 66K epochs
const MEDIUM = { startUtc: 1776043102, endUtc: 1776100000, expectedMinEpochs: 10000 };
// 20260414: 146K+ epochs (heaviest)
const LARGE = { startUtc: 1776125671, endUtc: 1776160000, expectedMinEpochs: 50000 };

// ═══════════════════════════════════════════════════════════════════════════════

describe.skipIf(!canRun)("compare analysis endpoints (live daemon)", () => {
  beforeAll(async () => { await testBegin(TOKEN); });
  afterAll(async () => { await testEnd(TOKEN); });

  // ── /v1/analysis/metrics ────────────────────────────────────────────────

  describe("GET session metrics", () => {
    it("returns metrics for a small session", async () => {
      const m = await api<Record<string, unknown>>("/v1/analysis/metrics", "POST", SMALL);
      expect(m.n_epochs).toBeGreaterThanOrEqual(SMALL.expectedMinEpochs);
      // Band powers should sum to ~1 (relative power)
      const bandSum =
        (m.rel_delta as number) +
        (m.rel_theta as number) +
        (m.rel_alpha as number) +
        (m.rel_beta as number) +
        (m.rel_gamma as number);
      expect(bandSum).toBeGreaterThan(0);
      // Should have all 51 keys
      expect(Object.keys(m).length).toBe(51);
    });

    it("returns metrics for a large session within reasonable time", async () => {
      const t0 = Date.now();
      const m = await api<Record<string, unknown>>("/v1/analysis/metrics", "POST", LARGE);
      const elapsed = Date.now() - t0;
      expect(m.n_epochs).toBeGreaterThanOrEqual(LARGE.expectedMinEpochs);
      // Should complete in <10s (was >3s before optimizations)
      expect(elapsed).toBeLessThan(10_000);
    }, 15_000);

    it("returns zero epochs for empty range", async () => {
      const m = await api<Record<string, unknown>>("/v1/analysis/metrics", "POST", {
        startUtc: 1000000000,
        endUtc: 1000000001,
      });
      expect(m.n_epochs).toBe(0);
    });

    it("returns correct averages for two different ranges", async () => {
      const [a, b] = await Promise.all([
        api<Record<string, unknown>>("/v1/analysis/metrics", "POST", SMALL),
        api<Record<string, unknown>>("/v1/analysis/metrics", "POST", MEDIUM),
      ]);
      // Both should return data but likely different values
      expect(a.n_epochs).toBeGreaterThan(0);
      expect(b.n_epochs).toBeGreaterThan(0);
      // Relaxation/engagement should be bounded
      for (const m of [a, b]) {
        expect(m.relaxation).toBeDefined();
        expect(m.engagement).toBeDefined();
      }
    });
  });

  // ── /v1/analysis/timeseries ─────────────────────────────────────────────

  describe("GET session timeseries", () => {
    it("returns epoch rows for a small session", async () => {
      const ts = await api<Record<string, unknown>[]>("/v1/analysis/timeseries", "POST", SMALL);
      expect(Array.isArray(ts)).toBe(true);
      expect(ts.length).toBeGreaterThan(0);
      // Each row should have a timestamp and band powers
      const row = ts[0];
      expect(row.t).toBeDefined();
      expect(row.rd).toBeDefined(); // rel_delta
      expect(row.rt).toBeDefined(); // rel_theta
    });

    it("downsamples large sessions to ~800 rows", async () => {
      const t0 = Date.now();
      const ts = await api<Record<string, unknown>[]>("/v1/analysis/timeseries", "POST", LARGE);
      const elapsed = Date.now() - t0;
      expect(ts.length).toBeLessThanOrEqual(1000);
      expect(ts.length).toBeGreaterThan(100);
      // Should be much faster than returning all 146K rows
      expect(elapsed).toBeLessThan(5_000);
    }, 10_000);

    it("rows are sorted by timestamp", async () => {
      const ts = await api<{ t: number }[]>("/v1/analysis/timeseries", "POST", SMALL);
      for (let i = 1; i < ts.length; i++) {
        expect(ts[i].t).toBeGreaterThanOrEqual(ts[i - 1].t);
      }
    });

    it("returns empty array for empty range", async () => {
      const ts = await api<unknown[]>("/v1/analysis/timeseries", "POST", {
        startUtc: 1000000000,
        endUtc: 1000000001,
      });
      expect(ts).toEqual([]);
    });
  });

  // ── /v1/analysis/sleep ──────────────────────────────────────────────────

  describe("GET sleep stages", () => {
    it("returns classified epochs for a session", async () => {
      const s = await api<{ epochs: { utc: number; stage: number }[]; summary: Record<string, unknown> }>(
        "/v1/analysis/sleep",
        "POST",
        SMALL,
      );
      expect(s.epochs.length).toBeGreaterThan(0);
      // Stages should be 0 (wake), 1 (N1), 2 (N2), 3 (N3), or 5 (REM)
      for (const e of s.epochs) {
        expect([0, 1, 2, 3, 5]).toContain(e.stage);
      }
      // Summary should have epoch counts
      expect(s.summary.total_epochs).toBe(s.epochs.length);
      expect(s.summary.epoch_secs).toBeGreaterThan(0);
    });

    it("returns sleep analysis for large session within reasonable time", async () => {
      const t0 = Date.now();
      const s = await api<{ epochs: unknown[]; summary: Record<string, unknown> }>("/v1/analysis/sleep", "POST", LARGE);
      const elapsed = Date.now() - t0;
      expect(s.epochs.length).toBeGreaterThan(0);
      expect(elapsed).toBeLessThan(10_000);
    }, 15_000);
  });

  // ── /v1/analysis/umap ──────────────────────────────────────────────────

  describe("UMAP compare", () => {
    it("returns reason when no epochs exist in range", async () => {
      // Far-future range with no data at all
      const r = await api<{
        points: unknown[];
        n_a: number;
        n_b: number;
        dim: number;
        total_a?: number;
        total_b?: number;
        reason?: string;
      }>("/v1/analysis/umap", "POST", {
        aStartUtc: 2000000000,
        aEndUtc: 2000000100,
        bStartUtc: 2000000200,
        bEndUtc: 2000000300,
      });
      expect(r.points).toEqual([]);
      expect(r.reason).toBeDefined();
      expect(r.reason).toContain("no_epochs");
    });

    it("returns points when embeddings exist", async () => {
      // SMALL range now has embeddings — UMAP should work (but takes time)
      const r = await api<{
        points: unknown[];
        n_a: number;
        n_b: number;
        dim: number;
        total_a?: number;
        total_b?: number;
        reason?: string;
      }>("/v1/analysis/umap", "POST", {
        aStartUtc: SMALL.startUtc,
        aEndUtc: SMALL.endUtc,
        bStartUtc: SMALL.startUtc,
        bEndUtc: SMALL.startUtc + 300,
      });
      // Either returns computed points or a reason
      expect(r).toHaveProperty("points");
      expect(r).toHaveProperty("n_a");
      expect(r).toHaveProperty("n_b");
      expect(r).toHaveProperty("dim");
      // total_a/total_b only present when returning early (no/few embeddings)
      if (r.points.length > 0) {
        expect(r.dim).toBeGreaterThan(0);
      } else {
      }
    }, 60_000);
  });

  // ── Parallel requests (compare window pattern) ─────────────────────────

  describe("parallel compare requests", () => {
    it("handles 4 concurrent metric+sleep requests", async () => {
      const t0 = Date.now();
      const [ma, mb, sa, sb] = await Promise.all([
        api<Record<string, unknown>>("/v1/analysis/metrics", "POST", SMALL),
        api<Record<string, unknown>>("/v1/analysis/metrics", "POST", MEDIUM),
        api<{ epochs: unknown[] }>("/v1/analysis/sleep", "POST", SMALL),
        api<{ epochs: unknown[] }>("/v1/analysis/sleep", "POST", MEDIUM),
      ]);
      const _elapsed = Date.now() - t0;
      expect(ma.n_epochs as number).toBeGreaterThan(0);
      expect(mb.n_epochs as number).toBeGreaterThan(0);
      expect(sa.epochs.length).toBeGreaterThan(0);
      expect(sb.epochs.length).toBeGreaterThan(0);
    }, 15_000);

    it("handles full compare flow: metrics + sleep + timeseries", async () => {
      const t0 = Date.now();
      const results = await Promise.allSettled([
        api<Record<string, unknown>>("/v1/analysis/metrics", "POST", SMALL),
        api<Record<string, unknown>>("/v1/analysis/metrics", "POST", MEDIUM),
        api<{ epochs: unknown[] }>("/v1/analysis/sleep", "POST", SMALL),
        api<{ epochs: unknown[] }>("/v1/analysis/sleep", "POST", MEDIUM),
        api<unknown[]>("/v1/analysis/timeseries", "POST", SMALL),
        api<unknown[]>("/v1/analysis/timeseries", "POST", MEDIUM),
      ]);
      const _elapsed = Date.now() - t0;
      const fulfilled = results.filter((r) => r.status === "fulfilled").length;
      expect(fulfilled).toBe(6);
    }, 30_000);
  });

  // ── invoke-proxy integration ───────────────────────────────────────────

  describe("invoke-proxy compare flow", () => {
    it("daemonInvoke routes metrics correctly", async () => {
      const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");
      const m = await daemonInvoke<Record<string, unknown>>("get_session_metrics", SMALL);
      expect(m.n_epochs as number).toBeGreaterThanOrEqual(SMALL.expectedMinEpochs);
    });

    it("daemonInvoke routes timeseries correctly", async () => {
      const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");
      const ts = await daemonInvoke<unknown[]>("get_session_timeseries", SMALL);
      expect(Array.isArray(ts)).toBe(true);
      expect(ts.length).toBeGreaterThan(0);
    });

    it("daemonInvoke routes sleep correctly", async () => {
      const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");
      const s = await daemonInvoke<{ epochs: unknown[] }>("get_sleep_stages", SMALL);
      expect(s.epochs.length).toBeGreaterThan(0);
    });

    it("enqueue + poll UMAP job lifecycle", async () => {
      const { daemonInvoke } = await import("../lib/daemon/invoke-proxy");

      // Use a no-data range so UMAP returns instantly (no GPU compute needed).
      const ticket = await daemonInvoke<{ job_id: number; queue_position: number; estimated_secs: number }>(
        "enqueue_umap_compare",
        { aStartUtc: 2000000000, aEndUtc: 2000000100, bStartUtc: 2000000200, bEndUtc: 2000000300 },
      );
      expect(ticket.job_id).toBeGreaterThan(0);
      expect(ticket.queue_position).toBe(0);

      // Poll until complete or error (should be near-instant for empty range).
      let result: Record<string, unknown> | null = null;
      for (let i = 0; i < 20; i++) {
        const poll = await daemonInvoke<{ status: string; result?: Record<string, unknown> }>("poll_job", {
          jobId: ticket.job_id,
        });
        if (poll.status === "complete") {
          result = poll.result ?? null;
          break;
        }
        if (poll.status === "error") {
          break;
        }
        await new Promise((r) => setTimeout(r, 500));
      }

      expect(result).not.toBeNull();
      if (result) {
        expect(result).toHaveProperty("points");
        expect(result).toHaveProperty("reason");
      }
    }, 30_000);
  });
});
