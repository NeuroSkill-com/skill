<!-- Chat window layout — inherits root layout (theme, locale, toasts).
     This file exists so SvelteKit treats /chat as its own route group.
     Text selection is enabled here because users need to copy model responses. -->
<script lang="ts">
import type { Snippet } from "svelte";

let { children }: { children: Snippet } = $props();
</script>

<style>
  /* Re-enable text selection for the entire chat window.
     The global app.css sets user-select:none on body; chat is the exception. */
  :global(body) {
    user-select: text;
    -webkit-user-select: text;
  }
</style>

{@render children()}
