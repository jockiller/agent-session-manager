<script lang="ts">
  import { onMount } from "svelte";
  import { RefreshCw, RotateCcw, ScrollText, SlidersHorizontal } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { appStore } from "$lib/stores/app.svelte";
  import { PLATFORMS } from "$lib/platforms/registry";
  import { usageStore } from "$lib/usage/store.svelte";
  import type { UsageRangePreset } from "$lib/usage/types";
  import UsageLogTable from "./UsageLogTable.svelte";
  import PricingModal from "./PricingModal.svelte";
  import { btnBorder, btnPrimary, selectClass } from "../config/styles";

  let showPricingModal = $state(false);
  let pricingSeedModel = $state("");

  let data = $derived(usageStore.dashboard);
  let currency = $derived(usageStore.logs?.currency ?? data?.currency ?? usageStore.currencyPref.currency);
  let total = $derived(usageStore.logs?.total ?? 0);

  function refreshCurrentLogs(page = 1) {
    void usageStore.loadLogs(page);
  }

  function selectPreset(preset: UsageRangePreset) {
    usageStore.rangePreset = preset;
    refreshCurrentLogs(1);
  }

  function openPricingFor(model = "") {
    pricingSeedModel = model;
    showPricingModal = true;
  }

  onMount(() => {
    void usageStore.loadLogs(1);
    if (!usageStore.dashboard) void usageStore.loadDashboard();
    if (!usageStore.syncStatus) void usageStore.syncInBackground();
  });
</script>

<div class="flex-1 min-h-0 flex flex-col bg-slate-50 dark:bg-slate-950 overflow-hidden">
  <div class="shrink-0 px-5 py-3 border-b border-slate-200/80 dark:border-slate-800 bg-white/80 dark:bg-slate-900/80 space-y-2.5">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <ScrollText class="h-4 w-4 text-claude-600 dark:text-claude-400" />
          <h1 class="text-base font-bold text-slate-900 dark:text-slate-100">{t("usage_logs_title")}</h1>
          {#if total > 0}
            <span class="px-1.5 py-0.5 rounded-md text-[10px] font-mono font-semibold bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 border border-slate-200/80 dark:border-slate-700">
              {total}
            </span>
          {/if}
        </div>
        <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">{t("usage_logs_subtitle")}</p>
      </div>

      <div class="flex items-center gap-2">
        <button type="button" class={btnBorder} onclick={() => openPricingFor("")}>
          <SlidersHorizontal class="h-3.5 w-3.5" />
          <span>{t("usage_pricing_btn")} ({currency})</span>
        </button>
        <button type="button" class={btnPrimary} disabled={usageStore.syncing} onclick={() => { void usageStore.sync(false); }}>
          <RefreshCw class="h-3.5 w-3.5 {usageStore.syncing ? 'animate-spin' : ''}" />
          <span>{usageStore.syncing ? t("usage_syncing") : t("usage_sync")}</span>
        </button>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2">
      <div class="flex items-center rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800 p-0.5">
        {#each [
          { id: "today", label: t("usage_range_today") },
          { id: "7d", label: t("usage_range_7d") },
          { id: "30d", label: t("usage_range_30d") },
          { id: "90d", label: t("usage_range_90d") },
          { id: "all", label: t("usage_range_all") },
        ] as opt (opt.id)}
          <button
            type="button"
            onclick={() => selectPreset(opt.id as UsageRangePreset)}
            class="px-2.5 py-1 rounded-md text-[11px] font-medium transition cursor-pointer {usageStore.rangePreset === opt.id
              ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
              : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
          >
            {opt.label}
          </button>
        {/each}
      </div>

      <select
        class="{selectClass} !w-36 !py-1"
        value={usageStore.platform}
        onchange={(e) => { usageStore.platform = e.currentTarget.value; refreshCurrentLogs(1); }}
      >
        <option value="all">{t("all_platforms")}</option>
        {#each PLATFORMS.filter((p) => p.hasUsage) as p (p.id)}
          <option value={p.id}>{p.label}</option>
        {/each}
      </select>

      <select
        class="{selectClass} !w-44 !py-1 font-mono"
        value={usageStore.model}
        onchange={(e) => { usageStore.model = e.currentTarget.value; refreshCurrentLogs(1); }}
      >
        <option value="all">{t("usage_all_models")}</option>
        {#each data?.availableModels ?? [] as m}
          <option value={m}>{m}</option>
        {/each}
      </select>

      <select
        class="{selectClass} !w-44 !py-1"
        value={usageStore.workspace}
        onchange={(e) => { usageStore.workspace = e.currentTarget.value; refreshCurrentLogs(1); }}
      >
        <option value="all">{t("all_workspaces", { n: data?.availableWorkspaces.length ?? 0 })}</option>
        {#each data?.availableWorkspaces ?? [] as ws}
          <option value={ws}>{ws.split("/").pop() || ws}</option>
        {/each}
      </select>

      {#if usageStore.platform !== "all" || usageStore.model !== "all" || usageStore.workspace !== "all" || usageStore.rangePreset !== "30d"}
        <button type="button" onclick={() => usageStore.resetFilters()} class="inline-flex items-center gap-1 px-2 py-1 text-[11px] text-slate-500 hover:text-rose-600 cursor-pointer">
          <RotateCcw class="h-3 w-3" />
          <span>{t("reset")}</span>
        </button>
      {/if}
    </div>
  </div>

  <div class="flex-1 min-h-0 flex flex-col p-4 overflow-hidden gap-3">
    {#if usageStore.error}
      <div class="shrink-0 p-3 rounded-xl bg-rose-50 dark:bg-rose-950/50 border border-rose-200 dark:border-rose-800 text-xs text-rose-700 dark:text-rose-300">
        {usageStore.error}
      </div>
    {/if}

    <div class="flex-1 min-h-0 flex flex-col bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl shadow-2xs overflow-hidden">
      <UsageLogTable
        logs={usageStore.logs}
        loading={usageStore.logsLoading}
        fillHeight={true}
        onPageChange={(p) => { void usageStore.loadLogs(p); }}
        onPageSizeChange={(ps) => {
          usageStore.logsPageSize = ps;
          void usageStore.loadLogs(1);
        }}
        onJumpSession={(sid) => appStore.jumpToSession(sid)}
        onConfigurePrice={(m) => openPricingFor(m)}
      />
    </div>
  </div>
</div>

<PricingModal
  isOpen={showPricingModal}
  seedModel={pricingSeedModel}
  onClose={() => (showPricingModal = false)}
/>