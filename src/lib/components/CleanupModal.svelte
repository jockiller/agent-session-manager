<script lang="ts">
  import {
    Trash2,
    X,
    ShieldCheck,
    AlertTriangle,
    Database,
    Zap,
    RefreshCw,
    AlertCircle,
  } from "@lucide/svelte";
  import type { SessionSummary } from "../types";
  import { formatBytes, formatTokens, getPlatformBadge } from "../utils";
  import { t } from "../i18n";

  let {
    isOpen = false,
    selectedSessions = [],
    isCleaning = false,
    onClose,
    onConfirm,
  }: {
    isOpen: boolean;
    selectedSessions: SessionSummary[];
    isCleaning: boolean;
    onClose: () => void;
    onConfirm: (force: boolean) => Promise<void>;
  } = $props();

  let forceRunning = $state(false);

  // Derived statistics of selected sessions
  let totalBytes = $derived(
    selectedSessions.reduce((acc, s) => acc + (s.size_bytes || 0), 0)
  );

  let totalTokens = $derived(
    selectedSessions.reduce((acc, s) => acc + (s.token_stats?.total_tokens || 0), 0)
  );

  let runningSessions = $derived(
    selectedSessions.filter((s) => s.is_running)
  );

  let runningCount = $derived(runningSessions.length);

  let effectiveTargetCount = $derived(
    forceRunning ? selectedSessions.length : selectedSessions.length - runningCount
  );

  let effectiveBytes = $derived(
    forceRunning
      ? totalBytes
      : selectedSessions.filter((s) => !s.is_running).reduce((acc, s) => acc + (s.size_bytes || 0), 0)
  );

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget && !isCleaning) {
      onClose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !isCleaning) {
      onClose();
    }
  }

  async function handleConfirmClick() {
    if (isCleaning || selectedSessions.length === 0) return;
    await onConfirm(forceRunning);
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <div
    class="fixed inset-0 z-50 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center p-4 animate-in fade-in duration-150 select-none"
    onclick={handleBackdropClick}
    role="dialog"
    aria-modal="true"
    aria-labelledby="modal-title"
  >
    <div
      class="bg-white rounded-2xl shadow-2xl border border-slate-200/90 w-full max-w-lg overflow-hidden flex flex-col max-h-[85vh] animate-in zoom-in-95 duration-150"
      onclick={(e) => e.stopPropagation()}
    >
      <!-- Modal Header -->
      <div class="px-5 py-4 border-b border-slate-100 flex items-center justify-between shrink-0 bg-slate-50/50">
        <div class="flex items-center gap-3">
          <div class="h-9 w-9 rounded-xl bg-rose-50 text-rose-600 flex items-center justify-center border border-rose-200/70 shrink-0">
            <Trash2 class="h-4.5 w-4.5" />
          </div>
          <div>
            <h2 id="modal-title" class="text-sm font-bold text-slate-900 tracking-tight">
              {t("cleanup_confirm_title")}
            </h2>
            <p class="text-xs text-slate-500 mt-0.5">
              {t("cleanup_notice")}
            </p>
          </div>
        </div>

        <button
          onclick={onClose}
          disabled={isCleaning}
          class="p-1.5 rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition cursor-pointer disabled:opacity-40"
          title={t("close")}
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- Modal Body -->
      <div class="p-5 overflow-y-auto space-y-4 flex-1">
        {#if selectedSessions.length === 0}
          <div class="py-8 text-center space-y-2">
            <AlertCircle class="h-10 w-10 text-slate-300 mx-auto" />
            <h3 class="text-xs font-semibold text-slate-700">{t("no_selection_title")}</h3>
            <p class="text-[11px] text-slate-400 max-w-xs mx-auto">
              {t("no_selection_desc")}
            </p>
          </div>
        {:else}
          <!-- Key Statistics 3-column Grid -->
          <div class="grid grid-cols-3 gap-2.5">
            <div class="p-3 rounded-xl bg-slate-50 border border-slate-200/80 flex flex-col">
              <span class="text-[10px] text-slate-400 font-medium">{t("selected_sessions_stat")}</span>
              <span class="text-base font-bold text-slate-900 mt-1 font-mono">
                {selectedSessions.length} <span class="text-xs font-normal text-slate-500">{t("items_suffix")}</span>
              </span>
            </div>

            <div class="p-3 rounded-xl bg-rose-50/70 border border-rose-200/70 flex flex-col">
              <div class="flex items-center gap-1 text-[10px] text-rose-600 font-medium">
                <Database class="h-3 w-3 shrink-0" />
                <span>{t("estimated_reclaim")}</span>
              </div>
              <span class="text-base font-bold text-rose-700 mt-1 font-mono">
                {formatBytes(effectiveBytes)}
              </span>
            </div>

            <div class="p-3 rounded-xl bg-amber-50/70 border border-amber-200/70 flex flex-col">
              <div class="flex items-center gap-1 text-[10px] text-amber-700 font-medium">
                <Zap class="h-3 w-3 shrink-0" />
                <span>{t("tokens_consumed")}</span>
              </div>
              <span class="text-base font-bold text-amber-900 mt-1 font-mono">
                {formatTokens(totalTokens)}
              </span>
            </div>
          </div>

          <!-- Running session protection alert -->
          {#if runningCount > 0}
            <div class="p-3 rounded-xl bg-amber-50/90 border border-amber-200/90 text-xs space-y-2 text-amber-900">
              <div class="flex items-center gap-2 font-semibold">
                <AlertTriangle class="h-4 w-4 text-amber-600 shrink-0" />
                <span>{t("running_detected_title", { n: runningCount })}</span>
              </div>
              <p class="text-[11px] text-amber-800 leading-relaxed">
                {t("running_detected_desc")}
              </p>
              <label class="flex items-center gap-2 pt-1 text-xs cursor-pointer select-none">
                <input
                  type="checkbox"
                  bind:checked={forceRunning}
                  class="rounded text-rose-600 focus:ring-0 border-amber-300"
                />
                <span class="font-medium">{t("force_terminate_checkbox")}</span>
              </label>
            </div>
          {/if}

          <!-- Selected Sessions List Preview -->
          <div class="space-y-1.5">
            <div class="flex items-center justify-between text-xs text-slate-500 font-medium px-0.5">
              <span>{t("cleanup_list_heading")}</span>
              <span class="text-[11px] font-mono text-slate-400">{t("items_total", { n: selectedSessions.length })}</span>
            </div>

            <div class="max-h-48 overflow-y-auto space-y-1 p-1.5 bg-slate-50/80 rounded-xl border border-slate-200/80 divide-y divide-slate-100">
              {#each selectedSessions as s}
                <div class="pt-1.5 first:pt-0 flex items-center justify-between gap-2 text-xs py-1 px-1">
                  <div class="flex items-center gap-2 min-w-0">
                    <span class="text-[10px] font-mono px-1.5 py-0.2 rounded border uppercase font-bold shrink-0 {getPlatformBadge(s.platform)}">
                      {s.platform}
                    </span>
                    <span class="text-xs text-slate-800 font-medium truncate" title={s.title}>
                      {s.title}
                    </span>
                  </div>

                  <div class="flex items-center gap-2 shrink-0 font-mono text-[11px] text-slate-400">
                    {#if s.is_running}
                      <span class="px-1.5 py-0.2 rounded text-[10px] font-semibold bg-emerald-50 text-emerald-700 border border-emerald-200 flex items-center gap-1">
                        <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                        {t("running")}
                      </span>
                    {/if}
                    <span>{formatBytes(s.size_bytes)}</span>
                  </div>
                </div>
              {/each}
            </div>
          </div>

          <!-- Native Trash Safety Notice -->
          <div class="flex items-start gap-2 p-2.5 rounded-xl bg-slate-50 text-slate-600 border border-slate-200/80 text-[11px] leading-relaxed">
            <ShieldCheck class="h-4 w-4 text-emerald-600 shrink-0 mt-0.5" />
            <span>
              {t("native_trash_notice")}
            </span>
          </div>
        {/if}
      </div>

      <!-- Modal Footer -->
      <div class="px-5 py-3.5 border-t border-slate-100 bg-slate-50/60 flex items-center justify-between shrink-0">
        <span class="text-xs text-slate-400">
          {#if selectedSessions.length > 0}
            {t("will_move_to_trash")} <strong class="text-slate-700 font-bold">{effectiveTargetCount}</strong> {t("items_suffix")}
          {/if}
        </span>

        <div class="flex items-center gap-2">
          <button
            onclick={onClose}
            disabled={isCleaning}
            class="px-3.5 py-1.5 rounded-xl border border-slate-200 bg-white hover:bg-slate-50 text-slate-700 text-xs font-semibold transition cursor-pointer shadow-2xs disabled:opacity-40"
          >
            {t("cancel")}
          </button>

          {#if selectedSessions.length > 0}
            <button
              onclick={handleConfirmClick}
              disabled={isCleaning || effectiveTargetCount === 0}
              class="px-4 py-1.5 rounded-xl bg-rose-600 hover:bg-rose-700 active:bg-rose-800 text-white text-xs font-semibold transition cursor-pointer shadow-2xs flex items-center gap-1.5 disabled:opacity-40"
            >
              {#if isCleaning}
                <RefreshCw class="h-3.5 w-3.5 animate-spin" />
                <span>{t("cleaning_in_progress")}</span>
              {:else}
                <Trash2 class="h-3.5 w-3.5" />
                <span>{t("confirm_move_to_trash")}</span>
              {/if}
            </button>
          {/if}
        </div>
      </div>
    </div>
  </div>
{/if}
