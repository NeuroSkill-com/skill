#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Static accessibility (a11y) audit for all Svelte components.
// Usage: node scripts/audit-a11y.js [--check]
//   --check  exit with code 1 if any errors found (for CI)

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const CHECK_MODE = process.argv.includes("--check");
const SRC_DIR = new URL("../src", import.meta.url).pathname;

// ── Collect all .svelte files ────────────────────────────────────────────────
function walk(dir) {
  const results = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (entry === "node_modules" || entry === ".svelte-kit") continue;
      results.push(...walk(full));
    } else if (entry.endsWith(".svelte")) {
      results.push(full);
    }
  }
  return results;
}

// ── Rule definitions ─────────────────────────────────────────────────────────
// Each rule: { id, severity, desc, check(content, file) => Finding[] }
// Finding: { line, col?, message }

const rules = [];

function addRule(id, severity, desc, check) {
  rules.push({ id, severity, desc, check });
}

// Helper: iterate over regex matches with line numbers.
// For HTML tag patterns, joins multi-line tags so attributes on the next line are captured.
function matchesWithLines(content, regex) {
  const results = [];
  const lines = content.split("\n");
  for (let i = 0; i < lines.length; i++) {
    // For tag-matching regexes, join continuation lines so multi-line tags are matched whole.
    let line = lines[i];
    if (/<(?:img|button|a|input|textarea|select|svg|video|audio|html|h[1-6])\b/.test(line) && !/>/.test(line)) {
      for (let j = i + 1; j < Math.min(i + 10, lines.length); j++) {
        line += " " + lines[j];
        if (/>/.test(lines[j])) break;
      }
    }
    let m;
    const re = new RegExp(regex.source, regex.flags.replace("g", "") + "g");
    while ((m = re.exec(line)) !== null) {
      results.push({ line: i + 1, match: m[0], groups: m, fullLine: line });
    }
  }
  return results;
}

// Helper: check if a line or nearby lines are inside a svelte-ignore block
function hasSvelteIgnore(content, lineNum) {
  const lines = content.split("\n");
  for (let i = Math.max(0, lineNum - 3); i < lineNum; i++) {
    if (lines[i] && lines[i].includes("svelte-ignore")) return true;
  }
  return false;
}

// Helper: check if line is inside <script> block
function isInScript(content, lineNum) {
  const lines = content.split("\n");
  let inScript = false;
  for (let i = 0; i < lineNum && i < lines.length; i++) {
    if (/<script/.test(lines[i])) inScript = true;
    if (/<\/script>/.test(lines[i])) inScript = false;
  }
  return inScript;
}

// Helper: check if line is inside <style> block
function isInStyle(content, lineNum) {
  const lines = content.split("\n");
  let inStyle = false;
  for (let i = 0; i < lineNum && i < lines.length; i++) {
    if (/<style/.test(lines[i])) inStyle = true;
    if (/<\/style>/.test(lines[i])) inStyle = false;
  }
  return inStyle;
}

function isInTemplate(content, lineNum) {
  return !isInScript(content, lineNum) && !isInStyle(content, lineNum);
}

// ── Rules ────────────────────────────────────────────────────────────────────

addRule("img-alt", "error", "Images must have alt text", (content) => {
  const findings = [];
  const lines = content.split("\n");
  for (const m of matchesWithLines(content, /<img\b/)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;
    // Gather the full tag across lines (Svelte expressions may contain > inside generics)
    let fullTag = "";
    for (let i = m.line - 1; i < Math.min(m.line + 10, lines.length); i++) {
      fullTag += " " + lines[i];
      // Check for /> or > that ends the img tag (heuristic: line ends with > or />)
      if (/\/>\s*$/.test(lines[i]) || (i > m.line - 1 && />\s*$/.test(lines[i]))) break;
    }
    if (!/\balt\s*=/.test(fullTag) && !/\balt\b/.test(fullTag)) {
      findings.push({ line: m.line, message: "<img> missing alt attribute" });
    }
  }
  return findings;
});

addRule("button-label", "error", "Buttons must have accessible labels", (content) => {
  const findings = [];
  const lines = content.split("\n");

  // Find button open tags and check for content or aria-label
  for (const m of matchesWithLines(content, /<button\b[^>]*>/)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;

    const tag = m.match;
    // Has aria-label or aria-labelledby
    if (/aria-label\s*=/.test(tag) || /aria-labelledby\s*=/.test(tag)) continue;
    // Has title
    if (/\btitle\s*=/.test(tag)) continue;

    // Check if the button has text content (look at same line + next few lines until </button>)
    let buttonContent = "";
    for (let i = m.line - 1; i < Math.min(m.line + 5, lines.length); i++) {
      buttonContent += lines[i] + "\n";
      if (lines[i].includes("</button>")) break;
    }

    // Remove the opening tag
    buttonContent = buttonContent.replace(/<button\b[^>]*>/, "");
    // Remove closing tag and everything after
    buttonContent = buttonContent.replace(/<\/button>[\s\S]*/, "");
    // Remove HTML tags to get text content
    const textContent = buttonContent.replace(/<[^>]*>/g, "").replace(/[{}\s]/g, "").trim();

    // If only special characters or empty, flag it
    if (!textContent || /^[↻←→↑↓×✕✖✗⨉☰…·•]+$/.test(textContent)) {
      // Check if inner elements have aria-label
      if (/aria-label\s*=/.test(buttonContent)) continue;
      // Check for sr-only span
      if (/sr-only/.test(buttonContent)) continue;
      findings.push({ line: m.line, message: "Button has no accessible label (add aria-label or visible text)" });
    }
  }
  return findings;
});

addRule("click-on-noninteractive", "warning", "Click handlers on non-interactive elements need role and keyboard support", (content) => {
  const findings = [];
  // Match onclick on div, span, li, section, etc.
  const nonInteractive = /(<(?:div|span|li|section|article|p|td|tr)\b[^>]*\bonclick\b[^>]*>)/;
  for (const m of matchesWithLines(content, nonInteractive)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;
    const tag = m.match;
    if (/\brole\s*=/.test(tag)) continue;
    if (/\btabindex\s*=/.test(tag)) continue;
    findings.push({ line: m.line, message: `Non-interactive element with onclick missing role and tabindex` });
  }
  return findings;
});

addRule("input-label", "error", "Form inputs must have associated labels", (content) => {
  const findings = [];
  const inputRe = /<(?:input|textarea|select)\b([^>]*)>/;
  const lines = content.split("\n");
  for (const m of matchesWithLines(content, inputRe)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;
    const attrs = m.groups[1] || "";

    // Skip hidden inputs, submit buttons, range inputs (typically have visible labels nearby)
    if (/type\s*=\s*["']hidden["']/.test(attrs)) continue;
    if (/type\s*=\s*["']submit["']/.test(attrs)) continue;
    if (/type\s*=\s*["']range["']/.test(attrs)) continue;

    // Has aria-label, aria-labelledby, or id (which could be linked via <label for>)
    if (/aria-label\s*=/.test(attrs)) continue;
    if (/aria-labelledby\s*=/.test(attrs)) continue;
    if (/\bid\s*=/.test(attrs)) continue; // assume label exists elsewhere
    if (/\btitle\s*=/.test(attrs)) continue;

    // Skip reusable components that spread rest props (callers provide aria-label)
    if (/\.\.\.\w*[Rr]est/.test(m.fullLine) || /\{\.\.\.\$\$restProps/.test(m.fullLine)) continue;

    // Check if input is wrapped in a <label> element (look up to 5 lines back)
    let wrappedInLabel = false;
    let labelDepth = 0;
    for (let i = m.line - 2; i >= Math.max(0, m.line - 8); i--) {
      if (/<\/label/.test(lines[i])) labelDepth++;
      if (/<label\b/.test(lines[i])) {
        if (labelDepth === 0) { wrappedInLabel = true; break; }
        labelDepth--;
      }
    }
    if (wrappedInLabel) continue;

    findings.push({ line: m.line, message: "Input missing label association (add id+label, aria-label, or aria-labelledby)" });
  }
  return findings;
});

addRule("link-text", "error", "Links must have descriptive text", (content) => {
  const findings = [];
  const lines = content.split("\n");
  for (const m of matchesWithLines(content, /<a\b[^>]*>/)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;
    const tag = m.match;
    if (/aria-label\s*=/.test(tag)) continue;
    if (/aria-labelledby\s*=/.test(tag)) continue;
    if (/\btitle\s*=/.test(tag)) continue;

    // Grab content until </a>
    let linkContent = "";
    for (let i = m.line - 1; i < Math.min(m.line + 3, lines.length); i++) {
      linkContent += lines[i] + "\n";
      if (lines[i].includes("</a>")) break;
    }
    linkContent = linkContent.replace(/<a\b[^>]*>/, "").replace(/<\/a>[\s\S]*/, "");
    const text = linkContent.replace(/<[^>]*>/g, "").replace(/[{}\s]/g, "").trim();

    if (!text) {
      // Check for sr-only or aria-label inside
      if (/sr-only/.test(linkContent) || /aria-label/.test(linkContent)) continue;
      findings.push({ line: m.line, message: "Link has no accessible text (add aria-label or visible text)" });
    }
  }
  return findings;
});

addRule("heading-hierarchy", "warning", "Heading levels should not skip", (content, file) => {
  const findings = [];
  const headings = [];
  for (const m of matchesWithLines(content, /<h([1-6])\b/)) {
    if (!isInTemplate(content, m.line)) continue;
    headings.push({ level: parseInt(m.groups[1]), line: m.line });
  }
  for (let i = 1; i < headings.length; i++) {
    const prev = headings[i - 1].level;
    const curr = headings[i].level;
    if (curr > prev + 1) {
      findings.push({
        line: headings[i].line,
        message: `Heading h${curr} skips level (previous was h${prev})`,
      });
    }
  }
  return findings;
});

addRule("tabindex-positive", "warning", "Avoid positive tabindex values", (content) => {
  const findings = [];
  for (const m of matchesWithLines(content, /tabindex\s*=\s*["']?(\d+)/)) {
    if (!isInTemplate(content, m.line)) continue;
    const val = parseInt(m.groups[1]);
    if (val > 0) {
      findings.push({ line: m.line, message: `Positive tabindex="${val}" disrupts natural tab order` });
    }
  }
  return findings;
});

addRule("aria-hidden-focusable", "error", "aria-hidden elements must not contain focusable children", (content) => {
  const findings = [];
  // Simplified: flag aria-hidden on interactive elements
  for (const m of matchesWithLines(content, /<(?:button|a|input|select|textarea)\b[^>]*aria-hidden\s*=\s*["']true["'][^>]*>/)) {
    if (!isInTemplate(content, m.line)) continue;
    findings.push({ line: m.line, message: "Interactive element has aria-hidden=\"true\" (makes it invisible to assistive tech but still focusable)" });
  }
  return findings;
});

addRule("autoplay-media", "warning", "Media should not autoplay without user control", (content) => {
  const findings = [];
  for (const m of matchesWithLines(content, /<(?:video|audio)\b[^>]*\bautoplay\b/)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;
    if (!/\bmuted\b/.test(m.match)) {
      findings.push({ line: m.line, message: "Autoplaying media without muted attribute" });
    }
  }
  return findings;
});

addRule("missing-lang", "warning", "HTML element should have lang attribute", (content, file) => {
  const findings = [];
  // Only check layout files
  if (!file.includes("+layout.svelte")) return findings;
  for (const m of matchesWithLines(content, /<html\b[^>]*>/)) {
    if (!/\blang\s*=/.test(m.match)) {
      findings.push({ line: m.line, message: "<html> missing lang attribute" });
    }
  }
  return findings;
});

addRule("svg-accessible", "warning", "SVG elements used as images need accessible labels", (content) => {
  const findings = [];
  // Find standalone <svg> tags (not inside other elements, role="img")
  for (const m of matchesWithLines(content, /<svg\b[^>]*>/)) {
    if (!isInTemplate(content, m.line)) continue;
    if (hasSvelteIgnore(content, m.line)) continue;
    const tag = m.match;
    // If it has role="img" it should have aria-label
    if (/role\s*=\s*["']img["']/.test(tag)) {
      if (!/aria-label\s*=/.test(tag) && !/aria-labelledby\s*=/.test(tag)) {
        findings.push({ line: m.line, message: 'SVG with role="img" missing aria-label' });
      }
    }
    // Decorative SVGs with aria-hidden are fine
  }
  return findings;
});

addRule("color-contrast-class", "info", "Text using opacity or very light color classes may have contrast issues", (content) => {
  const findings = [];
  // Flag text with very low opacity
  for (const m of matchesWithLines(content, /class="[^"]*text-[^"]*opacity-(?:10|20|25)[^"]*"/)) {
    if (!isInTemplate(content, m.line)) continue;
    findings.push({ line: m.line, message: "Very low text opacity may cause contrast issues (check WCAG 4.5:1 ratio)" });
  }
  return findings;
});

// ── Run audit ────────────────────────────────────────────────────────────────
const files = walk(SRC_DIR);
let totalErrors = 0;
let totalWarnings = 0;
let totalInfo = 0;
const fileFindings = {};

for (const file of files) {
  const content = readFileSync(file, "utf-8");
  const rel = relative(SRC_DIR, file);

  for (const rule of rules) {
    const findings = rule.check(content, file);
    for (const f of findings) {
      if (!fileFindings[rel]) fileFindings[rel] = [];
      fileFindings[rel].push({ ...f, ruleId: rule.id, severity: rule.severity });
      if (rule.severity === "error") totalErrors++;
      else if (rule.severity === "warning") totalWarnings++;
      else totalInfo++;
    }
  }
}

// ── Report ───────────────────────────────────────────────────────────────────
const COLORS = {
  error: "\x1b[31m",
  warning: "\x1b[33m",
  info: "\x1b[36m",
  reset: "\x1b[0m",
  dim: "\x1b[2m",
  bold: "\x1b[1m",
};

console.log(`\n${COLORS.bold}Accessibility Audit Report${COLORS.reset}`);
console.log(`Scanned ${files.length} Svelte components\n`);

const sortedFiles = Object.keys(fileFindings).sort();
for (const file of sortedFiles) {
  const findings = fileFindings[file];
  console.log(`${COLORS.bold}${file}${COLORS.reset}`);
  for (const f of findings) {
    const sev = COLORS[f.severity];
    console.log(`  ${sev}${f.severity}${COLORS.reset} ${COLORS.dim}L${f.line}${COLORS.reset} [${f.ruleId}] ${f.message}`);
  }
  console.log();
}

// Summary
console.log("─".repeat(60));
console.log(
  `${COLORS.bold}Summary:${COLORS.reset} ` +
  `${COLORS.error ? "\x1b[31m" : ""}${totalErrors} errors${COLORS.reset}, ` +
  `${COLORS.warning ? "\x1b[33m" : ""}${totalWarnings} warnings${COLORS.reset}, ` +
  `${COLORS.info ? "\x1b[36m" : ""}${totalInfo} info${COLORS.reset}`
);
console.log(`Files scanned: ${files.length} | Files with issues: ${sortedFiles.length}\n`);

if (totalErrors === 0 && totalWarnings === 0) {
  console.log(`${COLORS.bold}\x1b[32m✓ No accessibility issues found!${COLORS.reset}\n`);
}

if (CHECK_MODE && totalErrors > 0) {
  process.exit(1);
}
