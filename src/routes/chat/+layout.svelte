<!-- Chat window layout — inherits root layout (theme, locale, toasts).
     This file exists so SvelteKit treats /chat as its own route group. -->
<script lang="ts">
  import type { Snippet } from "svelte";
  let { children }: { children: Snippet } = $props();
</script>

{@render children()}
