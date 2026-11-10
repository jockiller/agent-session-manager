<script lang="ts">
  import { LoaderCircle, Search, X } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { configStore as store } from "$lib/config/store.svelte";
  import { summaryPills } from "$lib/config/document";
  import type { ModelEntry } from "$lib/config/types";
  import { fieldClass } from "./styles";

  let {
    isOpen = false,
    targetID = "",
    onClose,
    onPick,
  }: {
    isOpen: boolean;
    targetID?: string;
    onClose: () => void;
    onPick: (model: ModelEntry) => void;
  } = $props();

  let query = $state("");
  let results = $state<ModelEntry[]>([]);
  let searching = $state(false);
  let error = $state("");
  let searchInput = $state<HTMLInputElement | null>(null);
  let seq = 0;
  let wasOpen = false;

  $effect(() => {
    if (isOpen && !wasOpen) {
      wasOpen = true;
      const seed = targetID.trim();
      query = seed;
      error = "";
      void runSearch(seed);
      queueMicrotask(() => {
        searchInput?.focus();
        searchInput?.select();
      });
    } else if (!isOpen) {
      wasOpen = false;
      results = [];
    }
  });

  async function runSearch(text: string) {
    const id = ++seq;
    searching = true;
    error = "";
    try {
      const hits = await store.searchModels(text, 50);
      if (id === seq) results = hits;
    } catch (err) {
      if (id === seq) {
        results = [];
        error = err instanceof Error ? err.message : String(err);
      }
    } finally {
      if (id === seq) searching = false;
    }
  }

  function handleInput(value: string) {
    query = value;
    void runSearch(value);
  }

  function handleBackdrop(e: MouseEvent) {
    if (e.target === e.currentTarget) onClose();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }

  function price(model: ModelEntry): string {
    const cost = model.cost;
    if (!cost) return "";
    const parts: string[] = [];
    if (cost.input != null) parts.push(`${t("cfg_cost_input")} $${cost.input}`);
    if (cost.output != null) parts.push(`${t("cfg_cost_output")} $${cost.output}`);
    return parts.join(" · ");
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-50 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center p-4 animate-in fade-in duration-150"
    onclick={handleBackdrop}
    role="dialog"
    aria-modal="true"
    aria-labelledby="match-model-title"
    tabindex="-1"
  >
    <div
      class="bg-white rounded-2xl shadow-xl border border-slate-200/90 w-full max-w-lg max-h-[80vh] flex flex-col overflow-hidden animate-in zoom-in-95 duration-150"
      role="document"
    >
      <div class="flex items-start gap-3 px-4 pt-3.5 pb-2">
        <div class="flex-1 min-w-0">
          <h2 id="match-model-title" class="text-sm font-bold text-slate-900">{t("cfg_match_title")}</h2>
          <p class="mt-0.5 text-[11px] text-slate-400 truncate">{t("cfg_match_target", { id: targetID })}</p>
        </div>
        <button
          type="button"
          class="p-1 rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition cursor-pointer"
          title={t("close")}
          onclick={onClose}
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="px-4 pb-3">
        <div class="relative">
          <Search class="absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-slate-400" />
          <input
            bind:this={searchInput}
            class="{fieldClass} pl-8"
            placeholder={t("cfg_match_search_placeholder")}
            value={query}
            oninput={(e) => handleInput(e.currentTarget.value)}
          />
        </div>
      </div>

      <div class="flex-1 min-h-0 overflow-y-auto border-t border-slate-100">
        {#if searching}
          <div class="flex items-center gap-2 px-4 py-6 text-xs text-slate-400">
            <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
            {t("cfg_match_loading")}
          </div>
        {:else if error}
          <p class="px-4 py-6 text-xs text-rose-600">{error}</p>
        {:else if results.length === 0}
          <p class="px-4 py-6 text-xs text-slate-400">{t("cfg_match_empty")}</p>
        {:else}
          <div class="p-1.5 space-y-0.5">
            {#each results as hit (hit.id)}
              <button
                type="button"
                class="w-full text-left px-2.5 py-2 rounded-lg hover:bg-slate-50 transition cursor-pointer"
                onclick={() => onPick(hit)}
              >
                <span class="flex items-center gap-2 min-w-0">
                  <span class="text-xs font-medium font-mono text-slate-800 truncate">{hit.id}</span>
                  {#if hit.type === "image"}
                    <span class="shrink-0 rounded-full bg-violet-50 px-1.5 py-px text-[10px] font-medium text-violet-700">
                      {t("cfg_model_type_image")}
                    </span>
                  {/if}
                </span>
                <span class="mt-0.5 flex items-center gap-2 text-[10px] text-slate-400 min-w-0">
                  <span class="truncate">{summaryPills(hit).join(" · ")}</span>
                  {#if price(hit)}
                    <span class="shrink-0 text-slate-500">{price(hit)}</span>
                  {/if}
                </span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
