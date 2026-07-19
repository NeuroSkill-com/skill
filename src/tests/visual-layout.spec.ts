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

// This is a Tauri desktop app — primary targets are macOS/Windows/Linux
// at ≥ 1024px. We include 375px (iPhone SE 2nd gen) as the lower bound
// because the Tauri window can be resized that small on a desktop, but
// we don't test 320px — supporting iPhone-SE-1st-gen width would mean
// per-page redesigns that aren't justified for a desktop-first app.
const VIEWPORTS = [
  { name: "mobile", width: 375, height: 667 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "laptop", width: 1280, height: 800 },
  { name: "desktop", width: 1440, height: 900 },
  { name: "wide", width: 1920, height: 1080 },
  { name: "ultra", width: 2560, height: 1440 }, // 27" 1440p — common on dev workstations
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
  kind:
    | "overflow_x"
    | "tiny_text"
    | "overlap"
    | "offscreen_fixed"
    | "tiny_tap_target" // < 32×32 px — smaller than recommended HIG minimum
    | "edge_touch" // interactive element within 2px of viewport edge
    | "scroll_trap"; // sticky/fixed bar covers content that can't be scrolled past
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
      kind:
        | "overflow_x"
        | "tiny_text"
        | "overlap"
        | "offscreen_fixed"
        | "tiny_tap_target"
        | "edge_touch"
        | "scroll_trap";
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
        // 5px noise floor: animations (animate-ping, scale transitions,
        // pulse rings) frequently transient-overflow by 1–4px during
        // the keyframe loop. Real layout overflows are usually ≥ 10px.
        if (el.scrollWidth - el.clientWidth > 5) {
          // Skip decorative containers: empty text, no images, only
          // absolute-positioned children. These are usually animated
          // overlays (animate-ping pulses, gradient halos) where the
          // visual extent is intentional and doesn't affect layout.
          const text = (el.textContent || "").trim();
          const hasMedia = el.querySelector("img,svg,canvas,video") !== null;
          if (!text && !hasMedia) {
            const kids = Array.from(el.children) as HTMLElement[];
            const allOutOfFlow =
              kids.length > 0 &&
              kids.every((k) => {
                const ks = getComputedStyle(k);
                return ks.position === "absolute" || ks.position === "fixed";
              });
            if (allOutOfFlow) continue;
          }
          issues.push({
            kind: "overflow_x",
            selector: shortSelector(el),
            text: clip(text, 60),
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
            // Skip pairs where either side is intentionally floating
            // (fixed/absolute/sticky titlebars, modal backdrops, popovers).
            const aPos = getComputedStyle(a.el).position;
            const bPos = getComputedStyle(b.el).position;
            if (aPos === "fixed" || aPos === "absolute" || aPos === "sticky") continue;
            if (bPos === "fixed" || bPos === "absolute" || bPos === "sticky") continue;
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
        // Skip a11y patterns deliberately positioned off-screen until
        // focused or read by an assistive tool.
        if (el.classList.contains("skip-link") || el.classList.contains("sr-only")) continue;
        if (el.id === "a11y-announcer") continue;
        // Skip 1×1 visually-hidden announcers (common pattern: clip:rect)
        const r = el.getBoundingClientRect();
        if (r.width <= 1 && r.height <= 1) continue;
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

    // ── 5. Tap targets smaller than recommended minimum ──
    // Desktop-first app, so 24px floor (a typical icon button) rather
    // than the iOS HIG 44pt or Material 48dp. Anything smaller than a
    // typical menu icon is hard to hit even with a mouse.
    {
      const interactive = document.body.querySelectorAll<HTMLElement>(
        'button, a, [role="button"], [role="link"], input:not([type="hidden"]), select, summary',
      );
      const MIN_TAP_PX = 24;
      for (const el of interactive) {
        if (issues.length >= MAX_ISSUES) break;
        const cs = getComputedStyle(el);
        if (cs.display === "none" || cs.visibility === "hidden") continue;
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) continue;
        // ── False-positive filters specific to this codebase ──
        // Window-control buttons in the Tauri titlebar (close/min/max)
        // are intentionally 30×30, sized to match macOS HIG.
        const cls = el.className?.toString() || "";
        if (cls.includes("svelte-sm4m8n")) continue; // titlebar.svelte module
        // Resize handles (col-resize / row-resize) are intentionally
        // narrow — the cursor area extends well beyond the visible bar.
        if (cls.includes("cursor-col-resize") || cls.includes("cursor-row-resize")) continue;
        // Inline links inside text aren't point targets — text-flow
        // wraps them. Only flag links that LOOK like buttons.
        if (el.tagName === "A") {
          const parent = el.parentElement;
          if (parent && /^(P|SPAN|LI|TD|ARTICLE|SECTION)$/.test(parent.tagName)) continue;
        }
        // Native checkboxes / radios are tiny (14×14) by browser default,
        // but the click area normally extends to the associated <label>
        // (the spec says label-clicks toggle the input). Look both for
        // a wrapping label and a `<label for="…">` association.
        if (el.tagName === "INPUT") {
          const t = (el as HTMLInputElement).type;
          if (t === "checkbox" || t === "radio") {
            const wrap = el.closest("label");
            const forLabel = el.id ? document.querySelector<HTMLLabelElement>(`label[for="${el.id}"]`) : null;
            if (wrap || forLabel) {
              // Either form is good a11y — the whole label is clickable
              // and that's the effective tap target.
              continue;
            }
          }
        }
        // Full-row buttons (w-full / 100%) where one dimension is large
        // — e.g. sidebar items 803×20 — have plenty of click area even
        // when the height looks short. Skip if total area ≥ 24² = 576px².
        const area = r.width * r.height;
        if (area >= MIN_TAP_PX * MIN_TAP_PX) continue;
        if (r.width < MIN_TAP_PX || r.height < MIN_TAP_PX) {
          issues.push({
            kind: "tiny_tap_target",
            selector: shortSelector(el),
            text: clip(el.textContent || "", 30),
            detail: `${r.width.toFixed(0)}×${r.height.toFixed(0)}px (min ${MIN_TAP_PX}px)`,
          });
        }
      }
    }

    // ── 6. Interactive elements touching the viewport edge ──
    // Buttons/inputs flush against (or beyond) the viewport edge mean
    // padding broke at this size. 2px tolerance accounts for borders.
    {
      const vw = window.innerWidth;
      const vh = window.innerHeight;
      const interactive = document.body.querySelectorAll<HTMLElement>(
        'button, a, [role="button"], input:not([type="hidden"]), select',
      );
      const TOL = 2;
      for (const el of interactive) {
        if (issues.length >= MAX_ISSUES) break;
        const cs = getComputedStyle(el);
        if (cs.display === "none" || cs.visibility === "hidden") continue;
        // Skip floating elements (toolbars, modals) — those frequently
        // hug the edge by design.
        if (cs.position === "fixed" || cs.position === "sticky") continue;
        // Skip elements that are children of a fixed/sticky ancestor
        // (those are intentionally pinned).
        let p: HTMLElement | null = el.parentElement;
        let pinned = false;
        for (let depth = 0; p && depth < 5; depth++, p = p.parentElement) {
          const ps = getComputedStyle(p);
          if (ps.position === "fixed" || ps.position === "sticky") {
            pinned = true;
            break;
          }
        }
        if (pinned) continue;
        // Skip resize handles (intentionally flush against the column
        // they resize) and full-width sidebar rows (w-full extends to
        // edge by design).
        const cls = el.className?.toString() || "";
        if (cls.includes("cursor-col-resize") || cls.includes("cursor-row-resize")) continue;
        if (cls.includes("w-full")) continue;
        const r = el.getBoundingClientRect();
        if (r.width === 0 || r.height === 0) continue;
        // Only consider elements actually IN the viewport.
        if (r.right <= 0 || r.bottom <= 0 || r.left >= vw || r.top >= vh) continue;
        // Vertical edges are too noisy: top is usually a header pinned
        // to the top by an ancestor flex layout (false positive), and
        // bottom is the natural fold of any internally-scrollable
        // container (false positive). Only flag horizontal-edge touches,
        // which always indicate a real responsive layout bug.
        const onLeft = r.left < TOL;
        const onRight = r.right > vw - TOL;
        if (onLeft || onRight) {
          const sides = [onLeft && "left", onRight && "right"].filter(Boolean).join("+");
          issues.push({
            kind: "edge_touch",
            selector: shortSelector(el),
            text: clip(el.textContent || "", 30),
            detail: `${sides}; rect ${r.left.toFixed(0)},${r.top.toFixed(0)} ${r.width.toFixed(0)}×${r.height.toFixed(0)} / vp ${vw}×${vh}`,
          });
        }
      }
    }

    // ── 7. Scroll trap: sticky/fixed bar covers content that's not reachable ──
    // Fires when a sticky/fixed element is taller than 50% of viewport
    // height, leaving little space for content. Common when a header
    // fails to collapse on a small viewport.
    {
      const vh = window.innerHeight;
      const all = document.body.querySelectorAll<HTMLElement>("*");
      const sample = all.length > 1500 ? Array.from(all).slice(0, 1500) : Array.from(all);
      for (const el of sample) {
        if (issues.length >= MAX_ISSUES) break;
        const cs = getComputedStyle(el);
        if (cs.position !== "fixed" && cs.position !== "sticky") continue;
        if (cs.display === "none" || cs.visibility === "hidden") continue;
        const r = el.getBoundingClientRect();
        if (r.height < vh * 0.5) continue; // not big enough to be a trap
        // Skip full-viewport overlays (modals) — they're covers, not traps.
        if (r.height > vh * 0.95 && r.width > window.innerWidth * 0.95) continue;
        issues.push({
          kind: "scroll_trap",
          selector: shortSelector(el),
          text: clip(el.textContent || "", 30),
          detail: `${cs.position} bar ${r.width.toFixed(0)}×${r.height.toFixed(0)}px occupies ${((r.height / vh) * 100).toFixed(0)}% of viewport`,
        });
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

// Per-test results are written to disk under `OUT_DIR/_findings/` so that
// every Playwright worker contributes (afterAll runs per-worker, so a
// shared in-memory array would only capture one worker's slice). After
// the suite finishes, run `node scripts/aggregate-visual-report.mjs`
// to merge into `report.json` + `summary.txt`.

const FINDINGS_DIR = path.join(OUT_DIR, "_findings");

test.beforeAll(() => {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  fs.mkdirSync(FINDINGS_DIR, { recursive: true });
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
        const result: RouteResult = {
          route,
          viewport: `${vp.name} ${vp.width}x${vp.height}`,
          scale: scale.label,
          issueCount: issues.length,
          issues,
          screenshot: path.relative(process.cwd(), shot),
        };
        // Worker-safe: each test writes its own JSON. Aggregator merges them.
        const findingPath = path.join(FINDINGS_DIR, `${slug}_${fileBase}.json`);
        fs.writeFileSync(findingPath, JSON.stringify(result));

        // Soft-fail threshold: > 30 issues on a single page is almost
        // certainly a real layout bug worth attention. Single-digit
        // counts are usually false positives or expected behaviour.
        expect(issues.length, `Layout issues on ${route} @ ${vp.name} ${scale.label}%`).toBeLessThan(30);
      });
    }
  }
}
