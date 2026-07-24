import { defineConfig } from "@playwright/test";

// Headed (visible browser): HEADED=1, PLAYWRIGHT_HEADED=1, or CLI `--headed`.
// Optional slowdown so UI steps are watchable: PLAYWRIGHT_SLOWMO_MS=250 (default 250 when headed).
const headed =
  process.env.HEADED === "1" ||
  process.env.PLAYWRIGHT_HEADED === "1" ||
  process.argv.includes("--headed");
const slowMo = Number(
  process.env.PLAYWRIGHT_SLOWMO_MS ?? (headed ? "250" : "0"),
);

export default defineConfig({
  testDir: "./src/tests",
  testMatch: "**/*.spec.ts",
  timeout: headed ? 120_000 : 60_000,
  retries: 0,
  use: {
    baseURL: "http://localhost:1420",
    headless: !headed,
    launchOptions: slowMo > 0 ? { slowMo } : undefined,
    trace: "on-first-retry",
    screenshot: "on",
    video: headed ? "on" : "off",
  },
  outputDir: "test-results",
  webServer: {
    command: "npx vite dev --port 1420",
    port: 1420,
    reuseExistingServer: true,
    timeout: 30000,
  },
});
