import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./src/tests",
  testMatch: "**/*.spec.ts",
  timeout: 60000,
  retries: 0,
  use: {
    baseURL: "http://localhost:1420",
    trace: "on-first-retry",
    screenshot: "on",
  },
  outputDir: "test-results",
  webServer: {
    command: "npx vite dev --port 1420",
    port: 1420,
    reuseExistingServer: true,
    timeout: 30000,
  },
});
