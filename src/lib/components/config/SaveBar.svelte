<script lang="ts">
  import { t } from "$lib/i18n";
  import DirtyBadge from "./DirtyBadge.svelte";
  import { btnGhost, btnPrimary } from "./styles";

  let {
    isDirty,
    hint,
    saveTitle = "",
    onSave,
    onDiscard,
  }: {
    isDirty: boolean;
    hint: string;
    saveTitle?: string;
    onSave: () => void;
    onDiscard: () => void;
  } = $props();
</script>

<div class="shrink-0 flex items-center gap-2.5 px-4 py-2.5 border-t border-slate-200/80 bg-slate-50/80">
  {#if isDirty}
    <DirtyBadge />
  {:else}
    <span class="text-[11px] text-slate-400">{hint}</span>
  {/if}
  <div class="flex-1"></div>
  <button type="button" class={btnGhost} disabled={!isDirty} onclick={onDiscard}>
    {t("cfg_discard")}
  </button>
  <button type="button" class={btnPrimary} disabled={!isDirty} onclick={onSave}>
    {saveTitle || t("cfg_save")}
  </button>
</div>
