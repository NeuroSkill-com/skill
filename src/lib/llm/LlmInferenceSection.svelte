<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { ToggleRow } from "$lib/components/ui/toggle-row";
import { t } from "$lib/i18n/index.svelte";

interface LlmConfigView {
  n_gpu_layers: number;
  ctx_size: number | null;
  n_batch: number | null;
  n_ubatch: number | null;
  parallel: number;
  api_key: string | null;
  autoload_mmproj: boolean;
  no_mmproj_gpu: boolean;
  verbose: boolean;
  flash_attention: boolean;
  offload_kqv: boolean;
  gpu_memory_threshold: number;
  gpu_memory_gen_threshold: number;
  cache_type_k: string;
  cache_type_v: string;
  attn_rot_disabled: boolean;
}

interface Props {
  config: LlmConfigView;
  configSaving: boolean;
  wsPort: number;
  activeMaxCtx: number;
  hasAnyMmproj: boolean;
  hasDownloadedMmproj: boolean;
  onSetGpuLayers: (val: number) => void | Promise<void>;
  onSetCtxSize: (val: number | null) => void | Promise<void>;
  onSetParallel: (val: number) => void | Promise<void>;
  onSetApiKey: (val: string | null) => void | Promise<void>;
  onToggleAutoloadMmproj: () => void | Promise<void>;
  onToggleNoMmprojGpu: () => void | Promise<void>;
  onSetGpuMemoryThreshold: (val: number) => void | Promise<void>;
  onSetGpuMemoryGenThreshold: (val: number) => void | Promise<void>;
  onSetCacheTypeK: (val: string) => void | Promise<void>;
  onSetCacheTypeV: (val: string) => void | Promise<void>;
  onToggleAttnRotDisabled: () => void | Promise<void>;
  onSetNBatch: (val: number | null) => void | Promise<void>;
  onSetNUbatch: (val: number | null) => void | Promise<void>;
  onToggleFlashAttention: () => void | Promise<void>;
  onToggleOffloadKqv: () => void | Promise<void>;
}

let {
  config,
  configSaving,
  wsPort,
  activeMaxCtx,
  hasAnyMmproj,
  hasDownloadedMmproj,
  onSetGpuLayers,
  onSetCtxSize,
  onSetParallel,
  onSetApiKey,
  onToggleAutoloadMmproj,
  onToggleNoMmprojGpu,
  onSetGpuMemoryThreshold,
  onSetGpuMemoryGenThreshold,
  onSetCacheTypeK,
  onSetCacheTypeV,
  onToggleAttnRotDisabled,
  onSetNBatch,
  onSetNUbatch,
  onToggleFlashAttention,
  onToggleOffloadKqv,
}: Props = $props();

const KV_TYPES = [
  { tag: "f16", label: "F16" },
  { tag: "q8_0", label: "Q8_0" },
  { tag: "q5_0", label: "Q5_0" },
  { tag: "q4_0", label: "Q4_0" },
] as const;

let showAdvanced = $state(false);
let apiKeyVisible = $state(false);

const ctxOptions = $derived.by(() =>
  ([[null, "auto"]] as [number | null, string][]).concat(
    (
      [
        [4096, "4K"],
        [8192, "8K"],
        [16384, "16K"],
        [32768, "32K"],
        [65536, "64K"],
        [131072, "128K"],
      ] as [number, string][]
    ).filter(([val]) => activeMaxCtx === 0 || (val as number) <= activeMaxCtx),
  ),
);

const curlSnippet = $derived(
  `curl http://localhost:${wsPort}/v1/chat/completions \\\n  -H 'Content-Type: application/json' \\\n  -d '{"model":"default","messages":[{"role":"user","content":"Hello!"}]}'`,
);
</script>

<section class="flex flex-col gap-2">
  <button onclick={() => (showAdvanced = !showAdvanced)}
    class="flex items-center gap-2 px-0.5 cursor-pointer select-none group">
    <span class="text-ui-xs font-semibold tracking-widest uppercase text-muted-foreground group-hover:text-foreground transition-colors">
      {t("llm.section.inference")}
    </span>
    <svg viewBox="0 0 16 16" fill="currentColor"
         class="w-2.5 h-2.5 text-muted-foreground/50 transition-transform {showAdvanced ? 'rotate-180' : ''}">
      <path d="M3 6l5 5 5-5H3z"/>
    </svg>
    {#if configSaving}<span class="text-ui-xs text-muted-foreground">saving…</span>{/if}
  </button>

  {#if showAdvanced}
    <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
      <CardContent class="flex flex-col divide-y divide-border dark:divide-white/[0.05] py-0 px-0">
        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex items-baseline justify-between">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.gpuLayers")}</span>
            <span class="text-ui-base text-muted-foreground tabular-nums">
              {config.n_gpu_layers === 0 ? "CPU only" : config.n_gpu_layers >= 999 ? "All layers" : config.n_gpu_layers}
            </span>
          </div>
          <p class="text-ui-base text-muted-foreground -mt-1">{t("llm.inference.gpuLayersDesc")}</p>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each [[0, "CPU"], [8, "8"], [16, "16"], [32, "32"], [4294967295, "All"]] as [val, label]}
              <button onclick={() => onSetGpuLayers(val as number)}
                class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                     {((val as number) >= 999 ? config.n_gpu_layers >= 999 : config.n_gpu_layers === val)
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                {label}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-2 px-4 py-3.5">
          <div class="flex items-baseline justify-between">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.ctxSize")}</span>
            <span class="text-ui-base text-muted-foreground tabular-nums">{config.ctx_size != null ? config.ctx_size + " tokens" : "auto"}</span>
          </div>
          <p class="text-ui-base text-muted-foreground -mt-1">{t("llm.inference.ctxSizeDesc")}</p>
          <div class="flex items-center gap-1.5 flex-wrap">
            {#each ctxOptions as [val, label]}
              <button onclick={() => onSetCtxSize(val as number | null)}
                class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                     {(config.ctx_size ?? null) === val
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                {label}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex items-center justify-between gap-4 px-4 py-3.5">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.parallel")}</span>
            <span class="text-ui-base text-muted-foreground">{t("llm.inference.parallelDesc")}</span>
          </div>
          <div class="flex items-center gap-1.5">
            {#each [1, 2, 4] as val}
              <button onclick={() => onSetParallel(val)}
                class="rounded-lg border px-2.5 py-1.5 text-ui-base font-semibold transition-all cursor-pointer
                     {config.parallel === val
                       ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                       : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                {val}
              </button>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-2 px-4 py-3.5">
          <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.apiKey")}</span>
          <p class="text-ui-base text-muted-foreground -mt-1">{t("llm.inference.apiKeyDesc")}</p>
          <div class="flex items-center gap-2">
            <input type={apiKeyVisible ? "text" : "password"}
              aria-label="API key"
              placeholder={t("llm.inference.apiKeyPlaceholder")}
              value={config.api_key ?? ""}
              oninput={(e: Event) => onSetApiKey((e.target as HTMLInputElement).value || null)}
              class="flex-1 min-w-0 text-ui-md font-mono px-2 py-1 rounded-md
                   border border-border bg-background text-foreground placeholder:text-muted-foreground/40" />
            <button onclick={() => (apiKeyVisible = !apiKeyVisible)}
              class="shrink-0 text-ui-sm text-muted-foreground hover:text-foreground cursor-pointer">
              {apiKeyVisible ? "hide" : "show"}
            </button>
            {#if config.api_key}
              <button onclick={() => onSetApiKey(null)}
                class="shrink-0 text-ui-sm text-muted-foreground hover:text-red-500 cursor-pointer">clear</button>
            {/if}
          </div>
        </div>

        {#if hasAnyMmproj}
          <ToggleRow
            checked={config.autoload_mmproj}
            label={t("llm.mmproj.autoload")}
            description={t("llm.mmproj.autoloadDesc")}
            ontoggle={onToggleAutoloadMmproj}
            showBadge={false}
            class="border-t border-border/40 dark:border-white/[0.04]"
          />

          {#if hasDownloadedMmproj}
            <ToggleRow
              checked={config.no_mmproj_gpu}
              label={t("llm.mmproj.noGpu")}
              description={t("llm.mmproj.noGpuDesc")}
              ontoggle={onToggleNoMmprojGpu}
              showBadge={false}
            />
          {/if}
        {/if}

        <div class="flex flex-col gap-2 px-4 py-3.5 border-t border-border/40 dark:border-white/[0.04]">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.gpuMemThreshold")}</span>
            <span class="text-ui-base text-muted-foreground leading-relaxed">{t("llm.inference.gpuMemThresholdDesc")}</span>
          </div>
          <div class="flex items-center gap-3">
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-ui-sm text-muted-foreground">{t("llm.inference.gpuMemDecode")}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each [0, 0.25, 0.5, 0.75, 1.0] as val}
                  <button onclick={() => onSetGpuMemoryThreshold(val)}
                    class="rounded-lg border px-2 py-1 text-ui-sm font-semibold transition-all cursor-pointer
                         {config.gpu_memory_threshold === val
                           ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                           : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {val === 0 ? "Off" : `${val} GB`}
                  </button>
                {/each}
              </div>
            </div>
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-ui-sm text-muted-foreground">{t("llm.inference.gpuMemGen")}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each [0, 0.15, 0.3, 0.5, 0.75] as val}
                  <button onclick={() => onSetGpuMemoryGenThreshold(val)}
                    class="rounded-lg border px-2 py-1 text-ui-sm font-semibold transition-all cursor-pointer
                         {config.gpu_memory_gen_threshold === val
                           ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                           : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {val === 0 ? "Off" : `${val} GB`}
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </div>

        <!-- TurboQuant KV-cache types ──────────────────────────────────────── -->
        <div class="flex flex-col gap-2 px-4 py-3.5 border-t border-border/40 dark:border-white/[0.04]">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.kvCacheType")}</span>
            <span class="text-ui-base text-muted-foreground leading-relaxed">{t("llm.inference.kvCacheTypeDesc")}</span>
          </div>
          <div class="flex items-start gap-4">
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-ui-sm text-muted-foreground">{t("llm.inference.kvCacheK")}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each KV_TYPES as { tag, label }}
                  <button onclick={() => onSetCacheTypeK(tag)}
                    class="rounded-lg border px-2 py-1 text-ui-sm font-semibold transition-all cursor-pointer
                         {config.cache_type_k === tag
                           ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                           : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {label}
                  </button>
                {/each}
              </div>
            </div>
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-ui-sm text-muted-foreground">{t("llm.inference.kvCacheV")}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each KV_TYPES as { tag, label }}
                  <button onclick={() => onSetCacheTypeV(tag)}
                    class="rounded-lg border px-2 py-1 text-ui-sm font-semibold transition-all cursor-pointer
                         {config.cache_type_v === tag
                           ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                           : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {label}
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </div>

        <!-- TurboQuant attention rotation toggle ────────────────────────────── -->
        <ToggleRow
          checked={!config.attn_rot_disabled}
          label={t("llm.inference.attnRot")}
          description={t("llm.inference.attnRotDesc")}
          ontoggle={onToggleAttnRotDisabled}
          showBadge={false}
          class="border-t border-border/40 dark:border-white/[0.04]"
        />

        <!-- Prefill batch sizes ─────────────────────────────────────── -->
        <div class="flex flex-col gap-2 px-4 py-3.5 border-t border-border/40 dark:border-white/[0.04]">
          <div class="flex flex-col gap-0.5">
            <span class="text-ui-lg font-semibold text-foreground">{t("llm.inference.prefill")}</span>
            <span class="text-ui-base text-muted-foreground leading-relaxed">{t("llm.inference.prefillDesc")}</span>
          </div>
          <div class="flex items-start gap-4">
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-ui-sm text-muted-foreground">{t("llm.inference.nBatch")}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each [[null, "auto"], [512, "512"], [1024, "1K"], [2048, "2K"], [4096, "4K"], [8192, "8K"]] as [val, label]}
                  <button onclick={() => onSetNBatch(val as number | null)}
                    class="rounded-lg border px-2 py-1 text-ui-sm font-semibold transition-all cursor-pointer
                         {config.n_batch === val
                           ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                           : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {label}
                  </button>
                {/each}
              </div>
            </div>
            <div class="flex flex-col gap-1 flex-1">
              <span class="text-ui-sm text-muted-foreground">{t("llm.inference.nUbatch")}</span>
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each [[null, "auto"], [128, "128"], [256, "256"], [512, "512"], [1024, "1K"], [2048, "2K"]] as [val, label]}
                  <button onclick={() => onSetNUbatch(val as number | null)}
                    class="rounded-lg border px-2 py-1 text-ui-sm font-semibold transition-all cursor-pointer
                         {config.n_ubatch === val
                           ? 'border-violet-500/50 bg-violet-500/10 text-violet-600 dark:text-violet-400'
                           : 'border-border bg-muted text-muted-foreground hover:text-foreground'}">
                    {label}
                  </button>
                {/each}
              </div>
            </div>
          </div>
        </div>

        <!-- Flash attention + Offload KQV ──────────────────────────────── -->
        <ToggleRow
          checked={config.flash_attention}
          label={t("llm.inference.flashAttn")}
          description={t("llm.inference.flashAttnDesc")}
          ontoggle={onToggleFlashAttention}
          showBadge={false}
          class="border-t border-border/40 dark:border-white/[0.04]"
        />

        <ToggleRow
          checked={config.offload_kqv}
          label={t("llm.inference.offloadKqv")}
          description={t("llm.inference.offloadKqvDesc")}
          ontoggle={onToggleOffloadKqv}
          showBadge={false}
          class="border-t border-border/40 dark:border-white/[0.04]"
        />

        <div class="flex flex-col gap-1.5 px-4 py-3 bg-surface-3">
          <div class="flex items-center justify-between">
            <SectionHeader>Quick test</SectionHeader>
            <button
              onclick={async (e) => {
                await navigator.clipboard.writeText(curlSnippet);
                const btn = e.currentTarget as HTMLButtonElement;
                const prev = btn.textContent;
                btn.textContent = "Copied!";
                setTimeout(() => {
                  btn.textContent = prev ?? "Copy";
                }, 1500);
              }}
              class="text-ui-xs text-muted-foreground/60 hover:text-foreground transition-colors cursor-pointer select-none">
              Copy
            </button>
          </div>
          <pre class="text-ui-sm font-mono text-muted-foreground/80 whitespace-pre-wrap break-all leading-relaxed select-text cursor-text">{curlSnippet}</pre>
        </div>
      </CardContent>
    </Card>
  {/if}
</section>
