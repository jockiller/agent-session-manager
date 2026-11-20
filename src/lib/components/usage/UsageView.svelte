<script lang="ts">
  import { onMount } from "svelte";
  import {
    AlertCircle,
    BarChart3,
    Coins,
    ExternalLink,
    RefreshCw,
    RotateCcw,
    SlidersHorizontal,
    Sparkles,
    Zap,
  } from "@lucide/svelte";
  import type { EChartsCoreOption } from "echarts/core";
  import { t } from "$lib/i18n";
  import { appStore } from "$lib/stores/app.svelte";
  import { PLATFORMS, getPlatformLabel } from "$lib/platforms/registry";
  import { formatCompactTokens, formatMoney, usageStore } from "$lib/usage/store.svelte";
  import type { UsageRangePreset } from "$lib/usage/types";
  import { formatTimeAgo, getPlatformBadge } from "$lib/utils";
  import UsageChart from "./UsageChart.svelte";
  import PricingModal from "./PricingModal.svelte";
  import { btnBorder, btnPrimary, selectClass } from "../config/styles";

  let chartMode = $state<"cost" | "tokens">("cost");
  let breakdownTab = $state<"models" | "platforms" | "workspaces" | "sessions">("models");
  let showPricingModal = $state(false);
  let pricingSeedModel = $state("");

  let data = $derived(usageStore.dashboard);
  let currency = $derived(data?.currency ?? usageStore.currencyPref.currency);

  let trendOption = $derived.by((): EChartsCoreOption => {
    const dark = appStore.isDarkEffective;
    const textColor = dark ? "#9B968B" : "#716D64";
    const splitColor = dark ? "rgba(61,59,54,0.6)" : "rgba(229,226,217,0.85)";
    const series = data?.timeseries ?? [];
    const labels = series.map((p) => (data?.hourly ? p.bucket.slice(11) : p.bucket.slice(5)));

    if (chartMode === "cost") {
      return {
        backgroundColor: "transparent",
        tooltip: { trigger: "axis" },
        grid: { left: 44, right: 12, top: 12, bottom: 20 },
        xAxis: {
          type: "category",
          data: labels,
          axisLabel: { color: textColor, fontSize: 10 },
          axisLine: { lineStyle: { color: splitColor } },
        },
        yAxis: {
          type: "value",
          axisLabel: { color: textColor, fontSize: 10 },
          splitLine: { lineStyle: { color: splitColor } },
        },
        series: [
          {
            name: t("usage_total_cost"),
            type: "line",
            smooth: true,
            data: series.map((p) => Number(p.cost.toFixed(4))),
            itemStyle: { color: "#D97757" },
            areaStyle: { color: "rgba(217,119,87,0.18)" },
          },
        ],
      };
    }

    return {
      backgroundColor: "transparent",
      tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
      legend: { top: 0, textStyle: { color: textColor, fontSize: 10 } },
      grid: { left: 48, right: 12, top: 24, bottom: 20 },
      xAxis: {
        type: "category",
        data: labels,
        axisLabel: { color: textColor, fontSize: 10 },
        axisLine: { lineStyle: { color: splitColor } },
      },
      yAxis: {
        type: "value",
        axisLabel: {
          color: textColor,
          fontSize: 10,
          formatter: (val: number) => formatCompactTokens(val),
        },
        splitLine: { lineStyle: { color: splitColor } },
      },
      series: [
        { name: "Input", type: "bar", stack: "tok", data: series.map((p) => p.inputTokens), itemStyle: { color: "#D97757" } },
        { name: "Output", type: "bar", stack: "tok", data: series.map((p) => p.outputTokens), itemStyle: { color: "#7C6F9F" } },
        { name: "Cache Read", type: "bar", stack: "tok", data: series.map((p) => p.cacheReadTokens), itemStyle: { color: "#3A9D78" } },
        { name: "Cache Write", type: "bar", stack: "tok", data: series.map((p) => p.cacheWriteTokens), itemStyle: { color: "#E0A458" } },
      ],
    };
  });

  let platformChartOption = $derived.by((): EChartsCoreOption => {
    const dark = appStore.isDarkEffective;
    const textColor = dark ? "#9B968B" : "#716D64";
    const splitColor = dark ? "rgba(61,59,54,0.6)" : "rgba(229,226,217,0.85)";
    const rows = (data?.byPlatform ?? []).slice(0, 8).reverse();
    const useCost = rows.some((r) => r.cost > 0);
    return {
      backgroundColor: "transparent",
      tooltip: { trigger: "axis", axisPointer: { type: "shadow" } },
      grid: { left: 78, right: 14, top: 8, bottom: 20 },
      xAxis: {
        type: "value",
        axisLabel: {
          color: textColor,
          fontSize: 10,
          formatter: (val: number) => (useCost ? String(val) : formatCompactTokens(val)),
        },
        splitLine: { lineStyle: { color: splitColor } },
      },
      yAxis: {
        type: "category",
        data: rows.map((r) => getPlatformLabel(r.key)),
        axisLabel: { color: textColor, fontSize: 10 },
      },
      series: [
        {
          type: "bar",
          data: rows.map((r) => (useCost ? Number(r.cost.toFixed(4)) : r.totalTokens)),
          itemStyle: { color: "#D97757", borderRadius: [0, 4, 4, 0] },
        },
      ],
    };
  });

  function selectPreset(preset: UsageRangePreset) {
    usageStore.rangePreset = preset;
    void usageStore.loadDashboard();
  }

  function openPricingFor(model = "") {
    pricingSeedModel = model;
    showPricingModal = true;
  }

  onMount(() => {
    void usageStore.loadDashboard();
    if (!usageStore.syncStatus) {
      void usageStore.syncInBackground();
    }
  });
</script>

<div class="flex-1 min-h-0 flex flex-col bg-slate-50 dark:bg-slate-950 overflow-hidden">
  <!-- Header + Filter Controls -->
  <div class="shrink-0 px-5 py-3 border-b border-slate-200/80 dark:border-slate-800 bg-white/80 dark:bg-slate-900/80 space-y-2.5">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <BarChart3 class="h-4 w-4 text-claude-600 dark:text-claude-400" />
          <h1 class="text-base font-bold text-slate-900 dark:text-slate-100">{t("usage_title")}</h1>
        </div>
        <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">{t("usage_subtitle")}</p>
      </div>

      <div class="flex items-center gap-2">
        <button type="button" class={btnBorder} onclick={() => openPricingFor("")}>
          <SlidersHorizontal class="h-3.5 w-3.5" />
          <span>{t("usage_pricing_btn")} ({currency})</span>
        </button>
        <button
          type="button"
          class={btnPrimary}
          disabled={usageStore.syncing}
          onclick={() => { void usageStore.sync(false); }}
        >
          <RefreshCw class="h-3.5 w-3.5 {usageStore.syncing ? 'animate-spin' : ''}" />
          <span>{usageStore.syncing ? t("usage_syncing") : t("usage_sync")}</span>
        </button>
      </div>
    </div>

    <!-- Filters row -->
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
        onchange={(e) => { usageStore.platform = e.currentTarget.value; void usageStore.loadDashboard(); }}
      >
        <option value="all">{t("all_platforms")}</option>
        {#each PLATFORMS.filter((p) => p.hasUsage) as p (p.id)}
          <option value={p.id}>{p.label}</option>
        {/each}
      </select>

      <select
        class="{selectClass} !w-44 !py-1 font-mono"
        value={usageStore.model}
        onchange={(e) => { usageStore.model = e.currentTarget.value; void usageStore.loadDashboard(); }}
      >
        <option value="all">{t("usage_all_models")}</option>
        {#each data?.availableModels ?? [] as m}
          <option value={m}>{m}</option>
        {/each}
      </select>

      <select
        class="{selectClass} !w-44 !py-1"
        value={usageStore.workspace}
        onchange={(e) => { usageStore.workspace = e.currentTarget.value; void usageStore.loadDashboard(); }}
      >
        <option value="all">{t("all_workspaces", { n: data?.availableWorkspaces.length ?? 0 })}</option>
        {#each data?.availableWorkspaces ?? [] as ws}
          <option value={ws}>{ws.split("/").pop() || ws}</option>
        {/each}
      </select>

      {#if usageStore.platform !== "all" || usageStore.model !== "all" || usageStore.workspace !== "all" || usageStore.rangePreset !== "30d"}
        <button
          type="button"
          onclick={() => usageStore.resetFilters()}
          class="inline-flex items-center gap-1 px-2 py-1 text-[11px] text-slate-500 hover:text-rose-600 cursor-pointer"
        >
          <RotateCcw class="h-3 w-3" />
          <span>{t("reset")}</span>
        </button>
      {/if}
    </div>
  </div>

  <div class="flex-1 min-h-0 overflow-y-auto p-4 space-y-3.5">
    {#if usageStore.error}
      <div class="p-3 rounded-xl bg-rose-50 dark:bg-rose-950/50 border border-rose-200 dark:border-rose-800 text-xs text-rose-700 dark:text-rose-300">
        {usageStore.error}
      </div>
    {/if}

    {#if !data || data.kpis.eventCount === 0}
      <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-800 rounded-2xl p-12 text-center space-y-3">
        <BarChart3 class="h-10 w-10 text-slate-300 dark:text-slate-600 mx-auto" />
        <p class="text-xs text-slate-500 dark:text-slate-400">{t("usage_no_data")}</p>
        <button type="button" class="{btnPrimary} mx-auto" onclick={() => { void usageStore.sync(true); }}>
          <RefreshCw class="h-3.5 w-3.5" />
          {t("usage_sync")}
        </button>
      </div>
    {:else}
      {#if data.unpricedModels.length > 0}
        <div class="p-3 rounded-xl bg-amber-50/90 dark:bg-amber-950/40 border border-amber-200 dark:border-amber-800 flex flex-wrap items-center justify-between gap-2 text-xs text-amber-900 dark:text-amber-200">
          <div class="flex items-center gap-2 flex-wrap min-w-0">
            <AlertCircle class="h-4 w-4 text-amber-600 dark:text-amber-400 shrink-0" />
            <span>{t("usage_unpriced_notice", { n: data.unpricedModels.length })}</span>
            {#each data.unpricedModels.slice(0, 6) as um}
              <button
                type="button"
                onclick={() => openPricingFor(um)}
                class="px-1.5 py-0.5 rounded bg-white dark:bg-slate-900 border border-amber-300 dark:border-amber-700 font-mono text-[10px] hover:border-claude-500 cursor-pointer"
              >
                {um}
              </button>
            {/each}
          </div>
          <button type="button" class={btnBorder} onclick={() => openPricingFor(data.unpricedModels[0])}>
            {t("usage_configure_price")}
          </button>
        </div>
      {/if}

      <!-- KPI Cards -->
      <div class="grid grid-cols-2 lg:grid-cols-4 gap-2.5">
        <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-3 shadow-2xs">
          <div class="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
            <span>{t("usage_total_cost")}</span>
            <Coins class="h-3.5 w-3.5 text-claude-600 dark:text-claude-400" />
          </div>
          <div class="text-lg font-bold font-mono text-slate-900 dark:text-slate-100 mt-1">
            {formatMoney(data.kpis.totalCost, currency)}
          </div>
          <div class="text-[10px] text-slate-400 mt-0.5 font-mono">{currency} · {data.kpis.eventCount} events</div>
        </div>

        <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-3 shadow-2xs">
          <div class="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
            <span>{t("usage_total_tokens")}</span>
            <Zap class="h-3.5 w-3.5 text-claude-500 dark:text-claude-400" />
          </div>
          <div class="text-lg font-bold font-mono text-slate-900 dark:text-slate-100 mt-1">
            {formatCompactTokens(data.kpis.totalTokens)}
          </div>
          <div class="text-[10px] text-slate-400 mt-0.5 font-mono truncate">
            In {formatCompactTokens(data.kpis.inputTokens)} · Out {formatCompactTokens(data.kpis.outputTokens)}
          </div>
        </div>

        <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-3 shadow-2xs">
          <div class="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
            <span>{t("usage_cache_hit")}</span>
            <Sparkles class="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <div class="text-lg font-bold font-mono text-emerald-700 dark:text-emerald-400 mt-1">
            {data.kpis.cacheHitRate.toFixed(1)}%
          </div>
          <div class="text-[10px] text-slate-400 mt-0.5 font-mono">
            {t("usage_cache_saved")}: {formatMoney(data.kpis.cacheSavings, currency)}
          </div>
        </div>

        <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-3 shadow-2xs">
          <div class="text-[11px] text-slate-500 dark:text-slate-400">{t("usage_sessions_turns")}</div>
          <div class="text-lg font-bold font-mono text-slate-900 dark:text-slate-100 mt-1">
            {data.kpis.sessionCount}
          </div>
          <div class="text-[10px] text-slate-400 mt-0.5 font-mono">
            Cache R {formatCompactTokens(data.kpis.cacheReadTokens)} · W {formatCompactTokens(data.kpis.cacheWriteTokens)}
          </div>
        </div>
      </div>

      <!-- Charts Grid -->
      <div class="grid grid-cols-1 lg:grid-cols-3 gap-3">
        <div class="lg:col-span-2 bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-3 shadow-2xs space-y-1.5">
          <div class="flex items-center justify-between gap-2">
            <span class="text-xs font-bold text-slate-800 dark:text-slate-100">
              {chartMode === "cost" ? t("usage_trend_cost") : t("usage_trend_tokens")}
            </span>
            <div class="flex items-center rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800 p-0.5">
              <button
                type="button"
                onclick={() => (chartMode = "cost")}
                class="px-2 py-0.5 rounded-md text-[11px] font-medium cursor-pointer {chartMode === 'cost'
                  ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
                  : 'text-slate-500 dark:text-slate-400'}"
              >
                {t("usage_trend_cost")}
              </button>
              <button
                type="button"
                onclick={() => (chartMode = "tokens")}
                class="px-2 py-0.5 rounded-md text-[11px] font-medium cursor-pointer {chartMode === 'tokens'
                  ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
                  : 'text-slate-500 dark:text-slate-400'}"
              >
                {t("usage_trend_tokens")}
              </button>
            </div>
          </div>
          <UsageChart option={trendOption} height="145px" />
        </div>

        <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-3 shadow-2xs space-y-1.5">
          <div class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("usage_platform_share")}</div>
          <UsageChart option={platformChartOption} height="145px" />
        </div>
      </div>

      <!-- Breakdown Tables -->
      <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl shadow-2xs overflow-hidden">
        <div class="px-4 py-2.5 border-b border-slate-200/80 dark:border-slate-800 flex items-center gap-1.5 flex-wrap bg-slate-50/70 dark:bg-slate-900">
          {#each [
            { id: "models", label: t("usage_tab_models") },
            { id: "platforms", label: t("usage_tab_platforms") },
            { id: "workspaces", label: t("usage_tab_workspaces") },
            { id: "sessions", label: t("usage_tab_sessions") },
          ] as tab (tab.id)}
            <button
              type="button"
              onclick={() => (breakdownTab = tab.id as typeof breakdownTab)}
              class="px-3 py-1 rounded-lg text-xs font-medium transition cursor-pointer {breakdownTab === tab.id
                ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 shadow-2xs border border-slate-200 dark:border-slate-700'
                : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
            >
              {tab.label}
            </button>
          {/each}
        </div>

        <div class="w-full overflow-x-clip">
          {#if breakdownTab === "sessions"}
            <table class="w-full text-left text-xs">
              <thead class="text-[10px] uppercase text-slate-400 border-b border-slate-100 dark:border-slate-800">
                <tr>
                  <th class="px-4 py-2.5">Platform</th>
                  <th class="px-4 py-2.5">Session / Workspace</th>
                  <th class="px-4 py-2.5">Model</th>
                  <th class="px-4 py-2.5 text-right">Tokens</th>
                  <th class="px-4 py-2.5 text-right">Cost</th>
                  <th class="px-4 py-2.5 text-right">Active</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
                {#each data.topSessions as s (s.platform + s.sessionId)}
                  <tr
                    class="hover:bg-slate-50 dark:hover:bg-slate-800/50 transition cursor-pointer"
                    onclick={() => appStore.jumpToSession(s.sessionId)}
                  >
                    <td class="px-4 py-2.5">
                      <span class="px-1.5 py-0.5 rounded text-[10px] font-bold uppercase border {getPlatformBadge(s.platform)}">
                        {s.platform}
                      </span>
                    </td>
                    <td class="px-4 py-2.5 max-w-[260px]">
                      <div class="font-mono text-[11px] text-slate-800 dark:text-slate-100 truncate flex items-center gap-1">
                        <span class="truncate">{s.sessionId}</span>
                        <ExternalLink class="h-3 w-3 text-slate-400 shrink-0" />
                      </div>
                      {#if s.workspace}
                        <div class="text-[10px] text-slate-400 truncate">{s.workspace}</div>
                      {/if}
                    </td>
                    <td class="px-4 py-2.5 font-mono text-[11px] text-slate-600 dark:text-slate-300">{s.model}</td>
                    <td class="px-4 py-2.5 text-right font-mono">{formatCompactTokens(s.totalTokens)}</td>
                    <td class="px-4 py-2.5 text-right font-mono font-semibold text-slate-900 dark:text-slate-100">
                      {formatMoney(s.cost, currency)}
                    </td>
                    <td class="px-4 py-2.5 text-right text-[10px] text-slate-400">{formatTimeAgo(s.lastTsMs)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {:else}
            {@const rows =
              breakdownTab === "models"
                ? data.byModel
                : breakdownTab === "platforms"
                  ? data.byPlatform
                  : data.byWorkspace}
            <table class="w-full text-left text-xs">
              <thead class="text-[10px] uppercase text-slate-400 border-b border-slate-100 dark:border-slate-800">
                <tr>
                  <th class="px-4 py-2.5">Name</th>
                  <th class="px-4 py-2.5 text-right">Sessions</th>
                  <th class="px-4 py-2.5 text-right">Input</th>
                  <th class="px-4 py-2.5 text-right">Output</th>
                  <th class="px-4 py-2.5 text-right">Cache Hit</th>
                  <th class="px-4 py-2.5 text-right">Total</th>
                  <th class="px-4 py-2.5 text-right">Cost</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
                {#each rows as row (row.key)}
                  <tr class="hover:bg-slate-50 dark:hover:bg-slate-800/50 transition">
                    <td class="px-4 py-2.5 font-medium text-slate-800 dark:text-slate-100 max-w-[260px] truncate" title={row.key}>
                      {#if breakdownTab === "platforms"}
                        {getPlatformLabel(row.key)}
                      {:else}
                        <span class={breakdownTab === "models" ? "font-mono text-[11px]" : ""}>{row.label}</span>
                      {/if}
                    </td>
                    <td class="px-4 py-2.5 text-right font-mono text-slate-500">{row.sessionCount}</td>
                    <td class="px-4 py-2.5 text-right font-mono text-slate-500">{formatCompactTokens(row.inputTokens)}</td>
                    <td class="px-4 py-2.5 text-right font-mono text-slate-500">{formatCompactTokens(row.outputTokens)}</td>
                    <td class="px-4 py-2.5 text-right font-mono text-emerald-600 dark:text-emerald-400">{row.cacheHitRate.toFixed(1)}%</td>
                    <td class="px-4 py-2.5 text-right font-mono font-semibold">{formatCompactTokens(row.totalTokens)}</td>
                    <td class="px-4 py-2.5 text-right font-mono font-semibold text-slate-900 dark:text-slate-100">
                      {#if row.priced}
                        {formatMoney(row.cost, currency)}
                      {:else if breakdownTab === "models"}
                        <button
                          type="button"
                          class="text-[10px] text-amber-600 hover:underline cursor-pointer"
                          onclick={() => openPricingFor(row.key)}
                        >
                          {t("usage_configure_price")}
                        </button>
                      {:else}
                        <span class="text-slate-400">-</span>
                      {/if}
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
      </div>

      <p class="text-[11px] text-slate-400 dark:text-slate-500 leading-relaxed">{t("usage_coverage_note")}</p>
    {/if}
  </div>
</div>

<PricingModal
  isOpen={showPricingModal}
  seedModel={pricingSeedModel}
  onClose={() => (showPricingModal = false)}
/>
