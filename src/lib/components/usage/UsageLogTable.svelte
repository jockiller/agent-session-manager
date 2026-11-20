<script lang="ts">
  import { ChevronLeft, ChevronRight, ExternalLink } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { formatCompactTokens, formatDurationMs, formatMoney } from "$lib/usage/store.svelte";
  import type { UsageLogPage } from "$lib/usage/types";
  import { formatTime, getPlatformBadge, normalizeTimestamp } from "$lib/utils";

  let {
    logs = null,
    loading = false,
    showSessionCols = true,
    fillHeight = false,
    onPageChange,
    onPageSizeChange,
    onJumpSession,
    onConfigurePrice,
  }: {
    logs: UsageLogPage | null;
    loading: boolean;
    showSessionCols?: boolean;
    fillHeight?: boolean;
    onPageChange: (page: number) => void;
    onPageSizeChange?: (size: number) => void;
    onJumpSession?: (sessionId: string) => void;
    onConfigurePrice?: (model: string) => void;
  } = $props();

  let page = $derived(logs?.page ?? 1);
  let pageSize = $derived(logs?.pageSize ?? 50);
  let total = $derived(logs?.total ?? 0);
  let totalPages = $derived(Math.max(1, Math.ceil(total / Math.max(1, pageSize))));
  let items = $derived(logs?.items ?? []);
  let currency = $derived(logs?.currency ?? "USD");

  function formatLogTime(ts: number): string {
    const nts = normalizeTimestamp(ts);
    if (!nts) return "-";
    const d = new Date(nts);
    const p = (n: number) => String(n).padStart(2, "0");
    return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
  }

  function hitRateClass(rate: number): string {
    if (rate >= 70) return "bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-950/60 dark:text-emerald-300 dark:border-emerald-800/70";
    if (rate >= 30) return "bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-950/60 dark:text-amber-300 dark:border-amber-800/70";
    return "bg-slate-100 text-slate-600 border-slate-200 dark:bg-slate-800 dark:text-slate-400 dark:border-slate-700";
  }

  function latencyClass(ms: number): string {
    if (!ms || ms <= 0) return "text-slate-400";
    if (ms <= 3000) return "text-emerald-600 dark:text-emerald-400";
    if (ms <= 10000) return "text-amber-600 dark:text-amber-400";
    return "text-claude-600 dark:text-claude-400";
  }

  function formatTokSpeed(outputTokens: number, durationMs: number, ttftMs: number): string {
    if (!outputTokens || outputTokens <= 0 || !durationMs || durationMs <= 0) return "";
    const genMs = ttftMs > 0 && durationMs > ttftMs ? durationMs - ttftMs : durationMs;
    if (genMs < 50) return "";
    return `${((outputTokens * 1000) / genMs).toFixed(1)} t/s`;
  }
</script>

{#if loading && items.length === 0}
  <div class="py-10 text-center text-xs text-slate-400">{t("loading_messages")}</div>
{:else if items.length === 0}
  <div class="py-10 text-center text-xs text-slate-400">{t("usage_log_empty")}</div>
{:else}
  <div class={fillHeight ? "flex-1 min-h-0 flex flex-col" : "flex flex-col"}>
    <div class={fillHeight ? "flex-1 min-h-0 overflow-y-auto overflow-x-clip" : "w-full overflow-x-clip"}>
      <table class="w-full text-left text-xs">
        <thead class="{fillHeight ? 'sticky top-0 z-10' : ''} text-[10px] uppercase text-slate-400 border-b border-slate-200/80 dark:border-slate-800 bg-slate-50 dark:bg-slate-900">
          <tr>
            <th class="px-2.5 py-2">{t("usage_log_time")}</th>
            {#if showSessionCols}<th class="px-2.5 py-2">Platform</th>{/if}
            <th class="px-2.5 py-2">Model</th>
            <th class="px-2.5 py-2 text-right">{t("usage_log_ttft")}</th>
            <th class="px-2.5 py-2 text-right">{t("usage_log_duration")}</th>
            <th class="px-2.5 py-2 text-right">Tokens (In / Out)</th>
            <th class="px-2.5 py-2 text-right">{t("usage_cache_hit")}</th>
            <th class="px-2.5 py-2 text-right">Cost</th>
            {#if showSessionCols}<th class="px-2.5 py-2">Session</th>{/if}
          </tr>
        </thead>
        <tbody class="divide-y divide-slate-100 dark:divide-slate-800">
          {#each items as row (row.id)}
            {@const spd = formatTokSpeed(row.outputTokens, row.durationMs, row.ttftMs)}
            <tr class="hover:bg-slate-50/80 dark:hover:bg-slate-800/40 transition">
              <td class="px-2.5 py-1.5 font-mono text-[11px] text-slate-500 dark:text-slate-400 whitespace-nowrap" title={formatTime(row.tsMs)}>
                {formatLogTime(row.tsMs)}
                {#if row.granularity === "session"}
                  <span class="ml-1 px-1 py-0.2 rounded text-[9px] bg-slate-100 dark:bg-slate-800 text-slate-500" title={t("usage_log_session_granularity")}>S</span>
                {/if}
              </td>
              {#if showSessionCols}
                <td class="px-2.5 py-1.5 whitespace-nowrap">
                  <span class="px-1.5 py-0.5 rounded text-[10px] font-bold uppercase border {getPlatformBadge(row.platform)}">
                    {row.platform}
                  </span>
                </td>
              {/if}
              <td class="px-2.5 py-1.5 font-mono text-[11px] text-slate-800 dark:text-slate-200 max-w-[130px] truncate" title={row.modelRaw}>
                {row.modelNorm || row.modelRaw}
              </td>
              <td
                class="px-2.5 py-1.5 text-right font-mono text-[11px] whitespace-nowrap {latencyClass(row.ttftMs)}"
                title={row.ttftMs <= 0 ? t("usage_log_latency_na") : undefined}
              >
                {formatDurationMs(row.ttftMs)}
              </td>
              <td
                class="px-2.5 py-1.5 text-right font-mono text-[11px] whitespace-nowrap {latencyClass(row.durationMs)}"
                title={row.durationMs <= 0 ? t("usage_log_latency_na") : undefined}
              >
                <div>{formatDurationMs(row.durationMs)}</div>
                {#if spd}
                  <div class="text-[10px] text-slate-400">{spd}</div>
                {/if}
              </td>
              <td class="px-2.5 py-1.5 text-right font-mono whitespace-nowrap">
                <div class="text-[11px] text-slate-800 dark:text-slate-100">
                  {formatCompactTokens(row.inputTokens)} / {formatCompactTokens(row.outputTokens)}
                </div>
                {#if row.cacheReadTokens > 0 || row.cacheWriteTokens > 0 || row.reasoningTokens > 0}
                  <div class="text-[10px] text-slate-400">
                    {#if row.cacheReadTokens > 0}CR {formatCompactTokens(row.cacheReadTokens)}{/if}
                    {#if row.cacheWriteTokens > 0}{row.cacheReadTokens > 0 ? " · " : ""}CW {formatCompactTokens(row.cacheWriteTokens)}{/if}
                    {#if row.reasoningTokens > 0}{(row.cacheReadTokens > 0 || row.cacheWriteTokens > 0) ? " · " : ""}R {formatCompactTokens(row.reasoningTokens)}{/if}
                  </div>
                {/if}
              </td>
              <td class="px-2.5 py-1.5 text-right font-mono whitespace-nowrap">
                <span class="inline-block px-1.5 py-0.5 rounded border text-[10px] font-medium {hitRateClass(row.cacheHitRate)}">
                  {row.cacheHitRate.toFixed(1)}%
                </span>
              </td>
              <td class="px-2.5 py-1.5 text-right font-mono font-semibold whitespace-nowrap text-slate-900 dark:text-slate-100">
                {#if row.priced}
                  {formatMoney(row.cost, currency)}
                {:else if onConfigurePrice && row.modelNorm}
                  <button type="button" class="text-[10px] text-amber-600 hover:underline cursor-pointer" onclick={() => onConfigurePrice(row.modelNorm)}>
                    {t("usage_configure_price")}
                  </button>
                {:else}
                  <span class="text-slate-400">-</span>
                {/if}
              </td>
              {#if showSessionCols}
                <td class="px-2.5 py-1.5 max-w-[96px]">
                  {#if onJumpSession}
                    <button
                      type="button"
                      onclick={() => onJumpSession(row.sessionId)}
                      class="inline-flex items-center gap-1 font-mono text-[10px] text-claude-600 dark:text-claude-400 hover:underline truncate max-w-full cursor-pointer"
                      title={`${row.sessionId}${row.workspace ? `\n${row.workspace}` : ""}`}
                    >
                      <span class="truncate">{row.sessionId.slice(0, 12)}</span>
                      <ExternalLink class="h-2.5 w-2.5 shrink-0" />
                    </button>
                  {/if}
                </td>
              {/if}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="shrink-0 px-4 py-2 border-t border-slate-100 dark:border-slate-800 flex flex-wrap items-center justify-between gap-2 text-[11px] text-slate-500 dark:text-slate-400 bg-slate-50/40 dark:bg-slate-900/40">
      <span class="font-mono">{t("usage_log_page_info", { page, totalPages, total })}</span>
      <div class="flex items-center gap-2">
        {#if onPageSizeChange}
          <select
            class="bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-md px-1.5 py-1 text-[11px] font-mono text-slate-700 dark:text-slate-200"
            value={String(pageSize)}
            onchange={(e) => onPageSizeChange(Number(e.currentTarget.value))}
          >
            <option value="25">25 / pg</option>
            <option value="50">50 / pg</option>
            <option value="100">100 / pg</option>
          </select>
        {/if}
        <div class="inline-flex items-center gap-1">
          <button
            type="button"
            disabled={page <= 1 || loading}
            onclick={() => onPageChange(page - 1)}
            class="h-7 px-2 inline-flex items-center gap-1 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-700 dark:text-slate-200 disabled:opacity-40 cursor-pointer disabled:cursor-not-allowed hover:bg-slate-50 dark:hover:bg-slate-700"
          >
            <ChevronLeft class="h-3.5 w-3.5" />
            <span>{t("prev_page")}</span>
          </button>
          <button
            type="button"
            disabled={page >= totalPages || loading}
            onclick={() => onPageChange(page + 1)}
            class="h-7 px-2 inline-flex items-center gap-1 rounded-md border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-700 dark:text-slate-200 disabled:opacity-40 cursor-pointer disabled:cursor-not-allowed hover:bg-slate-50 dark:hover:bg-slate-700"
          >
            <span>{t("next_page")}</span>
            <ChevronRight class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}