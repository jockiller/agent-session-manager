<script lang="ts">
  let { title, accessory = "" }: { title: string; accessory?: string } = $props();
</script>

<div class="flex items-baseline gap-2">
  <span class="text-xs font-semibold text-slate-800">{title}</span>
  {#if accessory}
    <span class="text-[11px] text-slate-400">{accessory}</span>
  {/if}
</div>
