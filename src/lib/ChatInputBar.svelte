<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!-- Chat input bar — message textarea, image attachments, prompt library button. -->
<script lang="ts">
  import { tick }          from "svelte";
  import { t }             from "$lib/i18n/index.svelte";
  import PromptLibrary     from "$lib/PromptLibrary.svelte";
  import type { Attachment, ServerStatus } from "$lib/chat-types";

  interface Props {
    input: string;
    attachments: Attachment[];
    status: ServerStatus;
    generating: boolean;
    aborting: boolean;
    canSend: boolean;
    supportsVision: boolean;
    nCtx: number;
    liveUsedTokens: number;
    onSend: () => void;
    onAbort: () => void;
    onInputKeydown: (e: KeyboardEvent) => void;
    onBeforeInput: (e: InputEvent) => void;
  }

  let {
    input           = $bindable(),
    attachments     = $bindable(),
    status,
    generating,
    aborting,
    canSend,
    supportsVision,
    nCtx,
    liveUsedTokens,
    onSend,
    onAbort,
    onInputKeydown,
    onBeforeInput,
  }: Props = $props();

  let inputEl      = $state<HTMLTextAreaElement | null>(null);
  let fileInputEl  = $state<HTMLInputElement | null>(null);
  let promptLibRef = $state<PromptLibrary | null>(null);

  export function focus() { inputEl?.focus(); }
  export function getInputEl() { return inputEl; }

  export function autoResize() {
    if (!inputEl) return;
    inputEl.style.height = "auto";
    inputEl.style.height = Math.min(inputEl.scrollHeight, 200) + "px";
  }

  function openFilePicker() { fileInputEl?.click(); }

  async function onFilesSelected(e: Event) {
    const files = (e.target as HTMLInputElement).files;
    if (!files) return;
    for (const file of Array.from(files)) {
      if (!file.type.startsWith("image/")) continue;
      const dataUrl = await readFileAsDataUrl(file);
      attachments = [...attachments, { dataUrl, mimeType: file.type, name: file.name }];
    }
    if (fileInputEl) fileInputEl.value = "";
  }

  function removeAttachment(i: number) {
    attachments = attachments.filter((_, idx) => idx !== i);
  }

  function readFileAsDataUrl(file: File): Promise<string> {
    return new Promise((res, rej) => {
      const reader = new FileReader();
      reader.onload  = () => res(reader.result as string);
      reader.onerror = rej;
      reader.readAsDataURL(file);
    });
  }

  async function selectPrompt(text: string) {
    input = text;
    await tick();
    autoResize();
    inputEl?.focus();
    inputEl?.setSelectionRange(input.length, input.length);
  }
</script>

<footer class="relative shrink-0 border-t border-border dark:border-white/[0.06]
                bg-white dark:bg-[#0f0f18] px-3 py-2.5">

  <!-- Vision-not-supported warning -->
  {#if attachments.length > 0 && !supportsVision}
    <div class="flex items-center gap-2 mb-2 px-2 py-1.5 rounded-lg
                 bg-amber-500/10 border border-amber-500/30 text-amber-600 dark:text-amber-400">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
           stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 shrink-0">
        <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
        <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
      </svg>
      <span class="text-[0.65rem] font-medium leading-snug">
        {t("chat.noVision")}
      </span>
    </div>
  {/if}

  <!-- Attachment thumbnails strip -->
  {#if attachments.length > 0}
    <div class="flex flex-wrap gap-1.5 mb-2 px-1">
      {#each attachments as att, i}
        <div class="relative group">
          <img src={att.dataUrl} alt={att.name}
               class="h-16 w-16 rounded-lg object-cover border border-border shadow-sm
                      {!supportsVision ? 'opacity-50 grayscale' : ''}" />
          <button
            onclick={() => removeAttachment(i)}
            aria-label={t("chat.removeAttachment")}
            class="absolute -top-1.5 -right-1.5 w-4 h-4 rounded-full bg-red-500 text-white
                   flex items-center justify-center opacity-0 group-hover:opacity-100
                   transition-opacity cursor-pointer shadow">
            <svg viewBox="0 0 10 10" fill="currentColor" class="w-2 h-2">
              <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5"
                    stroke-linecap="round"/>
            </svg>
          </button>
        </div>
      {/each}
    </div>
  {/if}

  <!-- Prompt-library floating panel -->
  <PromptLibrary
    bind:this={promptLibRef}
    onSelect={selectPrompt}
  />

  <!-- Context window warning -->
  {#if nCtx > 0 && liveUsedTokens > 0}
    {@const warnPct = Math.round((liveUsedTokens / nCtx) * 100)}
    {#if warnPct >= 85}
      <div class="flex items-center justify-center gap-1.5 mb-1.5 px-2 py-1 rounded-md
                  {warnPct >= 95 ? 'bg-red-500/10 border border-red-500/20' : 'bg-amber-500/10 border border-amber-500/20'}">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"
             stroke-linecap="round" stroke-linejoin="round"
             class="w-3 h-3 shrink-0 {warnPct >= 95 ? 'text-red-500' : 'text-amber-500'}">
          <path d="M7.15 2.43L1.41 12a1 1 0 0 0 .86 1.5h11.46a1 1 0 0 0 .86-1.5L8.85 2.43a1 1 0 0 0-1.7 0z"/>
          <line x1="8" y1="6" x2="8" y2="9"/><line x1="8" y1="11" x2="8.01" y2="11"/>
        </svg>
        <span class="text-[0.55rem] {warnPct >= 95 ? 'text-red-600 dark:text-red-400' : 'text-amber-600 dark:text-amber-400'} leading-tight">
          {t("chat.ctxWarning", { pct: warnPct })}
        </span>
      </div>
    {/if}
  {/if}

  <!-- LLM accuracy warning -->
  <div class="flex items-center justify-center gap-1.5 mb-1.5 px-2 py-1 rounded-md
              bg-amber-500/8 border border-amber-500/15">
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"
         stroke-linecap="round" stroke-linejoin="round"
         class="w-3 h-3 shrink-0 text-amber-500/70">
      <path d="M7.15 2.43L1.41 12a1 1 0 0 0 .86 1.5h11.46a1 1 0 0 0 .86-1.5L8.85 2.43a1 1 0 0 0-1.7 0z"/>
      <line x1="8" y1="6" x2="8" y2="9"/><line x1="8" y1="11" x2="8.01" y2="11"/>
    </svg>
    <span class="text-[0.52rem] text-amber-600/70 dark:text-amber-400/70 leading-tight select-none">
      {t("chat.hint.llmWarning")}
    </span>
  </div>

  <div class="flex items-end gap-2 rounded-xl border border-border dark:border-white/[0.08]
              bg-background px-3 py-2
              focus-within:ring-1 focus-within:ring-violet-500/50
              focus-within:border-violet-500/30 transition-all">

    <!-- Image attach button -->
    <button
      onclick={openFilePicker}
      disabled={status !== "running" || generating}
      title={supportsVision ? t("chat.attachImage") : t("chat.attachImageNoVision")}
      class="shrink-0 p-1 rounded-md transition-colors cursor-pointer self-center
             disabled:opacity-30 disabled:cursor-not-allowed
             {supportsVision
               ? 'text-muted-foreground/50 hover:text-foreground hover:bg-muted'
               : 'text-amber-500/70 hover:text-amber-500 hover:bg-amber-500/10'}">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
           stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
        <rect x="3" y="3" width="18" height="18" rx="2"/>
        <circle cx="8.5" cy="8.5" r="1.5"/>
        <polyline points="21 15 16 10 5 21"/>
      </svg>
    </button>

    <!-- Prompt library button -->
    <button
      onclick={() => promptLibRef?.toggle()}
      disabled={status !== "running" || generating}
      title={t("chat.prompts.btn")}
      aria-label={t("chat.prompts.btn")}
      class="shrink-0 p-1 rounded-md transition-colors cursor-pointer self-center
             disabled:opacity-30 disabled:cursor-not-allowed
             {promptLibRef?.isOpen()
               ? 'text-violet-600 dark:text-violet-400 bg-violet-500/10'
               : 'text-muted-foreground/50 hover:text-violet-600 dark:hover:text-violet-400 hover:bg-violet-500/10'}">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
           stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
        <path d="M12 2l1.5 4.5L18 8l-4.5 1.5L12 14l-1.5-4.5L6 8l4.5-1.5Z"/>
        <path d="M19 14l.75 2.25L22 17l-2.25.75L19 20l-.75-2.25L16 17l2.25-.75Z"/>
        <path d="M5 17l.5 1.5L7 19l-1.5.5L5 21l-.5-1.5L3 19l1.5-.5Z"/>
      </svg>
    </button>

    <textarea
      bind:this={inputEl}
      bind:value={input}
      onkeydown={onInputKeydown}
      onbeforeinput={onBeforeInput}
      oninput={autoResize}
      placeholder={status === "running" ? t("chat.inputPlaceholder")
                   : status === "loading" ? t("chat.status.loading")
                   : t("chat.inputPlaceholderStopped")}
      disabled={status !== "running" || generating}
      rows="1"
      class="flex-1 bg-transparent text-[0.78rem] text-foreground resize-none
             placeholder:text-muted-foreground/40 focus:outline-none
             disabled:opacity-50 disabled:cursor-not-allowed
             max-h-48 leading-relaxed"
    ></textarea>

    {#if generating}
      <button
        onclick={onAbort}
        disabled={aborting}
        aria-label={aborting ? t("chat.btn.aborting") : t("chat.btn.stop")}
        title={aborting ? t("chat.btn.aborting") : t("chat.btn.stop")}
        class="shrink-0 h-7 rounded-lg flex items-center justify-center gap-1 px-2
               transition-colors cursor-pointer disabled:cursor-wait
               {aborting
                 ? 'bg-red-500/20 text-red-400'
                 : 'bg-red-500/10 text-red-500 hover:bg-red-500/20'}">
        {#if aborting}
          <svg class="w-3 h-3 animate-spin" viewBox="0 0 24 24" fill="none">
            <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="2.5" class="opacity-20"/>
            <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" class="opacity-80"/>
          </svg>
        {:else}
          <svg viewBox="0 0 24 24" fill="currentColor" class="w-3.5 h-3.5">
            <rect x="4" y="4" width="16" height="16" rx="2"/>
          </svg>
        {/if}
      </button>
    {:else}
      <button
        onclick={onSend}
        disabled={!canSend}
        aria-label={t("chat.btn.send")}
        class="shrink-0 w-7 h-7 rounded-lg flex items-center justify-center transition-colors
               {canSend
                 ? 'bg-violet-600 hover:bg-violet-700 text-white cursor-pointer'
                 : 'bg-muted text-muted-foreground/30 cursor-not-allowed'}">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
             stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 -rotate-90">
          <line x1="12" y1="19" x2="12" y2="5"/>
          <polyline points="5 12 12 5 19 12"/>
        </svg>
      </button>
    {/if}
  </div>

  <p class="text-[0.55rem] text-muted-foreground/30 text-center mt-1.5">
    {#if status === "running"}
      {t("chat.hint.running")}
    {:else if status === "loading"}
      {t("chat.hint.loading")}
    {:else}
      {t("chat.hint.stopped")}
    {/if}
  </p>
</footer>

<!-- Hidden file input for image uploads -->
<input
  bind:this={fileInputEl}
  type="file"
  accept="image/*"
  multiple
  class="hidden"
  onchange={onFilesSelected}
/>
