<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<!--
  Chat voice-input controls — mic toggle + per-session mode picker + status.

  * Continuous trigger → the mic button toggles a continuous voice session
    (start/stop).
  * Push-to-talk trigger → the mic button is press-and-hold (pointerdown opens
    the gate, pointerup closes it).

  The owning chat page drives the daemon (`asr_start` / `asr_stop` /
  `asr_set_ptt`) and feeds back `running`, `phase`, and `errorMsg`; this
  component is presentational + intent-emitting only.
-->
<script lang="ts">
import type { AsrPhase, AsrRouting, AsrTrigger } from "$lib/chat/asr";
import { t } from "$lib/i18n/index.svelte";

interface Props {
  trigger: AsrTrigger;
  routing: AsrRouting;
  running: boolean;
  phase: AsrPhase;
  /** Optional download / loading detail (e.g. Hub weight label). */
  statusDetail?: string;
  errorMsg: string;
  disabled: boolean;
  onSetTrigger: (trigger: AsrTrigger) => void;
  onSetRouting: (routing: AsrRouting) => void;
  /** Continuous trigger: toggle the session on click. */
  onToggle: () => void;
  /** Push-to-talk: pointer down (open the gate). */
  onPttDown: () => void;
  /** Push-to-talk: pointer up / leave (close the gate). */
  onPttUp: () => void;
  onDismissError: () => void;
}

let {
  trigger,
  routing,
  running,
  phase,
  statusDetail = "",
  errorMsg,
  disabled,
  onSetTrigger,
  onSetRouting,
  onToggle,
  onPttDown,
  onPttUp,
  onDismissError,
}: Props = $props();

const isPtt = $derived(trigger === "push_to_talk");

// Press-and-hold needs to release even if the pointer leaves the button.
let pressed = $state(false);
function pttDown(e: PointerEvent) {
  if (disabled) return;
  e.preventDefault();
  pressed = true;
  onPttDown();
}
function pttUp() {
  if (!pressed) return;
  pressed = false;
  onPttUp();
}

const micActive = $derived(isPtt ? pressed : running);

const phaseLabel = $derived(
  phase === "loading"
    ? statusDetail || t("chat.voice.statusLoading")
    : phase === "speaking"
      ? t("chat.voice.statusSpeaking")
      : phase === "listening"
        ? t("chat.voice.statusListening")
        : "",
);
</script>

<div class="flex items-center gap-2 flex-wrap">
  <!-- Mic button: toggle (continuous) or press-and-hold (push-to-talk) -->
  <button
    {disabled}
    onclick={isPtt ? undefined : onToggle}
    onpointerdown={isPtt ? pttDown : undefined}
    onpointerup={isPtt ? pttUp : undefined}
    onpointerleave={isPtt ? pttUp : undefined}
    onpointercancel={isPtt ? pttUp : undefined}
    aria-pressed={micActive}
    title={isPtt ? t("chat.voice.pttHint") : running ? t("chat.voice.stop") : t("chat.voice.start")}
    aria-label={isPtt ? t("chat.voice.pttHint") : running ? t("chat.voice.stop") : t("chat.voice.start")}
    class="shrink-0 w-7 h-7 rounded-lg flex items-center justify-center transition-colors cursor-pointer
           disabled:opacity-30 disabled:cursor-not-allowed select-none touch-none
           {micActive
             ? 'bg-violet-600 text-white hover:bg-violet-700'
             : 'bg-muted text-muted-foreground/60 hover:text-foreground'}"
  >
    {#if micActive}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
           stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 animate-pulse">
        <rect x="9" y="2" width="6" height="11" rx="3"/>
        <path d="M5 10a7 7 0 0 0 14 0"/>
        <line x1="12" y1="17" x2="12" y2="21"/>
      </svg>
    {:else}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"
           stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
        <rect x="9" y="2" width="6" height="11" rx="3"/>
        <path d="M5 10a7 7 0 0 0 14 0"/>
        <line x1="12" y1="17" x2="12" y2="21"/>
      </svg>
    {/if}
  </button>

  <!-- Mode picker: trigger -->
  <div class="flex items-center rounded-lg border border-border bg-muted/40 overflow-hidden">
    <button
      onclick={() => onSetTrigger("continuous")}
      disabled={running}
      class="px-2 py-1 text-ui-xs font-medium transition-colors disabled:cursor-not-allowed
             {trigger === 'continuous'
               ? 'bg-violet-500/15 text-violet-600 dark:text-violet-400'
               : 'text-muted-foreground hover:text-foreground'}"
    >
      {t("chat.voice.triggerContinuous")}
    </button>
    <button
      onclick={() => onSetTrigger("push_to_talk")}
      disabled={running}
      class="px-2 py-1 text-ui-xs font-medium transition-colors disabled:cursor-not-allowed
             {trigger === 'push_to_talk'
               ? 'bg-violet-500/15 text-violet-600 dark:text-violet-400'
               : 'text-muted-foreground hover:text-foreground'}"
    >
      {t("chat.voice.triggerPtt")}
    </button>
  </div>

  <!-- Mode picker: routing -->
  <div class="flex items-center rounded-lg border border-border bg-muted/40 overflow-hidden">
    <button
      onclick={() => onSetRouting("voice_loop")}
      disabled={running}
      class="px-2 py-1 text-ui-xs font-medium transition-colors disabled:cursor-not-allowed
             {routing === 'voice_loop'
               ? 'bg-violet-500/15 text-violet-600 dark:text-violet-400'
               : 'text-muted-foreground hover:text-foreground'}"
    >
      {t("chat.voice.routingLoop")}
    </button>
    <button
      onclick={() => onSetRouting("transcribe_only")}
      disabled={running}
      class="px-2 py-1 text-ui-xs font-medium transition-colors disabled:cursor-not-allowed
             {routing === 'transcribe_only'
               ? 'bg-violet-500/15 text-violet-600 dark:text-violet-400'
               : 'text-muted-foreground hover:text-foreground'}"
    >
      {t("chat.voice.routingTranscribe")}
    </button>
  </div>

  <!-- Live status indicator -->
  {#if errorMsg}
    <span class="flex items-center gap-1 text-ui-xs text-red-600 dark:text-red-400">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"
           stroke-linecap="round" stroke-linejoin="round" class="w-3 h-3 shrink-0">
        <circle cx="8" cy="8" r="6.5"/><line x1="8" y1="5" x2="8" y2="8.5"/><line x1="8" y1="11" x2="8.01" y2="11"/>
      </svg>
      <span class="truncate max-w-[16rem]">{errorMsg}</span>
      <button onclick={onDismissError} aria-label={t("chat.voice.dismissError")}
              class="shrink-0 p-0.5 rounded hover:bg-red-500/10 cursor-pointer">
        <svg viewBox="0 0 10 10" class="w-2 h-2"><path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/></svg>
      </button>
    </span>
  {:else if phaseLabel}
    <span class="flex items-center gap-1.5 text-ui-xs
                 {phase === 'speaking'
                   ? 'text-violet-600 dark:text-violet-400'
                   : 'text-muted-foreground'}">
      {#if phase === 'loading'}
        <svg class="w-3 h-3 animate-spin" viewBox="0 0 24 24" fill="none">
          <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" class="opacity-20"/>
          <path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
        </svg>
      {:else if phase === 'speaking'}
        <span aria-hidden="true">🎤</span>
      {:else}
        <span class="relative flex h-2 w-2">
          <span class="animate-ping absolute inline-flex h-full w-full rounded-full bg-violet-500/60"></span>
          <span class="relative inline-flex rounded-full h-2 w-2 bg-violet-500"></span>
        </span>
      {/if}
      {phaseLabel}
    </span>
  {/if}
</div>
