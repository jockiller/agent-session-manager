<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

<div class="bg-white border border-slate-200/90 rounded-xl p-3.5 shadow-2xs space-y-2.5">
  {@render children()}
</div>
