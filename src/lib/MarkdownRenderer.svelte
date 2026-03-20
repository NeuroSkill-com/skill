<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
  import { marked, Renderer } from "marked";
  import type { Tokens } from "marked";
  import { normalizeMarkdown } from "$lib/markdown-normalize";

  let {
    content = "",
    pending = false,
    className = "",
  }: {
    content: string;
    pending?: boolean;
    className?: string;
  } = $props();

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

  const html = $derived(marked.parse(normalizeMarkdown(content), {
    breaks: true,
    gfm: true,
    renderer,
  }) as string);

  // ── Copy handler (event delegation) ──────────────────────────────────────

  function onCopy(e: MouseEvent) {
    const btn = (e.target as HTMLElement).closest("[data-copy]") as HTMLElement | null;
    if (!btn) return;
    const code = btn.closest(".mdr-pre")?.querySelector("code")?.textContent ?? "";
    navigator.clipboard.writeText(code).catch(e => console.warn("[markdown] clipboard write failed:", e));
    btn.textContent = "Copied!";
    setTimeout(() => { btn.textContent = "Copy"; }, 1500);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class={`mdr ${className}`.trim()} onclick={onCopy}>
  {@html html}
</div>
