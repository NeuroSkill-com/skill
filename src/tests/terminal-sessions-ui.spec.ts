/**
 * E2E test for the Terminal Sessions card in the Activity tab.
 *
 * Talks to the *real* running daemon at 127.0.0.1:18444 (the test runner
 * does NOT spin up a separate daemon — start one before running this).
 * Mocks only the Tauri-IPC bootstrap so the Svelte app can resolve the
 * daemon URL and auth token. Then drives the UI and inspects what it
 * actually renders for the data currently in the DB.
 *
 * Run: npx playwright test src/tests/terminal-sessions-ui.spec.ts --reporter=list
 */

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { expect, test } from "@playwright/test";

// Read the running daemon's auth token from the standard location so the
// in-page fetch can authenticate.
function readToken(): string {
  const p =
    process.platform === "darwin"
      ? path.join(os.homedir(), "Library", "Application Support", "skill", "daemon", "auth.token")
      : path.join(os.homedir(), ".config", "skill", "daemon", "auth.token");
  return fs.readFileSync(p, "utf8").trim();
}

const TOKEN = readToken();
const PORT = 18444;

function bootstrapMockScript() {
  return `
    window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
    window.__TAURI_INTERNALS__.metadata = {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
      windows: [{ label: "main" }],
      webviews: [{ label: "main", windowLabel: "main" }],
    };
    window.__TAURI_INTERNALS__.invoke = function(cmd, args) {
      if (cmd === "get_daemon_bootstrap") {
        return Promise.resolve({
          port: ${PORT},
          token: ${JSON.stringify(TOKEN)},
          compatible_protocol: true,
          daemon_version: "0.1.0",
          protocol_version: 1,
        });
      }
      // Everything else: no-op so the page doesn't crash on missing commands.
      return Promise.resolve(null);
    };
    window.__TAURI_INTERNALS__.transformCallback = function(cb) {
      const id = Math.random().toString(36).slice(2);
      window["_" + id] = cb || (() => {});
      return id;
    };
  `;
}

test.describe("TerminalSessionsCard", () => {
  test("renders sessions returned by the live daemon and shows session-text for backfilled rows", async ({ page }) => {
    // Inject Tauri bootstrap before any module loads. We don't intercept
    // /v1/* — those go to the real daemon.
    await page.addInitScript(bootstrapMockScript());

    const consoleErrors: string[] = [];
    page.on("pageerror", (err) => consoleErrors.push(`pageerror: ${err.message}`));
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(`console.error: ${msg.text()}`);
    });

    await page.goto("/settings");
    await page.waitForLoadState("networkidle");

    // Scroll the (long) Activity page until our card scrolls into the viewport.
    // The "Terminal" group header is the predecessor to TerminalSessionsCard.
    const groupHeader = page.getByRole("heading", { name: "Terminal", level: 2 });
    await groupHeader.scrollIntoViewIfNeeded({ timeout: 10_000 });

    // Find the description paragraph, then walk up to its parent card.
    const descLine = page.getByText("Each shell from launch to exit", { exact: false }).first();
    await expect(descLine).toBeVisible({ timeout: 10_000 });
    // The enclosing <section> wraps both the description and all session rows.
    const card = descLine.locator("xpath=ancestor::section[1]").first();
    await expect(card).toBeVisible({ timeout: 5_000 });

    // Capture API truth so we can compare to what's rendered.
    const apiResp = await fetch(`http://127.0.0.1:${PORT}/v1/brain/terminal-sessions`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Authorization: `Bearer ${TOKEN}` },
      body: JSON.stringify({ limit: 30 }),
    });
    const apiData = (await apiResp.json()) as {
      sessions: Array<{
        id: string;
        started_at: number;
        ended_at: number | null;
        shell: string;
        initial_cwd: string;
        command_count: number;
        avg_focus: number | null;
        avg_mood: number | null;
        has_session_text?: boolean;
      }>;
      orphan_command_count?: number;
    };

    console.log(`[api] sessions returned: ${apiData.sessions.length}`);
    console.log(`[api] orphan commands:   ${apiData.orphan_command_count ?? "(missing)"}`);
    console.log(`[api] sessions with text: ${apiData.sessions.filter((s) => s.has_session_text).length}`);
    console.log(`[api] sessions with commands>0: ${apiData.sessions.filter((s) => s.command_count > 0).length}`);

    // Wait for at least one session row to render.
    await card
      .locator("[role='button']")
      .first()
      .waitFor({ state: "visible", timeout: 10_000 })
      .catch(() => undefined);

    const cardText = await card.innerText();
    console.log("=== rendered card text (first 1500 chars) ===");
    console.log(cardText.slice(0, 1500));

    // Hard assertions on the new structural improvements:

    // 1. Scope summary line is present (sessions count, linked commands,
    //    legacy/text-only count, and orphan count).
    expect(cardText).toMatch(/\d+ sessions?\b/);
    expect(cardText).toMatch(/\d+ commands? linked\b/);

    // 2. Orphan count surfaces if the daemon reported any.
    if ((apiData.orphan_command_count ?? 0) > 0) {
      expect(cardText).toContain(`${apiData.orphan_command_count} untracked command`);
    }

    // 3. Backfilled sessions get a "legacy" badge and "session text only"
    //    descriptor instead of the misleading "0 cmds".
    const backfilledCount = apiData.sessions.filter((s) => s.has_session_text).length;
    if (backfilledCount > 0) {
      // At least one row should show the new descriptor or legacy badge.
      const hasLegacyMarker = cardText.includes("legacy") || cardText.includes("session text only");
      expect(hasLegacyMarker).toBe(true);
    }

    // 4. Click a backfilled session and verify its text renders.
    const backfilled = apiData.sessions.find((s) => s.has_session_text);
    if (backfilled) {
      const row = card.locator("[role='button']", { hasText: /legacy/i }).first();
      if ((await row.count()) > 0) {
        await row.click();
        // The expanded panel labels the content as stripped session text.
        await page
          .getByText(/Stripped session text/i)
          .first()
          .waitFor({ state: "visible", timeout: 5_000 });
        const expandedText = await card.innerText();
        console.log("=== expanded backfilled session (first 1000 chars) ===");
        console.log(expandedText.slice(0, 1000));
        expect(expandedText).toContain("Stripped session text");
        // Collapse so it doesn't pollute the next assertion block.
        await row.click();
      }
    }

    // 5. Day buckets render with at least one bucket header.
    const buckets = card.locator("[data-testid='day-bucket']");
    const bucketCount = await buckets.count();
    console.log(`[ui] day buckets visible: ${bucketCount}`);
    expect(bucketCount).toBeGreaterThan(0);
    // Header is uppercased via CSS — case-insensitive match.
    const firstBucketText = await buckets.first().innerText();
    expect(firstBucketText).toMatch(/(today|yesterday|mon|tue|wed|thu|fri|sat|sun)/i);

    // 6. Filter chips: clicking "Live" should reduce visible rows to live ones.
    const liveCount = apiData.sessions.filter((s) => s.ended_at == null).length;
    const liveChip = card.locator("[data-filter='live']").first();
    if ((await liveChip.count()) > 0) {
      await liveChip.click();
      await page.waitForTimeout(150); // svelte derives reflow
      const visibleRows = await card.locator("[data-testid='session-row']").count();
      console.log(`[ui] rows visible after Live filter: ${visibleRows} (expected ${liveCount})`);
      expect(visibleRows).toBe(liveCount);
      // Reset to All for subsequent assertions.
      await card.locator("[data-filter='all']").first().click();
      await page.waitForTimeout(150);
    }

    // 7. F/M columns conditional rendering: if no row in the visible filter
    //    has avg_focus or avg_mood, the columns shouldn't be in the DOM.
    const anyEeg = apiData.sessions.some((s) => s.avg_focus != null || s.avg_mood != null);
    const eegCols = await card.locator("[data-testid='eeg-cols']").count();
    console.log(`[ui] anyEegInData=${anyEeg}, F/M-column elements rendered=${eegCols}`);
    if (!anyEeg) {
      expect(eegCols).toBe(0);
    } else {
      expect(eegCols).toBeGreaterThan(0);
    }

    // 8. Day-bucket collapse toggle works.
    const firstBucketHeader = buckets.first().locator("button").first();
    const beforeRows = await buckets.first().locator("[data-testid='session-row']").count();
    await firstBucketHeader.click();
    await page.waitForTimeout(150);
    const afterRows = await buckets.first().locator("[data-testid='session-row']").count();
    console.log(`[ui] day collapse: ${beforeRows} → ${afterRows}`);
    expect(afterRows).toBe(0);
    // Re-expand for screenshot.
    await firstBucketHeader.click();

    await page.screenshot({ path: "test-results/terminal-sessions-card.png", fullPage: false });
    expect(consoleErrors).toEqual([]);
  });
});
