<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Renders an LLM response as markdown.

  • Uses a local `Marked` instance (no global state pollution).
  • Custom renderer wraps code blocks with a language label and Copy button.
  • Copy button is handled via event delegation on the wrapper div.
  • All injected HTML elements are styled via :global() rules that respect
    the app's CSS variable theme (light / dark / high-contrast).
  • Safe for streaming: `marked.parse()` tolerates unclosed fences.
-->
<script lang="ts">
  import { Marked }   from "marked";
  import type { Tokens } from "marked";

  let { content = "", pending = false }: { content: string; pending?: boolean } = $props();

  // ── Local Marked instance ─────────────────────────────────────────────────

  const md = new Marked({
    breaks: true,
    gfm:    true,
    renderer: {
      // ── Fenced code block ──────────────────────────────────────────────
      code({ text, lang }: Tokens.Code): string {
        const escaped = text
          .replace(/&/g, "&amp;")
          .replace(/</g, "&lt;")
          .replace(/>/g, "&gt;");
        const label = lang
          ? `<span class="mdr-lang">${lang}</span>`
          : `<span></span>`;
        return `<div class="mdr-pre">`
          + `<div class="mdr-bar">${label}`
          + `<button class="mdr-copy" data-copy>Copy</button></div>`
          + `<pre><code>${escaped}</code></pre></div>`;
      },
      // ── Inline code ────────────────────────────────────────────────────
      codespan({ text }: Tokens.Codespan): string {
        return `<code class="mdr-code">${text}</code>`;
      },
    },
  });

  // ── Derived HTML ──────────────────────────────────────────────────────────

  const html = $derived(md.parse(content) as string);

  // ── Copy handler (event delegation) ──────────────────────────────────────

  function onCopy(e: MouseEvent) {
    const btn = (e.target as HTMLElement).closest("[data-copy]") as HTMLElement | null;
    if (!btn) return;
    const code = btn.closest(".mdr-pre")?.querySelector("code")?.textContent ?? "";
    navigator.clipboard.writeText(code).catch(() => {});
    btn.textContent = "Copied!";
    setTimeout(() => { btn.textContent = "Copy"; }, 1500);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="mdr" onclick={onCopy}>
  {@html html}{#if pending}<span
    class="inline-block w-0.5 h-[1em] bg-foreground/70 animate-pulse ml-0.5 align-middle"
  ></span>{/if}
</div>

<style>
  /* ── Wrapper ─────────────────────────────────────────────────────────────── */
  .mdr {
    font-size: inherit;
    line-height: 1.65;
    color: var(--foreground);
    word-break: break-word;
  }

  /* ── Block spacing ───────────────────────────────────────────────────────── */
  .mdr :global(p)          { margin: 0.35em 0; }
  .mdr :global(p:first-child) { margin-top: 0; }
  .mdr :global(p:last-child)  { margin-bottom: 0; }

  /* ── Headings ────────────────────────────────────────────────────────────── */
  .mdr :global(h1),
  .mdr :global(h2),
  .mdr :global(h3),
  .mdr :global(h4) {
    font-weight: 600;
    line-height: 1.3;
    margin: 0.9em 0 0.3em;
    color: var(--foreground);
  }
  .mdr :global(h1) { font-size: 1.15em; }
  .mdr :global(h2) { font-size: 1.05em; }
  .mdr :global(h3) { font-size: 0.97em; }
  .mdr :global(h4) { font-size: 0.92em; }
  .mdr :global(h1:first-child),
  .mdr :global(h2:first-child),
  .mdr :global(h3:first-child),
  .mdr :global(h4:first-child) { margin-top: 0; }

  /* ── Lists ───────────────────────────────────────────────────────────────── */
  .mdr :global(ul),
  .mdr :global(ol)  { margin: 0.35em 0; padding-left: 1.5em; }
  .mdr :global(li)  { margin: 0.15em 0; }
  .mdr :global(li > p) { margin: 0; }
  .mdr :global(ul)  { list-style-type: disc; }
  .mdr :global(ol)  { list-style-type: decimal; }

  /* ── Emphasis ────────────────────────────────────────────────────────────── */
  .mdr :global(strong) { font-weight: 650; }
  .mdr :global(em)     { font-style: italic; }
  .mdr :global(del)    { text-decoration: line-through; opacity: 0.6; }

  /* ── Links ───────────────────────────────────────────────────────────────── */
  .mdr :global(a) {
    color: oklch(0.58 0.24 293);      /* violet-500 */
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .mdr :global(a:hover) { color: oklch(0.49 0.24 293); }

  /* ── Blockquote ──────────────────────────────────────────────────────────── */
  .mdr :global(blockquote) {
    border-left: 3px solid oklch(0.58 0.24 293 / 35%);
    margin: 0.55em 0;
    padding: 0.25em 0.85em;
    color: var(--muted-foreground);
    font-style: italic;
  }

  /* ── Horizontal rule ─────────────────────────────────────────────────────── */
  .mdr :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 0.85em 0;
  }

  /* ── Tables ──────────────────────────────────────────────────────────────── */
  .mdr :global(table) {
    border-collapse: collapse;
    width: 100%;
    margin: 0.6em 0;
    font-size: 0.88em;
  }
  .mdr :global(th),
  .mdr :global(td) {
    border: 1px solid var(--border);
    padding: 0.28em 0.65em;
    text-align: left;
  }
  .mdr :global(th) {
    background: var(--muted);
    font-weight: 600;
  }
  .mdr :global(tr:nth-child(even) td) {
    background: oklch(from var(--muted) l c h / 40%);
  }

  /* ── Inline code ─────────────────────────────────────────────────────────── */
  .mdr :global(.mdr-code) {
    font-family: ui-monospace, "Cascadia Code", "Fira Code", "Consolas", monospace;
    font-size: 0.83em;
    background: oklch(0.58 0.24 293 / 9%);
    color: oklch(0.52 0.22 293);
    padding: 0.12em 0.38em;
    border-radius: 4px;
    border: 1px solid oklch(0.58 0.24 293 / 18%);
  }

  /* ── Code block container ────────────────────────────────────────────────── */
  .mdr :global(.mdr-pre) {
    margin: 0.55em 0;
    border-radius: 8px;
    border: 1px solid var(--border);
    overflow: hidden;
    background: var(--muted);
  }

  /* header bar */
  .mdr :global(.mdr-bar) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.22em 0.75em;
    background: oklch(from var(--muted) calc(l - 0.03) c h);
    border-bottom: 1px solid var(--border);
  }

  /* language label */
  .mdr :global(.mdr-lang) {
    font-family: ui-monospace, monospace;
    font-size: 0.65rem;
    color: var(--muted-foreground);
    letter-spacing: 0.03em;
    opacity: 0.75;
  }

  /* copy button */
  .mdr :global(.mdr-copy) {
    background: none;
    border: none;
    font-size: 0.62rem;
    font-family: inherit;
    color: var(--muted-foreground);
    cursor: pointer;
    padding: 0.1em 0.45em;
    border-radius: 3px;
    transition: color 0.15s, background 0.15s;
  }
  .mdr :global(.mdr-copy:hover) {
    color: var(--foreground);
    background: oklch(0.58 0.24 293 / 12%);
  }

  /* the <pre> itself */
  .mdr :global(.mdr-pre pre) {
    margin: 0;
    padding: 0.65em 0.85em;
    overflow-x: auto;
    font-family: ui-monospace, "Cascadia Code", "Fira Code", "Consolas", monospace;
    font-size: 0.77rem;
    line-height: 1.55;
    tab-size: 2;
  }

  /* <code> inside the block — reset inline-code styling */
  .mdr :global(.mdr-pre code) {
    font-size: inherit;
    background: none;
    color: var(--foreground);
    padding: 0;
    border: none;
    border-radius: 0;
  }
</style>
