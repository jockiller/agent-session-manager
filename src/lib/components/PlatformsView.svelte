<script lang="ts">
  import { Eye, EyeOff, RotateCcw, Search, Layers } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { appStore } from "$lib/stores/app.svelte";
  import { PLATFORMS } from "$lib/platforms/registry";
  import type { GlobalStats } from "$lib/types";
  import { formatBytes } from "$lib/utils";
  import { btnBorder } from "./config/styles";

  let { stats = null }: { stats: GlobalStats | null } = $props();

  let query = $state("");

  let filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return PLATFORMS.filter(
      (p) => !q || p.id.toLowerCase().includes(q) || p.label.toLowerCase().includes(q)
    );
  });

  let hiddenCount = $derived(appStore.hiddenPlatforms.length);
</script>

<div class="flex-1 min-h-0 flex flex-col bg-slate-50 dark:bg-slate-950 overflow-y-auto">
  <div class="max-w-5xl w-full mx-auto p-6 space-y-5">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <div>
        <div class="flex items-center gap-2">
          <Layers class="h-4 w-4 text-claude-600 dark:text-claude-400" />
          <h1 class="text-base font-bold text-slate-900 dark:text-slate-100">{t("platforms_title")}</h1>
        </div>
        <p class="text-xs text-slate-500 dark:text-slate-400 mt-1">{t("platforms_desc")}</p>
      </div>

      <div class="flex items-center gap-2">
        <div class="relative w-52">
          <Search class="absolute left-2.5 top-2 h-3.5 w-3.5 text-slate-400 pointer-events-none" />
          <input
            type="text"
            bind:value={query}
            placeholder={t("search_platforms")}
            class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-lg pl-8 pr-3 py-1.5 text-xs text-slate-800 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-claude-500"
          />
        </div>
        {#if hiddenCount > 0}
          <button type="button" class={btnBorder} onclick={() => appStore.showAllPlatforms()}>
            <RotateCcw class="h-3.5 w-3.5" />
            {t("platforms_show_all")}
          </button>
        {/if}
      </div>
    </div>

    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
      {#each filtered as plat (plat.id)}
        {@const pstat = stats?.platform_distribution[plat.id]}
        {@const hidden = appStore.isPlatformHidden(plat.id)}
        <div
          class="bg-white dark:bg-slate-900 border rounded-xl p-3.5 shadow-2xs transition flex flex-col gap-2.5 {hidden
            ? 'border-slate-200/60 dark:border-slate-800 opacity-60'
            : 'border-slate-200/90 dark:border-slate-700/90'}"
        >
          <div class="flex items-center justify-between gap-2">
            <div class="flex items-center gap-2 min-w-0">
              <span class="h-2.5 w-2.5 rounded-full {plat.dot} shrink-0"></span>
              <span class="text-xs font-bold text-slate-900 dark:text-slate-100 truncate">{plat.label}</span>
              <span class="text-[10px] font-mono text-slate-400 dark:text-slate-500">{plat.id}</span>
            </div>
            <button
              type="button"
              onclick={() => appStore.togglePlatformVisibility(plat.id)}
              class="flex items-center gap-1 px-2 py-1 rounded-lg border text-[11px] font-medium transition cursor-pointer {hidden
                ? 'border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-800 text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'
                : 'border-claude-200 dark:border-claude-800 bg-claude-50 dark:bg-claude-950/60 text-claude-700 dark:text-claude-300'}"
              title={hidden ? t("platform_show") : t("platform_hide")}
            >
              {#if hidden}
                <EyeOff class="h-3 w-3" />
                <span>{t("platform_hidden")}</span>
              {:else}
                <Eye class="h-3 w-3" />
                <span>{t("platform_visible")}</span>
              {/if}
            </button>
          </div>

          <div class="flex flex-wrap gap-1">
            <span class="px-1.5 py-0.5 rounded bg-slate-100 dark:bg-slate-800 text-[10px] text-slate-600 dark:text-slate-300">
              {t("cap_sessions")}
            </span>
            {#if plat.hasUsage}
              <span class="px-1.5 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/60 text-[10px] text-emerald-700 dark:text-emerald-300">
                {t("cap_usage")}
              </span>
            {/if}
            {#if plat.instructionTargetId}
              <span class="px-1.5 py-0.5 rounded bg-claude-50 dark:bg-claude-950/60 text-[10px] text-claude-700 dark:text-claude-300">
                {t("cap_instructions")}
              </span>
            {/if}
            {#if plat.hasConfigExport}
              <span class="px-1.5 py-0.5 rounded bg-amber-50 dark:bg-amber-950/60 text-[10px] text-amber-700 dark:text-amber-300">
                {t("cap_config")}
              </span>
            {/if}
          </div>

          <div class="pt-1 border-t border-slate-100 dark:border-slate-800 flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400 font-mono">
            <span>{pstat ? `${pstat.count} ${t("sessions_suffix")}` : `0 ${t("sessions_suffix")}`}</span>
            <span>{pstat ? formatBytes(pstat.bytes) : "0 B"}</span>
          </div>
        </div>
      {/each}
    </div>
  </div>
</div>
