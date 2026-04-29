import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const __dirname = dirname(fileURLToPath(import.meta.url));
const extRoot = resolve(__dirname, "../../extensions/vscode");

const FDA_REQUIRED_TOKENS = ["FDA", "CE"] as const;

// `extensions/vscode` is a git submodule. Skip when not checked out
// (CI without `submodules: true`, or fresh `git clone` without `--recursive`).
const submoduleAvailable = existsSync(`${extRoot}/src/sidebar.ts`);

describe.skipIf(!submoduleAvailable)("vscode extension — research-use disclaimer", () => {
  it("sidebar.ts defines _disclaimerFooter and calls it from every HTML producer", () => {
    const src = readFileSync(`${extRoot}/src/sidebar.ts`, "utf-8");

    expect(src).toMatch(/_disclaimerFooter\s*\(\s*\)\s*:\s*string/);

    const definitionMatches = src.match(/_disclaimerFooter\s*\(\s*\)\s*:/g) ?? [];
    const callMatches = src.match(/this\._disclaimerFooter\s*\(\s*\)/g) ?? [];

    expect(definitionMatches.length).toBe(1);
    // Loading + disconnected + connected — three render paths must call it.
    expect(callMatches.length).toBeGreaterThanOrEqual(3);
  });

  it("uses the localised helper for the disclaimer text — never hardcoded English", () => {
    const src = readFileSync(`${extRoot}/src/sidebar.ts`, "utf-8");
    const footerSrc = src.match(/_disclaimerFooter\s*\(\s*\)[^{]*\{[\s\S]*?\n\s\s\}/)?.[0] ?? "";
    // Either the bespoke loader (`tr("sidebar.disclaimer")`) or the
    // built-in (`vscode.l10n.t("sidebar.disclaimer")`) is acceptable —
    // both look up the same key.
    const usesLocaliser =
      /\btr\(\s*"sidebar\.disclaimer"/.test(footerSrc) || /\bvscode\.l10n\.t\(\s*"sidebar\.disclaimer"/.test(footerSrc);
    expect(usesLocaliser, "footer must call tr() or vscode.l10n.t() with the disclaimer key").toBe(true);
    // Negative: no hardcoded "Research tool" inside the footer body.
    expect(footerSrc).not.toMatch(/Research tool only/);
  });

  it("every l10n bundle defines sidebar.disclaimer with the FDA + CE tokens", () => {
    const bundleDir = `${extRoot}/l10n`;
    const bundles = readdirSync(bundleDir).filter((f) => f.startsWith("bundle.l10n.") && f.endsWith(".json"));
    expect(bundles.length).toBeGreaterThanOrEqual(9);

    for (const file of bundles) {
      const json = JSON.parse(readFileSync(`${bundleDir}/${file}`, "utf-8")) as Record<string, string>;
      const value = json["sidebar.disclaimer"];
      expect(value, `${file} is missing sidebar.disclaimer`).toBeTruthy();

      for (const token of FDA_REQUIRED_TOKENS) {
        expect(value, `${file} sidebar.disclaimer must mention "${token}"`).toContain(token);
      }
    }
  });

  it("every sidebar preview mock contains the disclaimer block", () => {
    const previewDir = `${extRoot}/media/preview`;
    const mocks = readdirSync(previewDir).filter((f) => f.startsWith("sidebar") && f.endsWith(".html"));
    expect(mocks.length).toBeGreaterThanOrEqual(5);

    for (const file of mocks) {
      const html = readFileSync(`${previewDir}/${file}`, "utf-8");
      expect(html, `${file} missing class="disclaimer"`).toContain('class="disclaimer"');
      expect(html, `${file} missing FDA/CE in disclaimer`).toMatch(/FDA, CE/);
    }
  });

  it("README opens with a research-use disclaimer block", () => {
    const readme = readFileSync(`${extRoot}/README.md`, "utf-8");
    const head = readme.slice(0, 2000);
    expect(head).toMatch(/Research tool only/i);
    expect(head).toMatch(/not a medical device/i);
    expect(head).toMatch(/FDA/);
    expect(head).toMatch(/CE/);
  });

  it("every sidebar state ships in both dark and light variants", () => {
    const screensDir = `${extRoot}/media/screenshots`;
    const files = readdirSync(screensDir).filter((f) => f.endsWith(".png"));
    const states = [
      "sidebar-connected",
      "sidebar-disconnected",
      "sidebar-fatigued",
      "sidebar-stuck",
      "sidebar-low-focus",
      "statusbar",
    ] as const;

    for (const state of states) {
      expect(files, `missing ${state}-dark.png`).toContain(`${state}-dark.png`);
      expect(files, `missing ${state}-light.png`).toContain(`${state}-light.png`);
    }
  });

  it("source README keeps relative srcset paths so it works on GitHub and against bundled images", () => {
    const readme = readFileSync(`${extRoot}/README.md`, "utf-8");
    const sourceTags = readme.match(/<source[^>]+>/g) ?? [];
    expect(sourceTags.length).toBeGreaterThanOrEqual(12); // 6 states × 2 themes

    for (const tag of sourceTags) {
      // Source README uses relative paths; absolute URLs are baked in
      // by scripts/build-marketplace-readme.mjs at package time.
      expect(tag, `expected relative srcset in source <source>: ${tag}`).toMatch(/srcset="media\//);
    }
  });

  it("build-marketplace-readme.mjs rewrites every relative media path to an absolute URL", async () => {
    const { execSync } = await import("node:child_process");
    execSync("node scripts/build-marketplace-readme.mjs", { cwd: extRoot, stdio: "pipe" });
    const generated = readFileSync(`${extRoot}/.marketplace.readme.md`, "utf-8");

    const sourceTags = generated.match(/<source[^>]+>/g) ?? [];
    expect(sourceTags.length).toBeGreaterThanOrEqual(12);
    for (const tag of sourceTags) {
      expect(tag, `unrewritten <source>: ${tag}`).toMatch(/srcset="https:\/\/github\.com\//);
    }
    const imgTags = generated.match(/<img\s[^>]*src="media\/[^"]+"/g) ?? [];
    expect(imgTags, "found unrewritten <img> tags").toEqual([]);

    // Cleanup so the file isn't accidentally committed.
    execSync("rm -f .marketplace.readme.md", { cwd: extRoot });
  });
});
