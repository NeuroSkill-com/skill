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
import { expect, test } from "@playwright/test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

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
    const descLine = page
      .getByText("Each shell from launch to exit", { exact: false })
      .first();
    await expect(descLine).toBeVisible({ timeout: 10_000 });
    // The enclosing <section> wraps both the description and all session rows.
    const card = descLine.locator("xpath=ancestor::section[1]").first();
    await expect(card).toBeVisible({ timeout: 5_000 });

    // Capture API truth so we can compare to what's rendered.
    const apiResp = await fetch(
      `http://127.0.0.1:${PORT}/v1/brain/terminal-sessions`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${TOKEN}` },
        body: JSON.stringify({ limit: 30 }),
      },
    );
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
      const hasLegacyMarker =
        cardText.includes("legacy") || cardText.includes("session text only");
      expect(hasLegacyMarker).toBe(true);
    }

    // 4. Click a backfilled session and verify its text renders.
    const backfilled = apiData.sessions.find((s) => s.has_session_text);
    if (backfilled) {
      const row = card
        .locator("[role='button']", { hasText: /legacy/i })
        .first();
      if (await row.count() > 0) {
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
      }
    }

    await page.screenshot({ path: "test-results/terminal-sessions-card.png", fullPage: false });
    expect(consoleErrors).toEqual([]);
  });
});
