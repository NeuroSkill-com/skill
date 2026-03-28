<!-- SPDX-License-Identifier: GPL-3.0-only -->
<!-- Copyright (C) 2026 NeuroSkill.com

HIG-compliant toggle switch.
Uses role="switch" + aria-checked for screen reader accessibility (Apple HIG: Toggles).
-->
<script lang="ts">
interface Props {
  /** Whether the toggle is on. */
  checked: boolean;
  /** Accessible label for the switch. */
  label?: string;
  /** Called when the toggle is clicked. */
  ontoggle: () => void;
  /** Whether the toggle is disabled. */
  disabled?: boolean;
  /** Active color class (default: bg-emerald-500). */
  activeColor?: string;
}

let { checked, label = "", ontoggle, disabled = false, activeColor = "bg-emerald-500" }: Props = $props();
</script>

<button
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label || undefined}
  {disabled}
  onclick={ontoggle}
  class="relative shrink-0 w-8 h-4 rounded-full transition-colors cursor-pointer
         disabled:opacity-50 disabled:cursor-not-allowed
         {checked ? activeColor : 'bg-muted dark:bg-white/[0.08]'}"
>
  <div
    class="absolute top-0.5 h-3 w-3 rounded-full bg-white shadow transition-transform
           {checked ? 'translate-x-4' : 'translate-x-0.5'}"
  ></div>
</button>
