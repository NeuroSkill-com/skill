<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
  import { marked, Renderer } from "marked";
  import type { Tokens } from "marked";

  let { content = "", pending = false }: { content: string; pending?: boolean } = $props();

  const renderer = new Renderer();

  renderer.code = ({ text, lang }: Tokens.Code): string => {
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
  };

  renderer.codespan = ({ text }: Tokens.Codespan): string => {
    return `<code class="mdr-code">${text}</code>`;
  };

  // ── Derived HTML ──────────────────────────────────────────────────────────

  // Normalize common LLM bold/italic formatting quirks before parsing.
  // Models often emit malformed bold/italic that CommonMark won't parse:
  //   "** word**"  – space after opening delimiter
  //   "**word **"  – space before closing delimiter
  //   "**Label:**x" – closing ** preceded by punctuation and followed by a
  //                   non-whitespace char is NOT "right-flanking" per CommonMark
  //                   spec § 6.4, so the parser treats it as literal asterisks.
  function normalizeMd(raw: string): string {
    return raw
      // 1. Strip stray space after opening ** (e.g. "** word**" → "**word**")
      .replace(/\*\*\s+(\S[\s\S]*?\S)\s*\*\*/g, "**$1**")
      .replace(/\*\*\s+(\S)\*\*/g, "**$1**")
      // 2. Strip stray space before closing ** (e.g. "**word **" → "**word**")
      .replace(/\*\*((?:[^*\n])+?)\s+\*\*/g, (_, g) => `**${g.trimEnd()}**`)
      // 3. CommonMark edge-case: closing ** preceded by punctuation and followed
      //    by a non-whitespace char is not right-flanking and won't close bold.
      //    Convert these to raw <strong> so the browser always renders them bold.
      .replace(/\*\*([^*\n]{1,300}[:.!?,;)\]'"»])\*\*(?=[^\s*])/g, "<strong>$1</strong>")
      // 4. Strip stray spaces inside * (italic)
      .replace(/\*\s+(\S[\s\S]*?\S)\s*\*/g, "*$1*")
      .replace(/\*\s+(\S)\*/g, "*$1*");
  }

  const html = $derived(marked.parse(normalizeMd(content), {
    breaks: true,
    gfm: true,
    renderer,
  }) as string);

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
  {@html html}
</div>
