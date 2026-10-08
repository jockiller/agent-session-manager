<script lang="ts">
  import {
    Layers,
    RefreshCw,
    Star,
    AlertCircle,
    Activity,
    Search,
    X,
  } from "@lucide/svelte";
  import type { GlobalStats } from "../types";
  import { t } from "../i18n";

  let {
    selectedPlatform = $bindable("all"),
    selectedFilter = $bindable<"all" | "running" | "starred" | "orphans">("all"),
    stats = null,
    isLoading = false,
    starredCount = 0,
    onRefresh,
  }: {
    selectedPlatform: string;
    selectedFilter: "all" | "running" | "starred" | "orphans";
    stats: GlobalStats | null;
    isLoading: boolean;
    starredCount: number;
    onRefresh: () => void;
  } = $props();

  let platforms = $derived([
    { id: "all", label: t("all_platforms"), color: "bg-slate-800 text-white", dot: "bg-slate-400" },
    { id: "zcode", label: "ZCode", color: "text-teal-700 bg-teal-50 border-teal-200", dot: "bg-teal-500" },
    { id: "grok", label: "Grok", color: "text-slate-700 bg-slate-100 border-slate-300", dot: "bg-slate-500" },
    { id: "antigravity", label: "Antigravity", color: "text-sky-700 bg-sky-50 border-sky-200", dot: "bg-sky-500" },
    { id: "claude", label: "Claude Code", color: "text-amber-700 bg-amber-50 border-amber-200", dot: "bg-amber-500" },
    { id: "openclaude", label: "OpenClaude", color: "text-orange-700 bg-orange-50 border-orange-200", dot: "bg-orange-500" },
    { id: "codex", label: "Codex", color: "text-emerald-700 bg-emerald-50 border-emerald-200", dot: "bg-emerald-500" },
    { id: "trae", label: "Trae", color: "text-violet-700 bg-violet-50 border-violet-200", dot: "bg-violet-500" },
    { id: "qoder", label: "Qoder", color: "text-cyan-700 bg-cyan-50 border-cyan-200", dot: "bg-cyan-500" },
    { id: "cursor", label: "Cursor", color: "text-indigo-700 bg-indigo-50 border-indigo-200", dot: "bg-indigo-500" },
    { id: "codebuddy", label: "CodeBuddy", color: "text-lime-700 bg-lime-50 border-lime-200", dot: "bg-lime-500" },
    { id: "workbuddy", label: "WorkBuddy", color: "text-emerald-700 bg-emerald-50 border-emerald-200", dot: "bg-emerald-500" },
    { id: "dsh", label: "DSH", color: "text-purple-700 bg-purple-50 border-purple-200", dot: "bg-purple-500" },
    { id: "pi", label: "Pi", color: "text-rose-700 bg-rose-50 border-rose-200", dot: "bg-rose-500" },
    { id: "omp", label: "Oh My Pi", color: "text-pink-700 bg-pink-50 border-pink-200", dot: "bg-pink-500" },
    { id: "qwen", label: "Qwen", color: "text-blue-700 bg-blue-50 border-blue-200", dot: "bg-blue-500" },
    { id: "opencode", label: "OpenCode", color: "text-blue-700 bg-blue-50 border-blue-200", dot: "bg-blue-500" },
    { id: "codewiz", label: "CodeWiz", color: "text-purple-700 bg-purple-50 border-purple-200", dot: "bg-purple-500" },
    { id: "kimi", label: "Kimi Code", color: "text-teal-700 bg-teal-50 border-teal-200", dot: "bg-teal-500" },
    { id: "gemini", label: "Gemini CLI", color: "text-sky-700 bg-sky-50 border-sky-200", dot: "bg-sky-500" },
    { id: "deepseek", label: "DeepSeek CLI", color: "text-indigo-700 bg-indigo-50 border-indigo-200", dot: "bg-indigo-500" },
    { id: "openclaw", label: "OpenClaw", color: "text-red-700 bg-red-50 border-red-200", dot: "bg-red-500" },
    { id: "hermes", label: "Hermes", color: "text-orange-700 bg-orange-50 border-orange-200", dot: "bg-orange-500" },
  ]);

  let sortedPlatforms = $derived([
    platforms[0],
    ...platforms
      .slice(1)
      .slice()
      .sort((a, b) => {
        const countA = stats?.platform_distribution[a.id]?.count || 0;
        const countB = stats?.platform_distribution[b.id]?.count || 0;
        if (countB !== countA) {
          return countB - countA;
        }
        return a.label.localeCompare(b.label);
      }),
  ]);

  let platformQuery = $state("");

  let filteredPlatforms = $derived.by(() => {
    const q = platformQuery.trim().toLowerCase();
    if (!q) return sortedPlatforms;
    return sortedPlatforms.filter((p) => {
      if (p.id === "all") return false;
      return (
        p.id.toLowerCase().includes(q) ||
        p.label.toLowerCase().includes(q)
      );
    });
  });
</script>

<aside class="w-56 shrink-0 h-full bg-slate-50/95 border-r border-slate-200/90 flex flex-col overflow-hidden select-none">
  <!-- Top: Brand Header & Refresh -->
  <div class="px-4 py-3 border-b border-slate-200/80 flex items-center justify-between shrink-0">
    <div class="flex items-center gap-2.5 min-w-0">
      <div class="h-8 w-8 rounded-lg bg-gradient-to-tr from-sky-500 via-indigo-500 to-indigo-600 flex items-center justify-center shadow-xs text-white shrink-0">
        <Layers class="h-4.5 w-4.5" />
      </div>
      <div class="min-w-0">
        <h1 class="text-xs font-bold text-slate-900 tracking-tight leading-tight truncate">
          {t("app_title")}
        </h1>
        <p class="text-[10px] text-slate-500 leading-none truncate">{t("app_subtitle")}</p>
      </div>
    </div>

    <!-- Quick refresh button -->
    <button
      onclick={onRefresh}
      disabled={isLoading}
      class="p-1.5 rounded-lg hover:bg-slate-200/70 text-slate-500 hover:text-slate-800 disabled:opacity-40 transition cursor-pointer"
      title={t("rescan_tooltip")}
    >
      <RefreshCw class="h-3.5 w-3.5 {isLoading ? 'animate-spin text-sky-600' : ''}" />
    </button>
  </div>

  <!-- Quick Navigation Views -->
  <div class="p-2 space-y-0.5 shrink-0 border-b border-slate-200/60">
    <div class="px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-slate-400">
      {t("session_views")}
    </div>

    <!-- All Sessions -->
    <button
      onclick={() => { selectedFilter = "all"; }}
      class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer {selectedFilter === 'all'
        ? 'bg-white text-slate-900 font-semibold shadow-2xs border border-slate-200/80'
        : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'}"
    >
      <div class="flex items-center gap-2">
        <Layers class="h-3.5 w-3.5 text-sky-600" />
        <span>{t("all_sessions")}</span>
      </div>
      {#if stats}
        <span class="text-[10px] font-mono font-medium px-1.5 py-0.2 rounded-full {selectedFilter === 'all' ? 'bg-slate-100 text-slate-800' : 'text-slate-400'}">
          {stats.total_sessions}
        </span>
      {/if}
    </button>

    <!-- Running Sessions -->
    <button
      onclick={() => { selectedFilter = "running"; }}
      class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer {selectedFilter === 'running'
        ? 'bg-white text-emerald-800 font-semibold shadow-2xs border border-slate-200/80'
        : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'}"
    >
      <div class="flex items-center gap-2">
        <Activity class="h-3.5 w-3.5 text-emerald-600" />
        <span>{t("running")}</span>
      </div>
      {#if stats && stats.running_count > 0}
        <span class="text-[10px] font-mono font-bold px-1.5 py-0.2 rounded-full bg-emerald-50 text-emerald-700 border border-emerald-200">
          {stats.running_count}
        </span>
      {:else}
        <span class="text-[10px] text-slate-400 font-mono">0</span>
      {/if}
    </button>

    <!-- Starred -->
    <button
      onclick={() => { selectedFilter = "starred"; }}
      class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer {selectedFilter === 'starred'
        ? 'bg-white text-amber-800 font-semibold shadow-2xs border border-slate-200/80'
        : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'}"
    >
      <div class="flex items-center gap-2">
        <Star class="h-3.5 w-3.5 text-amber-500 fill-amber-500" />
        <span>{t("starred")}</span>
      </div>
      {#if starredCount > 0}
        <span class="text-[10px] font-mono font-bold px-1.5 py-0.2 rounded-full bg-amber-50 text-amber-700 border border-amber-200">
          {starredCount}
        </span>
      {:else}
        <span class="text-[10px] text-slate-400 font-mono">0</span>
      {/if}
    </button>

    <!-- Orphans -->
    <button
      onclick={() => { selectedFilter = "orphans"; }}
      class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer {selectedFilter === 'orphans'
        ? 'bg-white text-rose-800 font-semibold shadow-2xs border border-slate-200/80'
        : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'}"
    >
      <div class="flex items-center gap-2">
        <AlertCircle class="h-3.5 w-3.5 text-rose-500" />
        <span>{t("empty_logs")}</span>
      </div>
      {#if stats && stats.orphan_count > 0}
        <span class="text-[10px] font-mono font-bold px-1.5 py-0.2 rounded-full bg-rose-50 text-rose-700 border border-rose-200">
          {stats.orphan_count}
        </span>
      {:else}
        <span class="text-[10px] text-slate-400 font-mono">0</span>
      {/if}
    </button>
  </div>

  <!-- Platform List: Takes all available flexible vertical height! -->
  <div class="p-2 pt-1 flex-1 min-h-0 flex flex-col">
    <div class="flex items-center justify-between px-2 py-1 text-[10px] font-bold uppercase tracking-wider text-slate-400 shrink-0">
      <span>{t("agent_platforms")}</span>
      {#if platformQuery.trim()}
        <span class="text-[9px] text-sky-600 font-mono font-normal">{t("matched_count", { n: filteredPlatforms.length })}</span>
      {/if}
    </div>

    <!-- Platform Search Input -->
    <div class="px-1 pb-1.5 shrink-0">
      <div class="relative">
        <Search class="absolute left-2 top-2 h-3 w-3 text-slate-400 pointer-events-none" />
        <input
          type="text"
          bind:value={platformQuery}
          placeholder={t("search_platforms")}
          class="w-full bg-slate-100/90 focus:bg-white border border-slate-200/90 focus:border-sky-500 rounded-md pl-6.5 pr-6 py-1 text-[11px] text-slate-800 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-sky-500 transition shadow-2xs"
        />
        {#if platformQuery}
          <button
            onclick={() => (platformQuery = "")}
            class="absolute right-1.5 top-1.5 text-slate-400 hover:text-slate-600 p-0.5 cursor-pointer"
            title={t("clear_search")}
          >
            <X class="h-3 w-3" />
          </button>
        {/if}
      </div>
    </div>

    <div class="flex-1 min-h-0 overflow-y-auto space-y-0.5 pr-0.5">
      {#if filteredPlatforms.length === 0}
        <div class="py-4 text-center text-[11px] text-slate-400">
          {t("no_matching_platforms")}
        </div>
      {:else}
        {#each filteredPlatforms as p}
          {@const pCount = p.id === "all" ? stats?.total_sessions : stats?.platform_distribution[p.id]?.count}
          {@const pLabel = p.label}
          <button
            onclick={() => { selectedPlatform = p.id; }}
            class="w-full flex items-center justify-between px-2.5 py-1.5 rounded-lg text-xs font-medium transition cursor-pointer {selectedPlatform === p.id
              ? 'bg-white text-slate-900 font-semibold shadow-2xs border border-slate-200/80'
              : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900'}"
          >
            <div class="flex items-center gap-2 min-w-0">
              <span class="h-2 w-2 rounded-full {p.dot} shrink-0"></span>
              <span class="truncate">{pLabel}</span>
            </div>

            {#if pCount !== undefined && pCount > 0}
              <span class="text-[10px] font-mono font-medium px-1.5 py-0.2 rounded-full {selectedPlatform === p.id ? 'bg-slate-100 text-slate-800' : 'text-slate-400'}">
                {pCount}
              </span>
            {/if}
          </button>
        {/each}
      {/if}
    </div>
  </div>
</aside>
