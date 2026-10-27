<script lang="ts">
  import { AlertTriangle, CheckCircle2, X } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import type { StoreToast } from "$lib/config/types";

  let { toast, onDismiss, onReveal }: { toast: StoreToast; onDismiss: () => void; onReveal: (path: string) => void } =
    $props();
</script>

<div
  class="pointer-events-auto w-full max-w-[380px] bg-white border border-slate-200/90 rounded-xl shadow-xl px-3.5 py-3 flex items-start gap-2.5"
>
  {#if toast.isError}
    <AlertTriangle class="h-4 w-4 text-rose-500 mt-0.5 shrink-0" />
  {:else}
    <CheckCircle2 class="h-4 w-4 text-emerald-500 mt-0.5 shrink-0" />
  {/if}
  <div class="min-w-0 flex-1">
    <div class="text-xs font-semibold text-slate-800">{toast.title}</div>
    {#if toast.detail}
      <p class="mt-0.5 text-[11px] text-slate-500 whitespace-pre-wrap break-all select-text line-clamp-3">{toast.detail}</p>
    {/if}
    {#if toast.revealedPath}
      <button
        type="button"
        class="mt-1 text-[11px] text-sky-600 hover:text-sky-700 cursor-pointer"
        onclick={() => onReveal(toast.revealedPath)}
      >
        {t("cfg_reveal_in_finder")}
      </button>
    {/if}
  </div>
  <button
    type="button"
    class="p-0.5 rounded text-slate-400 hover:text-slate-700 hover:bg-slate-100 cursor-pointer"
    title={t("close")}
    onclick={onDismiss}
  >
    <X class="h-3.5 w-3.5" />
  </button>
</div>
