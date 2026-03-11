<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  PromptLibrary — floating panel of built-in neurofeedback prompt templates.

  Usage
  ─────
  Bind a ref and call toggle() / close() from the parent:

    <PromptLibrary bind:this={promptLibRef} {onSelect} />

  The `onSelect(text)` callback receives the chosen template string.
  The panel closes automatically after a selection or when the user clicks
  outside / presses Escape.
-->
<script lang="ts">
  import { t } from "$lib/i18n/index.svelte";

  // ── Props ──────────────────────────────────────────────────────────────────

  let { onSelect }: { onSelect: (text: string) => void } = $props();

  // ── State ──────────────────────────────────────────────────────────────────

  let open     = $state(false);
  let panelEl  = $state<HTMLElement | null>(null);

  // ── Exposed API (bind:this) ────────────────────────────────────────────────

  export function toggle() { open = !open; }
  export function close()  { open = false;  }
  export function isOpen() { return open;   }

  // ── Click-outside handling ─────────────────────────────────────────────────

  function handleDocumentPointerDown(e: PointerEvent) {
    if (!open) return;
    if (panelEl && !panelEl.contains(e.target as Node)) {
      open = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (open && e.key === "Escape") { e.stopPropagation(); open = false; }
  }

  // ── Template data ──────────────────────────────────────────────────────────

  interface Category {
    key:     string;
    emoji:   string;
    prompts: string[];
  }

  const CATEGORIES: Category[] = [
    {
      key:   "session",
      emoji: "📋",
      prompts: [
        "Summarise today's neurofeedback session in plain language",
        "What were the key highlights of my last recording?",
        "Create a brief journal entry based on my session data",
        "How does today's brain activity compare to a healthy baseline?",
        "Give me a one-paragraph overview of what happened in my session",
      ],
    },
    {
      key:   "relax",
      emoji: "🧘",
      prompts: [
        "Suggest a relaxation technique to boost my alpha waves",
        "I feel tense — what breathing exercise can help right now?",
        "How can I enter a deeper meditative state with neurofeedback?",
        "My stress index is elevated — what should I do?",
        "Guide me through a quick coherence-building exercise",
      ],
    },
    {
      key:   "education",
      emoji: "📚",
      prompts: [
        "Explain what high theta activity means in everyday language",
        "What does elevated beta tell me about my mental state?",
        "Why is frontal alpha asymmetry (FAA) important in neurofeedback?",
        "What is the theta/alpha ratio and why does it matter?",
        "Explain gamma oscillations and when they appear",
        "What does a low alpha peak frequency (APF) indicate?",
        "Describe the difference between delta, theta, alpha, beta, and gamma",
        "What is EEG coherence and what does high coherence mean?",
      ],
    },
    {
      key:   "focus",
      emoji: "🎯",
      prompts: [
        "How can I improve my focus score during a session?",
        "What brain state is best for deep, sustained work?",
        "My cognitive load is high — how do I reduce it?",
        "Suggest a neurofeedback protocol to improve attention",
        "What should I do before a session to maximise engagement?",
        "How does heart-rate variability relate to focus?",
      ],
    },
    {
      key:   "analysis",
      emoji: "🔬",
      prompts: [
        "Interpret my current FAA score for me",
        "What does my drowsiness level suggest about my mental state?",
        "Analyse my coherence reading and what it means practically",
        "Is my current brain state typical for this time of day?",
        "What do my current beta/alpha and theta/alpha ratios tell me?",
        "Explain what my mood score means in terms of brain activity",
        "What does a high Lempel-Ziv complexity value indicate?",
      ],
    },
  ];

  // ── Interaction ────────────────────────────────────────────────────────────

  function pick(text: string) {
    onSelect(text);
    open = false;
  }
</script>

<!-- Listen for click-outside and Escape anywhere on the document -->
<svelte:document
  onpointerdown={handleDocumentPointerDown}
  onkeydown={handleKeydown}
/>

<!-- ─────────────────────────────────────────────────────────────────────────── -->
<!-- Trigger + floating panel wrapper                                            -->
<!-- The parent places this element in-flow; the panel is positioned absolute   -->
<!-- relative to the nearest positioned ancestor (the footer).                  -->
<!-- ─────────────────────────────────────────────────────────────────────────── -->
{#if open}
  <div
    bind:this={panelEl}
    role="dialog"
    aria-label={t("chat.prompts.title")}
    class="absolute bottom-full left-0 mb-2 z-50
           w-[min(96vw,560px)] max-h-[min(70vh,400px)]
           flex flex-col
           rounded-2xl border border-border dark:border-white/[0.08]
           bg-white dark:bg-[#111118]
           shadow-2xl shadow-black/20 dark:shadow-black/50
           overflow-hidden
           animate-in fade-in slide-in-from-bottom-2 duration-150">

    <!-- Header -->
    <div class="flex items-center justify-between gap-2
                px-4 py-2.5
                border-b border-border dark:border-white/[0.06]
                bg-slate-50/80 dark:bg-[#0d0d14] shrink-0">
      <div class="flex items-center gap-2">
        <!-- Sparkle icon -->
        <svg viewBox="0 0 20 20" fill="none" stroke="currentColor"
             stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"
             class="w-3.5 h-3.5 text-violet-500 shrink-0">
          <path d="M10 2v2M10 16v2M2 10h2M16 10h2
                   M4.22 4.22l1.42 1.42M14.36 14.36l1.42 1.42
                   M4.22 15.78l1.42-1.42M14.36 5.64l1.42-1.42"/>
          <circle cx="10" cy="10" r="3" fill="currentColor" stroke="none" opacity="0.25"/>
          <circle cx="10" cy="10" r="1.5" fill="currentColor" stroke="none"/>
        </svg>
        <span class="text-[0.68rem] font-semibold text-foreground">
          {t("chat.prompts.title")}
        </span>
      </div>
      <span class="text-[0.58rem] text-muted-foreground/50">
        {t("chat.prompts.subtitle")}
      </span>
    </div>

    <!-- Scrollable categories -->
    <div class="flex-1 overflow-y-auto
                scrollbar-thin scrollbar-track-transparent scrollbar-thumb-border
                px-3 py-3 flex flex-col gap-4">
      {#each CATEGORIES as cat}
        <section>
          <!-- Category label -->
          <div class="flex items-center gap-1.5 mb-1.5">
            <span class="text-base leading-none" aria-hidden="true">{cat.emoji}</span>
            <span class="text-[0.58rem] font-semibold uppercase tracking-widest
                         text-muted-foreground/60">
              {t(`chat.prompts.cat.${cat.key}`)}
            </span>
          </div>

          <!-- Prompt chips -->
          <div class="flex flex-wrap gap-1.5">
            {#each cat.prompts as prompt}
              <button
                onclick={() => pick(prompt)}
                class="px-2.5 py-1 rounded-lg text-[0.68rem] text-left leading-snug
                       border border-border dark:border-white/[0.07]
                       bg-background hover:bg-violet-500/8 dark:hover:bg-violet-500/12
                       hover:border-violet-400/40 dark:hover:border-violet-500/30
                       text-foreground/80 hover:text-violet-700 dark:hover:text-violet-300
                       transition-all cursor-pointer select-none
                       focus:outline-none focus:ring-1 focus:ring-violet-500/50">
                {prompt}
              </button>
            {/each}
          </div>
        </section>
      {/each}
    </div>

    <!-- Footer hint -->
    <div class="shrink-0 px-4 py-1.5
                border-t border-border dark:border-white/[0.05]
                bg-slate-50/50 dark:bg-[#0d0d14]">
      <p class="text-[0.55rem] text-muted-foreground/40 text-center">
        {t("chat.prompts.hint")}
      </p>
    </div>
  </div>
{/if}
