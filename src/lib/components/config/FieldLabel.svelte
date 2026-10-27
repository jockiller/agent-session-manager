<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    title,
    class: className = "",
    children,
  }: {
    title: string;
    class?: string;
    children: Snippet;
  } = $props();
</script>

<label class="flex flex-col gap-1 min-w-0 {className}">
  <span class="text-[11px] text-slate-500">{title}</span>
  {@render children()}
</label>
