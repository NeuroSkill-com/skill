<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

Inline confirm/cancel action row.
Replaces the repeated confirm-delete pattern used in history and labels pages.

Usage:
  <ConfirmAction
    message={t("history.confirmDelete")}
    confirmLabel={t("history.yesDelete")}
    cancelLabel={t("common.cancel")}
    disabled={false}
    onconfirm={() => deleteSession(id)}
    oncancel={() => confirmDelete = null}
  />
-->
<script lang="ts">
  import { Button } from "$lib/components/ui/button";

  interface Props {
    /** Confirmation prompt text. */
    message: string;
    /** Label for the confirm (destructive) button. */
    confirmLabel: string;
    /** Label for the cancel button. */
    cancelLabel: string;
    /** Whether the confirm button is disabled (e.g. while deleting). */
    disabled?: boolean;
    /** Called when the user confirms the action. */
    onconfirm: (e: MouseEvent) => void;
    /** Called when the user cancels. */
    oncancel: (e: MouseEvent) => void;
  }

  let {
    message,
    confirmLabel,
    cancelLabel,
    disabled = false,
    onconfirm,
    oncancel,
  }: Props = $props();
</script>

<div class="flex items-center gap-2">
  <span class="text-[0.68rem] text-red-600 dark:text-red-400 font-medium flex-1">
    {message}
  </span>
  <Button size="sm" variant="destructive" class="text-[0.62rem] h-6 px-2"
          {disabled}
          onclick={(e: MouseEvent) => { e.stopPropagation(); onconfirm(e); }}>
    {disabled ? "…" : confirmLabel}
  </Button>
  <Button size="sm" variant="ghost" class="text-[0.62rem] h-6 px-2 text-muted-foreground"
          onclick={(e: MouseEvent) => { e.stopPropagation(); oncancel(e); }}>
    {cancelLabel}
  </Button>
</div>
