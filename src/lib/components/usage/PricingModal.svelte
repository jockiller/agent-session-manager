<script lang="ts">
  import { Plus, Search, Trash2, X } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { usageStore } from "$lib/usage/store.svelte";
  import MatchModelModal from "../config/MatchModelModal.svelte";
  import type { ModelEntry } from "$lib/config/types";
  import { btnBorder, btnGhost, btnPrimary, fieldClass, selectClass } from "../config/styles";

  let {
    isOpen = false,
    seedModel = "",
    onClose,
  }: {
    isOpen: boolean;
    seedModel?: string;
    onClose: () => void;
  } = $props();

  const CURRENCIES = ["USD", "CNY", "EUR", "JPY"];
  const DEFAULT_RATES: Record<string, number> = { USD: 1, CNY: 7.25, EUR: 0.92, JPY: 152 };

  let currencyCode = $state("USD");
  let exchangeRate = $state("1");
  let modelId = $state("");
  let inputRate = $state("0");
  let outputRate = $state("0");
  let cacheReadRate = $state("0");
  let cacheWriteRate = $state("0");
  let showMatchModal = $state(false);
  let saving = $state(false);

  $effect(() => {
    if (isOpen) {
      if (seedModel) modelId = seedModel;
      void usageStore.loadPricingOverrides().then(() => {
        currencyCode = usageStore.currencyPref.currency;
        exchangeRate = String(usageStore.currencyPref.rate);
      });
    }
  });

  function onCurrencyChange(code: string) {
    currencyCode = code;
    exchangeRate = String(DEFAULT_RATES[code] ?? 1);
  }

  async function saveCurrency() {
    const r = Number(exchangeRate);
    await usageStore.setCurrency(currencyCode, Number.isFinite(r) && r > 0 ? r : undefined);
  }

  async function saveOverride() {
    if (!modelId.trim() || saving) return;
    saving = true;
    try {
      await usageStore.savePricingOverride(modelId.trim(), {
        input: Number(inputRate) || 0,
        output: Number(outputRate) || 0,
        cacheRead: Number(cacheReadRate) || 0,
        cacheWrite: Number(cacheWriteRate) || 0,
      });
      modelId = "";
    } finally {
      saving = false;
    }
  }

  function handlePickedCatalog(hit: ModelEntry) {
    showMatchModal = false;
    if (!modelId.trim()) modelId = hit.id;
    inputRate = String(hit.cost?.input ?? 0);
    outputRate = String(hit.cost?.output ?? 0);
    cacheReadRate = String(hit.cost?.cacheRead ?? 0);
    cacheWriteRate = String(hit.cost?.cacheWrite ?? 0);
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    onclick={(e) => {
      if (e.target === e.currentTarget) onClose();
    }}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-2xl max-h-[82vh] flex flex-col overflow-hidden">
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
        <div>
          <h2 class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("usage_pricing_title")}</h2>
          <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">{t("usage_pricing_desc")}</p>
        </div>
        <button type="button" onclick={onClose} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer">
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="flex-1 overflow-y-auto p-5 space-y-5">
        <!-- Currency -->
        <div class="p-3.5 rounded-xl bg-slate-50 dark:bg-slate-800/60 border border-slate-200/80 dark:border-slate-700 flex flex-wrap items-end gap-3">
          <label class="flex flex-col gap-1 w-32">
            <span class="text-[11px] text-slate-500 dark:text-slate-400">{t("usage_currency")}</span>
            <select class={selectClass} value={currencyCode} onchange={(e) => onCurrencyChange(e.currentTarget.value)}>
              {#each CURRENCIES as c}
                <option value={c}>{c}</option>
              {/each}
            </select>
          </label>
          <label class="flex flex-col gap-1 w-36">
            <span class="text-[11px] text-slate-500 dark:text-slate-400">Rate (per 1 USD)</span>
            <input class="{fieldClass} font-mono" bind:value={exchangeRate} inputmode="decimal" />
          </label>
          <button type="button" class={btnPrimary} onclick={() => { void saveCurrency(); }}>
            {t("cfg_save")}
          </button>
        </div>

        <!-- Add / Edit Override -->
        <div class="p-3.5 rounded-xl bg-slate-50 dark:bg-slate-800/60 border border-slate-200/80 dark:border-slate-700 space-y-3">
          <div class="flex items-center justify-between">
            <span class="text-xs font-semibold text-slate-800 dark:text-slate-100">{t("usage_add_override")}</span>
            <button type="button" class={btnBorder} onclick={() => (showMatchModal = true)}>
              <Search class="h-3 w-3" />
              {t("usage_match_catalog")}
            </button>
          </div>
          <div class="grid grid-cols-2 sm:grid-cols-5 gap-2">
            <label class="flex flex-col gap-1 col-span-2 sm:col-span-5">
              <span class="text-[10px] text-slate-500">{t("cfg_model_id")}</span>
              <input class="{fieldClass} font-mono" bind:value={modelId} placeholder="claude-sonnet-4-5" />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-[10px] text-slate-500">{t("cfg_cost_input")}</span>
              <input class="{fieldClass} font-mono" bind:value={inputRate} inputmode="decimal" />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-[10px] text-slate-500">{t("cfg_cost_output")}</span>
              <input class="{fieldClass} font-mono" bind:value={outputRate} inputmode="decimal" />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-[10px] text-slate-500">{t("cfg_cost_cache_read")}</span>
              <input class="{fieldClass} font-mono" bind:value={cacheReadRate} inputmode="decimal" />
            </label>
            <label class="flex flex-col gap-1">
              <span class="text-[10px] text-slate-500">{t("cfg_cost_cache_write")}</span>
              <input class="{fieldClass} font-mono" bind:value={cacheWriteRate} inputmode="decimal" />
            </label>
            <div class="flex items-end">
              <button type="button" class="{btnPrimary} w-full justify-center" disabled={!modelId.trim() || saving} onclick={saveOverride}>
                <Plus class="h-3.5 w-3.5" />
                {t("cfg_save")}
              </button>
            </div>
          </div>
        </div>

        <!-- Existing Overrides -->
        {#if usageStore.overrides.length > 0}
          <div class="space-y-1.5">
            {#each usageStore.overrides as row (row.modelNorm)}
              <div class="flex items-center justify-between gap-2 px-3 py-2 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 text-xs">
                <div class="min-w-0">
                  <div class="font-mono font-semibold text-slate-800 dark:text-slate-100 truncate">{row.modelNorm}</div>
                  <div class="text-[10px] font-mono text-slate-400">
                    In ${row.input} · Out ${row.output} · Cache R ${row.cacheRead} · W ${row.cacheWrite}
                  </div>
                </div>
                <div class="flex items-center gap-1 shrink-0">
                  <button
                    type="button"
                    class={btnGhost}
                    onclick={() => {
                      modelId = row.modelNorm;
                      inputRate = String(row.input);
                      outputRate = String(row.output);
                      cacheReadRate = String(row.cacheRead);
                      cacheWriteRate = String(row.cacheWrite);
                    }}
                  >
                    {t("inst_editor_edit")}
                  </button>
                  <button
                    type="button"
                    class="p-1 text-slate-400 hover:text-rose-600 cursor-pointer"
                    onclick={() => { void usageStore.deletePricingOverride(row.modelNorm); }}
                  >
                    <Trash2 class="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

<MatchModelModal
  isOpen={showMatchModal}
  targetID={modelId}
  onClose={() => (showMatchModal = false)}
  onPick={handlePickedCatalog}
/>
