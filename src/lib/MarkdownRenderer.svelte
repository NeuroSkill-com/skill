<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
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
