<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com -->
<script lang="ts">
import { marked } from "marked";
import { Card, CardContent } from "$lib/components/ui/card";
import { SectionHeader } from "$lib/components/ui/section-header";
import { t } from "$lib/i18n/index.svelte";

interface SkillInfo {
  name: string;
  description: string;
  source: string;
  enabled: boolean;
}

interface Props {
  skills: SkillInfo[];
  skillsLoading: boolean;
  skillsLicense: string;
  onToggleSkill: (name: string, enabled: boolean) => void | Promise<void>;
  onSetAllSkills: (enabled: boolean) => void | Promise<void>;
}

let { skills, skillsLoading, skillsLicense, onToggleSkill, onSetAllSkills }: Props = $props();

let hoveredSkill = $state<string | null>(null);
let skillsLicenseOpen = $state(false);

function inlineMd(src: string): string {
  return marked.parseInline(src, { gfm: true }) as string;
}
</script>

<section class="flex flex-col gap-2">
  <div class="flex items-center gap-2 px-0.5">
    <SectionHeader>{t("llm.tools.skillsSection")}</SectionHeader>
    <span class="text-ui-xs text-muted-foreground/50">
      {skills.filter((s) => s.enabled).length}/{skills.length}
    </span>
  </div>

  <Card class="border-border dark:border-white/[0.06] bg-surface-1 gap-0 py-0 overflow-hidden">
    <CardContent class="flex flex-col py-0 px-0">
      <div class="flex flex-col gap-1 px-4 pt-3.5 pb-2">
        <div class="flex items-center justify-between gap-4">
          <div class="flex flex-col gap-1">
            <p class="text-ui-base text-muted-foreground leading-relaxed">
              {t("llm.tools.skillsSectionDesc")}
            </p>
            {#if skillsLicense}
              <button
                onclick={() => (skillsLicenseOpen = !skillsLicenseOpen)}
                class="flex items-center gap-1 text-ui-sm font-semibold text-violet-600 dark:text-violet-400
                       hover:text-violet-600/80 transition-colors cursor-pointer self-start">
                <svg class="w-3 h-3 transition-transform {skillsLicenseOpen ? 'rotate-90' : ''}"
                     viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5"
                     stroke-linecap="round" stroke-linejoin="round">
                  <polyline points="9 18 15 12 9 6"/>
                </svg>
                <span class="text-violet-600 dark:text-violet-400 font-bold">AI100</span> {t("llm.tools.skillsLicenseLabel")}
              </button>
            {/if}
          </div>
          {#if skills.length > 0}
            <div class="flex items-center gap-1 shrink-0">
              <button
                onclick={() => onSetAllSkills(true)}
                class="rounded-md border border-border px-2 py-0.5 text-ui-xs font-semibold
                     text-muted-foreground hover:text-foreground transition-colors cursor-pointer bg-background">
                {t("llm.tools.skillsEnableAll")}
              </button>
              <button
                onclick={() => onSetAllSkills(false)}
                class="rounded-md border border-border px-2 py-0.5 text-ui-xs font-semibold
                     text-muted-foreground hover:text-foreground transition-colors cursor-pointer bg-background">
                {t("llm.tools.skillsDisableAll")}
              </button>
            </div>
          {/if}
        </div>

        {#if skillsLicenseOpen && skillsLicense}
          <div class="mt-1 rounded-lg border border-violet-500/20 bg-violet-500/[0.04] px-3 py-2.5
                      max-h-48 overflow-y-auto">
            <pre class="text-ui-xs leading-relaxed text-muted-foreground whitespace-pre-wrap font-sans">{@html skillsLicense.replace(/AI100/g, '<span class="text-violet-600 dark:text-violet-400 font-semibold">AI100</span>')}</pre>
          </div>
        {/if}
      </div>

      <div class="flex flex-col gap-2 px-4 pb-3">
        {#if skillsLoading}
          <p class="text-ui-sm text-muted-foreground py-2">{t("llm.tools.skillsLoading")}</p>
        {:else if skills.length === 0}
          <p class="text-ui-sm text-muted-foreground py-2">{t("llm.tools.skillsNone")}</p>
        {:else}
          {#each skills as skill}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div class="relative rounded-xl border
                        border-border/60 dark:border-white/[0.06]
                        {skill.enabled
                          ? 'bg-surface-3'
                          : 'bg-surface-3/50 opacity-60'}"
                 onmouseenter={() => (hoveredSkill = skill.name)}
                 onmouseleave={() => (hoveredSkill = null)}>
              <div class="flex items-center justify-between gap-3 px-3 py-2.5">
                <div class="flex flex-col gap-0.5 min-w-0">
                  <div class="flex items-center gap-1.5">
                    <span class="text-ui-md font-semibold text-foreground truncate">{skill.name}</span>
                    <span class="text-ui-2xs font-medium rounded-full border px-1.5 py-0
                                 border-border/50 text-muted-foreground/60 shrink-0">
                      {skill.source}
                    </span>
                  </div>
                  <span class="text-ui-sm text-muted-foreground leading-relaxed skill-desc
                               {hoveredSkill === skill.name ? '' : 'line-clamp-2'}">{@html inlineMd(skill.description)}</span>
                </div>
                <button role="switch" aria-checked={skill.enabled} aria-label={skill.name}
                  onclick={() => onToggleSkill(skill.name, !skill.enabled)}
                  class="relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2
                         border-transparent transition-colors duration-200 mt-0.5
                         {skill.enabled ? 'bg-violet-500' : 'bg-muted dark:bg-white/10'}">
                  <span class="pointer-events-none inline-block h-4 w-4 rounded-full bg-white shadow-md
                                transform transition-transform duration-200
                                {skill.enabled ? 'translate-x-4' : 'translate-x-0'}"></span>
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </CardContent>
  </Card>
</section>

<style>
  :global(.skill-desc code) {
    font-size: 0.58rem;
    padding: 0.05rem 0.3rem;
    border-radius: 0.25rem;
    background: var(--color-muted, oklch(0.96 0 0));
  }
  :global(.skill-desc a) {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  :global(.skill-desc strong) {
    font-weight: 600;
    color: var(--color-foreground);
  }
</style>
