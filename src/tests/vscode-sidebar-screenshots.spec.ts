import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { expect, test } from "@playwright/test";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const previewDir = resolve(__dirname, "../../extensions/vscode/media/preview");
const outDir = resolve(__dirname, "../../extensions/vscode/media/screenshots");

type Theme = "dark" | "light";

test.use({
  viewport: { width: 320, height: 1100 },
  deviceScaleFactor: 2,
});

test.describe.configure({ mode: "serial" });

const sidebarStates: Array<{ name: string; html: string; height?: number }> = [
  { name: "sidebar-connected", html: "sidebar.html", height: 1120 },
  { name: "sidebar-disconnected", html: "sidebar-disconnected.html", height: 320 },
  { name: "sidebar-fatigued", html: "sidebar-fatigued.html", height: 720 },
  { name: "sidebar-stuck", html: "sidebar-stuck.html", height: 720 },
  { name: "sidebar-low-focus", html: "sidebar-low-focus.html", height: 640 },
];

const themes: Theme[] = ["dark", "light"];

for (const { name, html, height } of sidebarStates) {
  for (const theme of themes) {
    test(`${name} — ${theme}`, async ({ page }) => {
      if (height) await page.setViewportSize({ width: 320, height });
      await page.emulateMedia({ colorScheme: theme });
      await page.goto(pathToFileURL(`${previewDir}/${html}`).href);
      if (theme === "light") {
        await page.evaluate(() => document.documentElement.classList.add("light"));
      }
      await expect(page.locator(".container").first()).toBeVisible();
      await page.screenshot({
        path: `${outDir}/${name}-${theme}.png`,
        fullPage: true,
        omitBackground: false,
      });
    });
  }
}

for (const theme of themes) {
  test(`statusbar — ${theme}`, async ({ page }) => {
    await page.setViewportSize({ width: 1100, height: 130 });
    await page.emulateMedia({ colorScheme: theme });
    await page.goto(pathToFileURL(`${previewDir}/statusbar.html`).href);
    if (theme === "light") {
      await page.evaluate(() => document.body.classList.add("light"));
    }
    await expect(page.locator(".statusbar").first()).toBeVisible();
    await page.screenshot({
      path: `${outDir}/statusbar-${theme}.png`,
      fullPage: true,
      omitBackground: false,
    });
  });
}
