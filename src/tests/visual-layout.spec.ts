// SPDX-License-Identifier: GPL-3.0-only
//
// Visual regression suite — iterates routes × viewport sizes × font scales,
// captures screenshots, and runs in-page DOM checks for layout bugs:
//
//   • Horizontal overflow (element scrollWidth > clientWidth)
//   • Tiny text rendering (computed font-size < 9px → unreadable)
//   • Overlapping sibling text nodes (bounding-box intersection)
//   • Off-viewport fixed/absolute elements
//
// Findings are aggregated into `test-results/visual-layout/report.json`
// and screenshots saved at `test-results/visual-layout/<route>/<vp>x<scale>.png`.
// The suite is non-fatal on issue counts — it surfaces problems rather
// than asserting a specific budget. Read the JSON to triage.

import * as fs from "node:fs";
import * as path from "node:path";
import { expect, type Page, test } from "@playwright/test";

// ── Configuration ────────────────────────────────────────────────────────────

const ROUTES = [
  "/",
  "/about",
  "/api",
  "/calibration",
  "/chat",
  "/compare",
  "/downloads",
  "/focus-timer",
  "/help",
  "/history",
  "/label",
  "/labels",
  "/onboarding",
  "/search",
  "/session",
  "/settings",
  "/virtual-devices",
  "/whats-new",
];

const VIEWPORTS = [
  { name: "mobile", width: 375, height: 667 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "laptop", width: 1280, height: 800 },
  { name: "desktop", width: 1440, height: 900 },
  { name: "wide", width: 1920, height: 1080 },
];

// Font scales applied as html { font-size: <px> } — propagates through
// any rem-based styling (Tailwind `text-*` classes use rem). Anything
// hardcoded in px stays the same, which itself is a finding worth
// surfacing (those won't respect OS accessibility settings).
const FONT_SCALES = [
  { label: "100", htmlFontPx: 16 },
  { label: "125", htmlFontPx: 20 },
  { label: "150", htmlFontPx: 24 },
];

const OUT_DIR = path.join(process.cwd(), "test-results", "visual-layout");

// ── DOM-side checks (run inside the page) ────────────────────────────────────

interface LayoutIssue {
  kind: "overflow_x" | "tiny_text" | "overlap" | "offscreen_fixed";
  selector: string;
  text?: string;
  detail?: string;
}

/**
 * Inject and run layout checks. Runs entirely in-page so it picks up
 * actual computed styles and bounding boxes.
 */
async function detectIssues(page: Page): Promise<LayoutIssue[]> {
  return page.evaluate(() => {
    const issues: {
      kind: "overflow_x" | "tiny_text" | "overlap" | "offscreen_fixed";
      selector: string;
      text?: string;
      detail?: string;
    }[] = [];
    const MAX_ISSUES = 60; // cap per page so the report stays useful

    function shortSelector(el: Element): string {
      const tag = el.tagName.toLowerCase();
      const id = el.id ? `#${el.id}` : "";
      const classes = (el.className || "")
        .toString()
        .split(/\s+/)
        .filter(Boolean)
        .slice(0, 3)
        .map((c) => `.${c}`)
        .join("");
      return `${tag}${id}${classes}`.slice(0, 120);
    }

    function clip(s: string, n: number): string {
      const t = (s || "").trim().replace(/\s+/g, " ");
      return t.length > n ? `${t.slice(0, n)}…` : t;
    }

    // ── 1. Horizontal overflow ──
    // Catches elements whose content extends past their container — most
    // common cause of horizontal scrollbars and text running off-screen.
    {
      const root = document.documentElement;
      const all = document.body.querySelectorAll<HTMLElement>("*");
      // Scan a bounded sample to keep evaluation fast.
      const sample = all.length > 4000 ? Array.from(all).slice(0, 4000) : Array.from(all);
      for (const el of sample) {
        if (issues.length >= MAX_ISSUES) break;
        const cs = getComputedStyle(el);
        // Skip elements that are intentionally scrollable.
        if (cs.overflowX === "auto" || cs.overflowX === "scroll") continue;
        // Skip elements with hidden overflow — they'll clip silently,
        // which is sometimes a bug but is harder to triage automatically.
        if (cs.overflowX === "hidden") continue;
        if (el.scrollWidth - el.clientWidth > 1) {
          issues.push({
            kind: "overflow_x",
            selector: shortSelector(el),
            text: clip(el.textContent || "", 60),
            detail: `${el.scrollWidth}px content / ${el.clientWidth}px container`,
          });
        }
      }
      // Body-level horizontal scroll is the biggest red flag — surface
      // even when per-element scan misses it.
      if (root.scrollWidth - root.clientWidth > 1) {
        issues.push({
          kind: "overflow_x",
          selector: "html",
          detail: `page scrolls horizontally: ${root.scrollWidth}px / ${root.clientWidth}px`,
        });
      }
    }

    // ── 2. Tiny text (computed font-size < 9px) ──
    {
      const candidates = document.body.querySelectorAll<HTMLElement>(
        "p, span, div, button, a, label, h1, h2, h3, h4, h5, h6, li, td, th",
      );
      for (const el of candidates) {
        if (issues.length >= MAX_ISSUES) break;
        const text = (el.textContent || "").trim();
        if (!text) continue;
        const cs = getComputedStyle(el);
        const fs = Number.parseFloat(cs.fontSize);
        if (Number.isFinite(fs) && fs > 0 && fs < 9) {
          // Only flag if element is actually visible (not display:none).
          if (cs.display === "none" || cs.visibility === "hidden") continue;
          issues.push({
            kind: "tiny_text",
            selector: shortSelector(el),
            text: clip(text, 40),
            detail: `${fs.toFixed(1)}px`,
          });
        }
      }
    }

    // ── 3. Overlap among sibling text nodes ──
    // Two text-bearing siblings whose bounding boxes overlap usually
    // means a layout bug (e.g. a label running into a value at narrow
    // viewports). We only check direct siblings to keep it tractable.
    {
      const containers = document.body.querySelectorAll<HTMLElement>("*");
      for (const c of containers) {
        if (issues.length >= MAX_ISSUES) break;
        const kids = Array.from(c.children) as HTMLElement[];
        if (kids.length < 2) continue;
        // Skip absolutely-positioned containers — overlap is usually intentional.
        const cs = getComputedStyle(c);
        if (cs.position === "absolute" || cs.position === "relative") {
          // Only skip if any child is absolutely positioned (typical
          // overlay pattern). Heuristic.
          const anyAbs = kids.some((k) => {
            const ks = getComputedStyle(k);
            return ks.position === "absolute" || ks.position === "fixed";
          });
          if (anyAbs) continue;
        }
        const boxes = kids.map((k) => ({
          el: k,
          rect: k.getBoundingClientRect(),
          text: (k.textContent || "").trim(),
        }));
        for (let i = 0; i < boxes.length; i++) {
          for (let j = i + 1; j < boxes.length; j++) {
            const a = boxes[i];
            const b = boxes[j];
            if (!a.text || !b.text) continue;
            // Bounding-box intersection.
            const overlap =
              a.rect.left < b.rect.right &&
              a.rect.right > b.rect.left &&
              a.rect.top < b.rect.bottom &&
              a.rect.bottom > b.rect.top;
            if (overlap) {
              // Filter very small overlaps (pixel rounding).
              const ix = Math.min(a.rect.right, b.rect.right) - Math.max(a.rect.left, b.rect.left);
              const iy = Math.min(a.rect.bottom, b.rect.bottom) - Math.max(a.rect.top, b.rect.top);
              if (ix > 3 && iy > 3) {
                issues.push({
                  kind: "overlap",
                  selector: `${shortSelector(a.el)} ⨯ ${shortSelector(b.el)}`,
                  text: `${clip(a.text, 25)} | ${clip(b.text, 25)}`,
                  detail: `overlap ${ix.toFixed(0)}×${iy.toFixed(0)}px`,
                });
                if (issues.length >= MAX_ISSUES) break;
              }
            }
          }
          if (issues.length >= MAX_ISSUES) break;
        }
      }
    }

    // ── 4. Fixed/absolute elements off the viewport ──
    {
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const all = document.body.querySelectorAll<HTMLElement>("*");
      const sample = all.length > 2000 ? Array.from(all).slice(0, 2000) : Array.from(all);
      for (const el of sample) {
        if (issues.length >= MAX_ISSUES) break;
        const cs = getComputedStyle(el);
        if (cs.position !== "fixed" && cs.position !== "absolute") continue;
        if (cs.display === "none" || cs.visibility === "hidden") continue;
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) continue;
        // Element is entirely off-screen on the right or bottom.
        if (r.left >= vw || r.top >= vh || r.right <= 0 || r.bottom <= 0) {
          issues.push({
            kind: "offscreen_fixed",
            selector: shortSelector(el),
            text: clip(el.textContent || "", 40),
            detail: `rect ${r.left.toFixed(0)},${r.top.toFixed(0)} ${r.width.toFixed(0)}×${r.height.toFixed(0)} / vp ${vw}×${vh}`,
          });
        }
      }
    }

    return issues;
  });
}

// ── Test scaffolding ─────────────────────────────────────────────────────────

interface RouteResult {
  route: string;
  viewport: string;
  scale: string;
  issueCount: number;
  issues: LayoutIssue[];
  screenshot: string;
}

const allResults: RouteResult[] = [];

test.beforeAll(() => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
});

test.afterAll(() => {
  // Aggregate report.
  const summary = {
    total_combinations: allResults.length,
    issue_counts_by_kind: allResults
      .flatMap((r) => r.issues.map((i) => i.kind))
      .reduce<Record<string, number>>((acc, k) => {
        acc[k] = (acc[k] ?? 0) + 1;
        return acc;
      }, {}),
    worst_offenders: [...allResults]
      .sort((a, b) => b.issueCount - a.issueCount)
      .slice(0, 20)
      .map((r) => ({ route: r.route, viewport: r.viewport, scale: r.scale, issues: r.issueCount })),
    results: allResults,
  };
  fs.writeFileSync(path.join(OUT_DIR, "report.json"), JSON.stringify(summary, null, 2));

  // Compact human-readable summary.
  const lines: string[] = [];
  lines.push(`Visual layout audit — ${allResults.length} combinations`);
  for (const [k, v] of Object.entries(summary.issue_counts_by_kind)) {
    lines.push(`  ${k}: ${v}`);
  }
  lines.push("");
  lines.push("Top offenders:");
  for (const o of summary.worst_offenders) {
    if (o.issues === 0) break;
    lines.push(`  ${o.route} @ ${o.viewport} ${o.scale}% — ${o.issues}`);
  }
  fs.writeFileSync(path.join(OUT_DIR, "summary.txt"), lines.join("\n"));
});

for (const route of ROUTES) {
  for (const vp of VIEWPORTS) {
    for (const scale of FONT_SCALES) {
      const slug = route.replace(/^\//, "") || "root";
      const fileBase = `${vp.name}-${vp.width}x${vp.height}-${scale.label}`;
      const testName = `${route} @ ${vp.name} ${vp.width}x${vp.height} font ${scale.label}%`;

      test(testName, async ({ page }) => {
        await page.setViewportSize({ width: vp.width, height: vp.height });

        // Set the html font-size BEFORE navigation so initial layout
        // happens at the target scale (avoids re-layout flashes).
        await page.addInitScript((fontPx: number) => {
          window.addEventListener("DOMContentLoaded", () => {
            document.documentElement.style.fontSize = `${fontPx}px`;
          });
        }, scale.htmlFontPx);

        const resp = await page.goto(route, { waitUntil: "networkidle", timeout: 15000 }).catch(() => null);
        // Some routes may 404 in dev (e.g. dynamic pages without params).
        // Still capture so we can see the error state.
        if (!resp) {
          test.info().annotations.push({ type: "warn", description: `goto failed for ${route}` });
        }
        // Settle: give async data fetches a moment, but bound it.
        await page.waitForTimeout(800);

        const dirAbs = path.join(OUT_DIR, slug);
        fs.mkdirSync(dirAbs, { recursive: true });
        const shot = path.join(dirAbs, `${fileBase}.png`);
        await page.screenshot({ path: shot, fullPage: true });

        const issues = await detectIssues(page);
        allResults.push({
          route,
          viewport: `${vp.name} ${vp.width}x${vp.height}`,
          scale: scale.label,
          issueCount: issues.length,
          issues,
          screenshot: path.relative(process.cwd(), shot),
        });

        // Soft-fail threshold: > 30 issues on a single page is almost
        // certainly a real layout bug worth attention. Single-digit
        // counts are usually false positives or expected behaviour.
        expect(issues.length, `Layout issues on ${route} @ ${vp.name} ${scale.label}%`).toBeLessThan(30);
      });
    }
  }
}
