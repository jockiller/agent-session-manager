<script lang="ts">
  import {
    Folder,
    Clock,
    CheckSquare,
    Square,
    AlertCircle,
    ChevronDown,
    ChevronRight,
    Star,
    Sparkles,
    CornerDownRight,
    ListTree,
    List,
    Search,
    Filter,
    ArrowUpDown,
    Zap,
    X,
    Database,
    Activity,
    RotateCcw,
    Calendar,
  } from "@lucide/svelte";
  import type { SessionSummary, SessionTreeNode } from "../types";
  import {
    formatBytes,
    formatTimeAgo,
    formatTokens,
    getPlatformBadge,
    toLocalDateString,
  } from "../utils";
  import { t } from "../i18n";

  let {
    treeNodes = [],
    flatSessions = [],
    allSessions = [],
    viewMode = $bindable("tree"),
    searchQuery = $bindable(""),
    sortBy = $bindable("updated_desc"),
    selectedWorkspace = $bindable("all"),
    selectedTimeRange = $bindable("all"),
    customStartDate = $bindable(""),
    customEndDate = $bindable(""),
    selectedKind = $bindable("all"),
    workspaces = [],
    selectedSessionId = null,
    selectedIds = $bindable(new Set<string>()),
    starredIds = new Set<string>(),
    onSelectSession,
    onToggleStar,
  }: {
    treeNodes: SessionTreeNode[];
    flatSessions: SessionSummary[];
    allSessions: SessionSummary[];
    viewMode: "tree" | "flat";
    searchQuery: string;
    sortBy: string;
    selectedWorkspace: string;
    selectedTimeRange: string;
    customStartDate?: string;
    customEndDate?: string;
    selectedKind: string;
    workspaces: string[];
    selectedSessionId: string | null;
    selectedIds: Set<string>;
    starredIds: Set<string>;
    onSelectSession: (s: SessionSummary) => void;
    onToggleStar: (id: string, e: Event) => void;
  } = $props();

  let showFilterPopover = $state(false);

  // Track manually collapsed parent nodes (default is expanded)
  let collapsedParents = $state<Set<string>>(new Set());

  function toggleExpand(parentId: string, e: Event) {
    e.stopPropagation();
    const next = new Set(collapsedParents);
    if (next.has(parentId)) {
      next.delete(parentId);
    } else {
      next.add(parentId);
    }
    collapsedParents = next;
  }

  function expandAll() {
    collapsedParents = new Set();
  }

  function collapseAll() {
    collapsedParents = new Set(treeNodes.filter((n) => n.subagents.length > 0).map((n) => n.session.id));
  }

  // All visible session IDs across tree or flat (deduplicated by id)
  let allVisibleSessions = $derived(
    viewMode === "tree"
      ? Array.from(
          new Map(
            treeNodes
              .flatMap((n) => [n.session, ...n.subagents])
              .map((s) => [s.id, s])
          ).values()
        )
      : flatSessions
  );

  // Dynamic telemetry metrics reactive to all active filter criteria
  let visibleTelemetry = $derived.by(() => {
    let totalTokens = 0;
    let totalBytes = 0;
    let promptTokens = 0;
    let cacheReadTokens = 0;
    let runningCount = 0;

    for (const s of allVisibleSessions) {
      totalBytes += s.size_bytes || 0;
      if (s.is_running) runningCount++;
      if (s.token_stats) {
        totalTokens += s.token_stats.total_tokens || 0;
        promptTokens += s.token_stats.prompt_tokens || 0;
        cacheReadTokens += s.token_stats.cache_read_tokens || 0;
      }
    }

    const totalCachePrompt = promptTokens + cacheReadTokens;
    const cacheHitRate =
      totalCachePrompt > 0 && cacheReadTokens > 0
        ? Math.round((cacheReadTokens / totalCachePrompt) * 1000) / 10
        : null;

    return {
      count: allVisibleSessions.length,
      totalTokens,
      totalBytes,
      cacheHitRate,
      runningCount,
    };
  });

  // Select all derivations
  let isAllSelected = $derived(
    allVisibleSessions.length > 0 &&
      allVisibleSessions.every((s) => selectedIds.has(s.id))
  );

  let selectedVisibleCount = $derived(
    allVisibleSessions.filter((s) => selectedIds.has(s.id)).length
  );

  function toggleSelect(id: string, e: Event) {
    e.stopPropagation();
    const next = new Set(selectedIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    selectedIds = next;
  }

  function toggleSelectAll() {
    const next = new Set(selectedIds);
    if (isAllSelected) {
      for (const s of allVisibleSessions) {
        next.delete(s.id);
      }
    } else {
      for (const s of allVisibleSessions) {
        next.add(s.id);
      }
    }
    selectedIds = next;
  }

  // Quick time presets
  let quickPresets = $derived([
    { id: "all", label: t("preset_all") },
    { id: "today", label: t("preset_today") },
    { id: "yesterday", label: t("preset_yesterday") },
    { id: "3d", label: t("preset_3d") },
    { id: "7d", label: t("preset_7d") },
    { id: "30d", label: t("preset_30d") },
  ]);

  function applyTimePreset(presetId: string) {
    const now = new Date();
    const todayStr = toLocalDateString(now);

    if (presetId === "all") {
      selectedTimeRange = "all";
      customStartDate = "";
      customEndDate = "";
    } else if (presetId === "today") {
      selectedTimeRange = "today";
      customStartDate = todayStr;
      customEndDate = todayStr;
    } else if (presetId === "yesterday") {
      const y = new Date();
      y.setDate(y.getDate() - 1);
      const yStr = toLocalDateString(y);
      selectedTimeRange = "yesterday";
      customStartDate = yStr;
      customEndDate = yStr;
    } else if (presetId === "3d") {
      const d = new Date();
      d.setDate(d.getDate() - 2);
      selectedTimeRange = "3d";
      customStartDate = toLocalDateString(d);
      customEndDate = todayStr;
    } else if (presetId === "7d") {
      const d = new Date();
      d.setDate(d.getDate() - 6);
      selectedTimeRange = "7d";
      customStartDate = toLocalDateString(d);
      customEndDate = todayStr;
    } else if (presetId === "30d") {
      const d = new Date();
      d.setDate(d.getDate() - 29);
      selectedTimeRange = "30d";
      customStartDate = toLocalDateString(d);
      customEndDate = todayStr;
    }
  }

  function handleCustomDateInput() {
    selectedTimeRange = "custom";
  }

  let isTimeFilterActive = $derived(
    Boolean(customStartDate) ||
      Boolean(customEndDate) ||
      (selectedTimeRange !== "all" && selectedTimeRange !== "")
  );

  let activePreset = $derived.by(() => {
    if (!customStartDate && !customEndDate) {
      return selectedTimeRange === "all" ? "all" : selectedTimeRange;
    }
    const todayStr = toLocalDateString(new Date());
    const y = new Date();
    y.setDate(y.getDate() - 1);
    const yStr = toLocalDateString(y);
    const d3 = new Date();
    d3.setDate(d3.getDate() - 2);
    const d3Str = toLocalDateString(d3);
    const d7 = new Date();
    d7.setDate(d7.getDate() - 6);
    const d7Str = toLocalDateString(d7);
    const d30 = new Date();
    d30.setDate(d30.getDate() - 29);
    const d30Str = toLocalDateString(d30);

    if (customStartDate === todayStr && customEndDate === todayStr) return "today";
    if (customStartDate === yStr && customEndDate === yStr) return "yesterday";
    if (customStartDate === d3Str && customEndDate === todayStr) return "3d";
    if (customStartDate === d7Str && customEndDate === todayStr) return "7d";
    if (customStartDate === d30Str && customEndDate === todayStr) return "30d";
    return "custom";
  });

  let timeFilterChipLabel = $derived.by(() => {
    if (activePreset === "today") return t("preset_today");
    if (activePreset === "yesterday") return t("preset_yesterday");
    if (activePreset === "3d") return t("preset_3d");
    if (activePreset === "7d") return t("preset_7d");
    if (activePreset === "30d") return t("preset_30d");
    if (customStartDate && customEndDate) {
      return customStartDate === customEndDate
        ? customStartDate
        : `${customStartDate} ~ ${customEndDate}`;
    }
    if (customStartDate && !customEndDate) return t("from_date_onwards", { date: customStartDate });
    if (!customStartDate && customEndDate) return t("until_date", { date: customEndDate });
    return t("filtered_chip");
  });

  // Filter derivations & helper chips
  let activeFilterCount = $derived(
    (selectedWorkspace !== "all" ? 1 : 0) +
      (isTimeFilterActive ? 1 : 0) +
      (selectedKind !== "all" ? 1 : 0)
  );

  let hasActiveFilters = $derived(
    Boolean(searchQuery.trim()) || activeFilterCount > 0
  );

  let searchInputEl = $state<HTMLInputElement | null>(null);

  function resetFilters() {
    searchQuery = "";
    selectedWorkspace = "all";
    selectedKind = "all";
    applyTimePreset("all");
  }

  function handleWindowKeydown(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
      e.preventDefault();
      searchInputEl?.focus();
      searchInputEl?.select();
    } else if (e.key === "Escape" && document.activeElement === searchInputEl) {
      searchQuery = "";
      searchInputEl?.blur();
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<div class="w-96 md:w-[410px] shrink-0 flex flex-col h-full bg-slate-50/70 border-r border-slate-200 overflow-hidden">
  <!-- Top Toolbar: Search + Quick Tools + Active Chips -->
  <div class="p-3 border-b border-slate-200 bg-white/95 backdrop-blur-md space-y-2 shrink-0 shadow-2xs">
    <!-- Search Input -->
    <div class="relative">
      <Search class="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-slate-400" />
      <input
        bind:this={searchInputEl}
        type="text"
        bind:value={searchQuery}
        placeholder={t("search_sessions")}
        class="w-full bg-slate-50 border border-slate-200 focus:bg-white rounded-lg pl-8 pr-12 py-1.5 text-xs text-slate-900 placeholder:text-slate-400 focus:outline-none focus:border-sky-500 focus:ring-1 focus:ring-sky-500 transition shadow-2xs"
      />
      {#if searchQuery}
        <button
          onclick={() => (searchQuery = "")}
          class="absolute right-2.5 top-2 text-slate-400 hover:text-slate-600 p-0.5 cursor-pointer"
          title={t("clear_search")}
        >
          <X class="h-3.5 w-3.5" />
        </button>
      {:else}
        <span class="absolute right-2.5 top-2 text-[10px] text-slate-400 font-mono pointer-events-none border border-slate-200/90 rounded px-1 py-0.2 bg-white/80">
          ⌘K
        </span>
      {/if}
    </div>

    <!-- Secondary Controls Row: Select All + Expand/Collapse on Left, View + Sort + Filter on Right -->
    <div class="flex items-center justify-between text-xs text-slate-600 gap-1.5 min-w-0">
      <!-- Left: Master Select All & Tree Expand/Collapse -->
      <div class="flex items-center gap-1.5 min-w-0">
        <button
          onclick={toggleSelectAll}
          disabled={allVisibleSessions.length === 0}
          class="flex items-center gap-1 px-1.5 py-0.5 rounded-md text-xs font-medium text-slate-700 hover:bg-slate-100 hover:text-slate-900 transition cursor-pointer select-none whitespace-nowrap disabled:opacity-40 disabled:cursor-not-allowed"
          title={isAllSelected ? t("deselect_all") : t("select_all")}
        >
          {#if isAllSelected}
            <CheckSquare class="h-3.5 w-3.5 text-sky-600 shrink-0" />
            <span class="text-[11px] font-semibold text-sky-700 whitespace-nowrap">{t("select_all")} ({allVisibleSessions.length})</span>
          {:else if selectedVisibleCount > 0}
            <Square class="h-3.5 w-3.5 text-sky-600 shrink-0" />
            <span class="text-[11px] font-semibold text-slate-800 whitespace-nowrap">{t("selected_count", { n: selectedVisibleCount })}/{allVisibleSessions.length}</span>
          {:else}
            <Square class="h-3.5 w-3.5 text-slate-400 shrink-0" />
            <span class="text-[11px] text-slate-600 whitespace-nowrap">{t("select_all")} ({allVisibleSessions.length})</span>
          {/if}
        </button>

        {#if viewMode === "tree"}
          <div class="flex items-center gap-1 text-[11px] text-slate-400 pl-1 border-l border-slate-200 whitespace-nowrap">
            <button onclick={expandAll} class="hover:text-sky-600 transition cursor-pointer hover:underline">{t("expand")}</button>
            <span class="text-slate-300">/</span>
            <button onclick={collapseAll} class="hover:text-sky-600 transition cursor-pointer hover:underline">{t("collapse")}</button>
          </div>
        {/if}
      </div>

      <!-- Right Action Group: View Switcher + Sort + Filter -->
      <div class="flex items-center gap-1 shrink-0">
        <!-- View Mode: Tree vs Flat -->
        <div class="flex items-center bg-slate-100 p-0.5 rounded-lg border border-slate-200 shrink-0">
          <button
            onclick={() => (viewMode = "tree")}
            class="p-1 rounded text-xs transition cursor-pointer {viewMode === 'tree'
              ? 'bg-white text-sky-700 shadow-2xs font-semibold'
              : 'text-slate-500 hover:text-slate-800'}"
            title={t("view_tree")}
          >
            <ListTree class="h-3.5 w-3.5" />
          </button>
          <button
            onclick={() => (viewMode = "flat")}
            class="p-1 rounded text-xs transition cursor-pointer {viewMode === 'flat'
              ? 'bg-white text-sky-700 shadow-2xs font-semibold'
              : 'text-slate-500 hover:text-slate-800'}"
            title={t("view_flat")}
          >
            <List class="h-3.5 w-3.5" />
          </button>
        </div>

        <!-- Sort Dropdown -->
        <div class="flex items-center bg-slate-50 border border-slate-200 px-1.5 py-0.5 rounded-lg text-[11px] shrink-0">
          <ArrowUpDown class="h-3 w-3 text-slate-400 mr-1" />
          <select
            bind:value={sortBy}
            class="bg-transparent text-[11px] text-slate-700 focus:outline-none cursor-pointer pr-1"
          >
            <option value="updated_desc">{t("sort_updated")}</option>
            <option value="created_asc">{t("sort_created")}</option>
            <option value="tokens_desc">{t("sort_tokens")}</option>
            <option value="turns_desc">{t("sort_turns")}</option>
            <option value="size_desc">{t("sort_size")}</option>
          </select>
        </div>

        <!-- Filter Toggle Button (With Active Badge) -->
        <button
          onclick={() => (showFilterPopover = !showFilterPopover)}
          class="relative p-1 rounded-lg border text-xs transition cursor-pointer shrink-0 {showFilterPopover
            ? 'bg-sky-50 border-sky-300 text-sky-700'
            : activeFilterCount > 0
              ? 'bg-sky-50 border-sky-200 text-sky-700'
              : 'bg-white border-slate-200 text-slate-600 hover:bg-slate-50'}"
          title={t("active_filter")}
        >
          <Filter class="h-3.5 w-3.5" />
          {#if activeFilterCount > 0}
            <span class="absolute -top-1 -right-1 h-3.5 w-3.5 bg-sky-600 text-white text-[9px] font-bold rounded-full flex items-center justify-center">
              {activeFilterCount}
            </span>
          {/if}
        </button>
      </div>
    </div>

    <!-- Filter Popover Panel (Collapsible) -->
    {#if showFilterPopover}
      <div class="pt-2 border-t border-slate-100 flex flex-col gap-2.5 text-[11px] text-slate-600 bg-slate-50/90 p-2.5 rounded-lg border border-slate-200/80 animate-in slide-in-from-top-1 duration-100">
        <!-- Top Row: Workspace & Kind Filters -->
        <div class="flex items-center gap-2">
          <!-- Workspace Filter -->
          <div class="flex-1 min-w-0">
            <span class="text-slate-500 font-medium block text-[10px] mb-1">{t("workspace")}</span>
            <select
              bind:value={selectedWorkspace}
              class="w-full bg-white border border-slate-200 rounded-md px-2 py-1 text-[11px] text-slate-800 focus:outline-none focus:border-sky-500 focus:ring-1 focus:ring-sky-500 cursor-pointer shadow-2xs"
            >
              <option value="all">{t("all_workspaces", { n: workspaces.length })}</option>
              {#each workspaces as ws}
                <option value={ws}>{ws.split("/").pop() || ws}</option>
              {/each}
            </select>
          </div>

          <!-- Kind Filter -->
          <div class="w-24 shrink-0">
            <span class="text-slate-500 font-medium block text-[10px] mb-1">{t("hierarchy")}</span>
            <select
              bind:value={selectedKind}
              class="w-full bg-white border border-slate-200 rounded-md px-2 py-1 text-[11px] text-slate-800 focus:outline-none focus:border-sky-500 focus:ring-1 focus:ring-sky-500 cursor-pointer shadow-2xs"
            >
              <option value="all">{t("all_hierarchy")}</option>
              <option value="main">{t("main_session")}</option>
              <option value="sub">{t("subagent_session")}</option>
            </select>
          </div>
        </div>

        <!-- Bottom Section: Time Range Section (Quick Presets + Date Range Pickers) -->
        <div class="pt-2 border-t border-slate-200/70 space-y-2">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-1 text-[10px] font-medium text-slate-600">
              <Calendar class="h-3 w-3 text-slate-500" />
              <span>{t("time_range")}</span>
            </div>
            {#if isTimeFilterActive}
              <button
                onclick={() => applyTimePreset("all")}
                class="text-[10px] text-slate-400 hover:text-rose-600 transition cursor-pointer"
                title={t("clear_time")}
              >
                {t("clear_time")}
              </button>
            {/if}
          </div>

          <!-- Quick Presets Capsules -->
          <div class="flex items-center gap-1 flex-wrap">
            {#each quickPresets as preset}
              <button
                onclick={() => applyTimePreset(preset.id)}
                class="px-2 py-0.5 rounded-full text-[10px] transition cursor-pointer border {activePreset === preset.id
                  ? 'bg-sky-600 text-white border-sky-600 font-medium shadow-2xs'
                  : 'bg-white hover:bg-slate-100 text-slate-600 border-slate-200'}"
              >
                {preset.label}
              </button>
            {/each}
          </div>

          <!-- Date Range Picker Inputs -->
          <div class="flex items-center gap-1.5 pt-0.5">
            <div class="flex-1 min-w-0">
              <input
                type="date"
                bind:value={customStartDate}
                max={customEndDate || undefined}
                onchange={handleCustomDateInput}
                class="w-full bg-white border border-slate-200 rounded-md px-1.5 py-1 text-[11px] text-slate-800 focus:outline-none focus:border-sky-500 focus:ring-1 focus:ring-sky-500 font-mono shadow-2xs cursor-pointer"
              />
            </div>
            <span class="text-slate-400 text-[11px] shrink-0 font-medium">{t("to_date")}</span>
            <div class="flex-1 min-w-0">
              <input
                type="date"
                bind:value={customEndDate}
                min={customStartDate || undefined}
                onchange={handleCustomDateInput}
                class="w-full bg-white border border-slate-200 rounded-md px-1.5 py-1 text-[11px] text-slate-800 focus:outline-none focus:border-sky-500 focus:ring-1 focus:ring-sky-500 font-mono shadow-2xs cursor-pointer"
              />
            </div>
            {#if customStartDate || customEndDate}
              <button
                onclick={() => applyTimePreset("all")}
                class="p-1 text-slate-400 hover:text-rose-600 rounded hover:bg-slate-100 transition cursor-pointer shrink-0"
                title={t("clear_time")}
              >
                <X class="h-3 w-3" />
              </button>
            {/if}
          </div>
        </div>
      </div>
    {/if}

    <!-- Active Filter Chips Tag Bar -->
    {#if hasActiveFilters}
      <div class="pt-1.5 border-t border-slate-100 flex items-center flex-wrap gap-1.5 text-[10px]">
        {#if searchQuery.trim()}
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-slate-100 text-slate-700 border border-slate-200 whitespace-nowrap">
            <span>{t("search_chip_prefix")}"{searchQuery.trim()}"</span>
            <button onclick={() => (searchQuery = "")} class="hover:text-slate-900 cursor-pointer">
              <X class="h-2.5 w-2.5" />
            </button>
          </span>
        {/if}

        {#if selectedWorkspace !== "all"}
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-sky-50 text-sky-800 border border-sky-200 whitespace-nowrap">
            <span>{t("workspace_chip_prefix")}{selectedWorkspace.split("/").pop() || selectedWorkspace}</span>
            <button onclick={() => (selectedWorkspace = "all")} class="hover:text-sky-950 cursor-pointer">
              <X class="h-2.5 w-2.5" />
            </button>
          </span>
        {/if}

        {#if isTimeFilterActive}
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-indigo-50 text-indigo-800 border border-indigo-200 whitespace-nowrap">
            <span>{t("time_chip_prefix")}{timeFilterChipLabel}</span>
            <button onclick={() => applyTimePreset("all")} class="hover:text-indigo-950 cursor-pointer">
              <X class="h-2.5 w-2.5" />
            </button>
          </span>
        {/if}

        {#if selectedKind !== "all"}
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-amber-50 text-amber-800 border border-amber-200 whitespace-nowrap">
            <span>{t("hierarchy_chip_prefix")}{selectedKind === "main" ? t("main_session") : t("subagent_session")}</span>
            <button onclick={() => (selectedKind = "all")} class="hover:text-amber-950 cursor-pointer">
              <X class="h-2.5 w-2.5" />
            </button>
          </span>
        {/if}

        <button
          onclick={resetFilters}
          class="inline-flex items-center gap-0.5 text-slate-400 hover:text-rose-600 transition cursor-pointer font-medium ml-auto whitespace-nowrap"
          title={t("reset_all_filters")}
        >
          <RotateCcw class="h-2.5 w-2.5" />
          <span>{t("reset")}</span>
        </button>
      </div>
    {/if}
  </div>

  <!-- Dynamic Linked Telemetry Bar: Clean Single-Line Status Strip (No wrapping, no overcrowding) -->
  <div class="px-3.5 py-1.5 bg-slate-100/70 border-b border-slate-200/80 flex items-center justify-between text-xs text-slate-500 shrink-0 select-none whitespace-nowrap">
    <!-- Left: Count & Running Status -->
    <div class="flex items-center gap-2 whitespace-nowrap shrink-0">
      <span class="font-semibold text-slate-700 text-[11px] whitespace-nowrap">
        {visibleTelemetry.count} {t("sessions_suffix")}
      </span>
      {#if visibleTelemetry.runningCount > 0}
        <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded-full text-[10px] font-medium bg-emerald-50 text-emerald-700 border border-emerald-200/80 whitespace-nowrap">
          <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
          <span>{visibleTelemetry.runningCount} {t("running")}</span>
        </span>
      {/if}
    </div>

    <!-- Right: Concise Single-Line Telemetry -->
    <div class="flex items-center gap-2 font-mono text-[11px] whitespace-nowrap shrink-0">
      <!-- Token Metric -->
      <span class="flex items-center gap-1 text-amber-800 bg-amber-50/70 px-1.5 py-0.5 rounded border border-amber-200/60 whitespace-nowrap font-medium" title={t("telemetry_tokens_tip")}>
        <Zap class="h-3 w-3 text-amber-600 shrink-0" />
        <span>{formatTokens(visibleTelemetry.totalTokens)}</span>
      </span>

      <!-- Cache Hit Rate Metric -->
      {#if visibleTelemetry.cacheHitRate !== null}
        <span class="text-sky-700 bg-sky-50/70 px-1.5 py-0.5 rounded border border-sky-200/60 whitespace-nowrap font-medium" title={t("telemetry_cache_tip")}>
          {t("cache_hit", { rate: visibleTelemetry.cacheHitRate })}
        </span>
      {/if}

      <!-- Total Size Metric -->
      <span class="flex items-center gap-1 text-slate-600 bg-white/80 px-1.5 py-0.5 rounded border border-slate-200/70 whitespace-nowrap" title={t("telemetry_disk_tip")}>
        <Database class="h-3 w-3 text-indigo-500 shrink-0" />
        <span>{formatBytes(visibleTelemetry.totalBytes)}</span>
      </span>
    </div>
  </div>

  <!-- Session Cards Stream -->
  <div class="flex-1 overflow-y-auto p-2 space-y-1.5">
    {#if allVisibleSessions.length === 0}
      <div class="h-64 flex flex-col items-center justify-center text-slate-400 text-xs">
        <Folder class="h-10 w-10 mb-2 stroke-1 text-slate-300" />
        <p class="font-medium text-slate-500">{t("no_sessions_found")}</p>
        <p class="text-[11px] text-slate-400 mt-0.5">{t("no_sessions_found_hint")}</p>
      </div>
    {:else if viewMode === "tree"}
      <!-- Hierarchical Tree Mode -->
      {#each treeNodes as node (node.session.id)}
        <div class="space-y-1">
          <!-- Main Parent Session Card -->
          <div
            role="button"
            tabindex="0"
            onclick={() => onSelectSession(node.session)}
            onkeydown={(e) => e.key === "Enter" && onSelectSession(node.session)}
            class="group relative p-2.5 rounded-xl border text-left transition cursor-pointer {selectedSessionId === node.session.id
              ? 'bg-sky-50/80 border-sky-400 shadow-sm ring-1 ring-sky-300/80'
              : 'bg-white hover:bg-slate-50/90 border-slate-200/90 hover:border-slate-300 shadow-2xs'}"
          >
            <!-- Top Row: Checkbox, Platform Badge, Model, Status, Subagents Pill, Star -->
            <div class="flex items-center justify-between gap-1.5 mb-1">
              <div class="flex items-center gap-1.5 flex-wrap min-w-0">
                <button
                  onclick={(e) => toggleSelect(node.session.id, e)}
                  class="text-slate-400 hover:text-sky-600 p-0.5 shrink-0"
                  title={t("checkbox_select_tip")}
                >
                  {#if selectedIds.has(node.session.id)}
                    <CheckSquare class="h-3.5 w-3.5 text-sky-600" />
                  {:else}
                    <Square class="h-3.5 w-3.5 text-slate-300 group-hover:text-slate-400" />
                  {/if}
                </button>

                <span class="px-1.5 py-0.2 rounded text-[10px] font-bold uppercase tracking-wider border {getPlatformBadge(node.session.platform)}">
                  {node.session.platform}
                </span>

                {#if node.session.flavor && node.session.flavor !== node.session.platform}
                  <span class="text-[10px] px-1.5 py-0.2 rounded bg-slate-100 text-slate-600 border border-slate-200 font-mono truncate max-w-[130px]" title={node.session.flavor}>
                    {node.session.flavor}
                  </span>
                {/if}

                {#if node.session.is_running}
                  <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-emerald-50 text-emerald-700 border border-emerald-200 font-medium">
                    <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                    {t("running")}
                  </span>
                {/if}

                {#if !node.session.has_transcript}
                  <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-amber-50 text-amber-700 border border-amber-200">
                    <AlertCircle class="h-2.5 w-2.5" />
                    {t("empty_logs")}
                  </span>
                {/if}
              </div>

              <!-- Right: Subagents toggle + Star button -->
              <div class="flex items-center gap-1 shrink-0">
                {#if node.subagents.length > 0}
                  <button
                    onclick={(e) => toggleExpand(node.session.id, e)}
                    class="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-indigo-50 hover:bg-indigo-100 text-indigo-700 border border-indigo-200 transition cursor-pointer"
                    title={t("expand") + "/" + t("collapse")}
                  >
                    <Sparkles class="h-2.5 w-2.5 text-indigo-600" />
                    <span>{t("subagents_count", { n: node.subagents.length })}</span>
                    {#if !collapsedParents.has(node.session.id)}
                      <ChevronDown class="h-2.5 w-2.5" />
                    {:else}
                      <ChevronRight class="h-2.5 w-2.5" />
                    {/if}
                  </button>
                {/if}

                <button
                  onclick={(e) => onToggleStar(node.session.id, e)}
                  class="p-0.5 text-slate-300 hover:text-amber-500 transition cursor-pointer"
                  title={t("star_session")}
                >
                  <Star class="h-3.5 w-3.5 {starredIds.has(node.session.id) ? 'text-amber-500 fill-amber-500' : ''}" />
                </button>
              </div>
            </div>

            <!-- Title -->
            <h3 class="text-xs font-semibold text-slate-900 line-clamp-2 leading-snug mb-1 group-hover:text-sky-700 transition">
              {node.session.title || t("untitled_session")}
            </h3>

            <!-- Bottom Metadata & Telemetry Chips (Concise: cwd hidden) -->
            <div class="flex items-center justify-between text-[11px] text-slate-500 gap-1.5 pt-0.5">
              <div class="flex items-center gap-1.5 text-[10px] text-slate-400 font-sans shrink-0">
                <div class="flex items-center gap-0.5" title={t("updated_time")}>
                  <Clock class="h-2.5 w-2.5" />
                  <span>{formatTimeAgo(node.session.updated_at)}</span>
                </div>
                <span>·</span>
                {#if node.session.turn_count > 0}
                  <span>{node.session.turn_count} {t("turns")}</span>
                  <span>·</span>
                {/if}
                <span>{formatBytes(node.session.size_bytes)}</span>
              </div>

              <div class="flex items-center gap-1.5 shrink-0 font-mono flex-wrap justify-end">
                {#if node.session.token_stats && node.session.token_stats.total_tokens > 0}
                  <span class="inline-flex items-center gap-1 text-amber-800 bg-amber-50 px-1.5 py-0.2 rounded border border-amber-200/80 font-bold text-[10px]" title={t("total_tokens")}>
                    <Zap class="h-2.5 w-2.5 text-amber-600 shrink-0" />
                    <span>{formatTokens(node.session.token_stats.total_tokens)}</span>
                  </span>
                  {#if node.session.token_stats.cache_hit_rate !== undefined && node.session.token_stats.cache_hit_rate !== null && node.session.token_stats.cache_hit_rate > 0}
                    <span class="inline-flex items-center gap-0.5 text-[9px] px-1 py-0.2 rounded bg-sky-50 text-sky-700 border border-sky-200 font-mono" title={t("cache_hit_rate")}>
                      <Database class="h-2.5 w-2.5 text-sky-600 shrink-0" />
                      <span>{node.session.token_stats.cache_hit_rate}%</span>
                    </span>
                  {/if}
                  {#if node.session.token_stats.tokens_per_second !== undefined && node.session.token_stats.tokens_per_second !== null && node.session.token_stats.tokens_per_second > 0}
                    <span class="inline-flex items-center gap-0.5 text-[9px] px-1 py-0.2 rounded bg-emerald-50 text-emerald-700 border border-emerald-200 font-mono" title={t("output_speed")}>
                      <Activity class="h-2.5 w-2.5 text-emerald-600 shrink-0" />
                      <span>{node.session.token_stats.tokens_per_second.toFixed(0)} t/s</span>
                    </span>
                  {/if}
                {/if}
              </div>
            </div>
          </div>

          <!-- Mounted Subagents Container (Collapsible) -->
          {#if node.subagents.length > 0 && !collapsedParents.has(node.session.id)}
            <div class="ml-3.5 pl-2.5 border-l-2 border-indigo-200/90 space-y-1 py-0.5">
              {#each node.subagents as sub (sub.id)}
                <div
                  role="button"
                  tabindex="0"
                  onclick={() => onSelectSession(sub)}
                  onkeydown={(e) => e.key === "Enter" && onSelectSession(sub)}
                  class="group relative p-2 rounded-lg border text-left transition cursor-pointer {selectedSessionId === sub.id
                    ? 'bg-indigo-50/90 border-indigo-400 shadow-xs ring-1 ring-indigo-300'
                    : 'bg-white hover:bg-slate-50 border-slate-200/80 hover:border-slate-300 shadow-2xs'}"
                >
                  <div class="flex items-center justify-between gap-1 mb-1">
                    <div class="flex items-center gap-1.5 flex-wrap">
                      <button
                        onclick={(e) => toggleSelect(sub.id, e)}
                        class="text-slate-400 hover:text-indigo-600 p-0.5"
                      >
                        {#if selectedIds.has(sub.id)}
                          <CheckSquare class="h-3 w-3 text-indigo-600" />
                        {:else}
                          <Square class="h-3 w-3 text-slate-300 group-hover:text-slate-400" />
                        {/if}
                      </button>

                      <span class="inline-flex items-center gap-0.5 px-1.5 py-0.2 rounded text-[9px] font-bold bg-indigo-50 text-indigo-700 border border-indigo-200">
                        <CornerDownRight class="h-2.5 w-2.5" />
                        {t("subagent_badge")}
                      </span>

                      {#if sub.is_running}
                        <span class="inline-flex items-center gap-0.5 px-1 py-0.2 rounded text-[9px] bg-emerald-50 text-emerald-700 border border-emerald-200 font-medium">
                          <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                          {t("running")}
                        </span>
                      {/if}
                    </div>

                    <button
                      onclick={(e) => onToggleStar(sub.id, e)}
                      class="p-0.5 text-slate-300 hover:text-amber-500 transition cursor-pointer"
                      title={t("star_session")}
                    >
                      <Star class="h-3 w-3 {starredIds.has(sub.id) ? 'text-amber-500 fill-amber-500' : ''}" />
                    </button>
                  </div>

                  <h4 class="text-[11px] font-medium text-slate-800 line-clamp-1 group-hover:text-indigo-700 transition">
                    {sub.title || t("untitled_subagent")}
                  </h4>

                  <div class="flex items-center justify-between text-[10px] text-slate-400 mt-0.5 font-mono">
                    <span class="truncate max-w-[130px] text-slate-500 font-sans">
                      {sub.flavor || sub.dirname}
                    </span>
                    <div class="flex items-center gap-1.5 flex-wrap justify-end">
                      {#if sub.token_stats && sub.token_stats.total_tokens > 0}
                        <span class="inline-flex items-center gap-0.5 text-amber-800 bg-amber-50 px-1 py-0.2 rounded border border-amber-200/60 font-mono">
                          <Zap class="h-2.5 w-2.5 text-amber-600 shrink-0" />
                          <span>{formatTokens(sub.token_stats.total_tokens)}</span>
                        </span>
                      {/if}
                      <span>{formatBytes(sub.size_bytes)}</span>
                      <span>{formatTimeAgo(sub.updated_at)}</span>
                    </div>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/each}
    {:else}
      <!-- Flat List Mode -->
      {#each flatSessions as s (s.id)}
        <div
          role="button"
          tabindex="0"
          onclick={() => onSelectSession(s)}
          onkeydown={(e) => e.key === "Enter" && onSelectSession(s)}
          class="group relative p-2.5 rounded-xl border text-left transition cursor-pointer {selectedSessionId === s.id
            ? 'bg-sky-50/80 border-sky-400 shadow-sm ring-1 ring-sky-300/80'
            : 'bg-white hover:bg-slate-50/90 border-slate-200/90 hover:border-slate-300 shadow-2xs'}"
        >
          <!-- Top Row -->
          <div class="flex items-center justify-between gap-1.5 mb-1">
            <div class="flex items-center gap-1.5 flex-wrap min-w-0">
              <button
                onclick={(e) => toggleSelect(s.id, e)}
                class="text-slate-400 hover:text-sky-600 p-0.5"
              >
                {#if selectedIds.has(s.id)}
                  <CheckSquare class="h-3.5 w-3.5 text-sky-600" />
                {:else}
                  <Square class="h-3.5 w-3.5 text-slate-300 group-hover:text-slate-400" />
                {/if}
              </button>

              <span class="px-1.5 py-0.2 rounded text-[10px] font-bold uppercase tracking-wider border {getPlatformBadge(s.platform)}">
                {s.platform}
              </span>

              {#if s.is_subagent}
                <span class="inline-flex items-center gap-0.5 px-1.5 py-0.2 rounded text-[10px] font-medium bg-indigo-50 text-indigo-700 border border-indigo-200">
                  <CornerDownRight class="h-2.5 w-2.5" />
                  {t("subagent_badge")}
                </span>
              {/if}

              {#if s.is_running}
                <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-emerald-50 text-emerald-700 border border-emerald-200 font-medium">
                  <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                  {t("running")}
                </span>
              {/if}

              {#if !s.has_transcript}
                <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-amber-50 text-amber-700 border border-amber-200">
                  <AlertCircle class="h-2.5 w-2.5" />
                  {t("empty_logs")}
                </span>
              {/if}
            </div>

            <button
              onclick={(e) => onToggleStar(s.id, e)}
              class="p-0.5 text-slate-300 hover:text-amber-500 transition cursor-pointer"
              title={t("star_session")}
            >
              <Star class="h-3.5 w-3.5 {starredIds.has(s.id) ? 'text-amber-500 fill-amber-500' : ''}" />
            </button>
          </div>

          <!-- Title -->
          <h3 class="text-xs font-semibold text-slate-900 line-clamp-2 leading-snug mb-1 group-hover:text-sky-700 transition">
            {s.title || t("untitled_session")}
          </h3>

          <!-- Metadata (Concise: cwd hidden) -->
          <div class="flex items-center justify-between text-[11px] text-slate-500 gap-1.5 pt-0.5">
            <div class="flex items-center gap-1.5 text-[10px] text-slate-400 font-sans shrink-0">
              <div class="flex items-center gap-0.5" title={t("updated_time")}>
                <Clock class="h-2.5 w-2.5" />
                <span>{formatTimeAgo(s.updated_at)}</span>
              </div>
              <span>·</span>
              {#if s.turn_count > 0}
                <span>{s.turn_count} {t("turns")}</span>
                <span>·</span>
              {/if}
              <span>{formatBytes(s.size_bytes)}</span>
            </div>

            <div class="flex items-center gap-1.5 shrink-0 font-mono flex-wrap justify-end">
              {#if s.token_stats && s.token_stats.total_tokens > 0}
                <span class="inline-flex items-center gap-1 text-amber-800 bg-amber-50 px-1.5 py-0.2 rounded border border-amber-200/80 font-bold text-[10px]" title={t("total_tokens")}>
                  <Zap class="h-2.5 w-2.5 text-amber-600 shrink-0" />
                  <span>{formatTokens(s.token_stats.total_tokens)}</span>
                </span>
                {#if s.token_stats.cache_hit_rate !== undefined && s.token_stats.cache_hit_rate !== null && s.token_stats.cache_hit_rate > 0}
                  <span class="inline-flex items-center gap-0.5 text-[9px] px-1 py-0.2 rounded bg-sky-50 text-sky-700 border border-sky-200 font-mono" title={t("cache_hit_rate")}>
                    <Database class="h-2.5 w-2.5 text-sky-600 shrink-0" />
                    <span>{s.token_stats.cache_hit_rate}%</span>
                  </span>
                {/if}
                {#if s.token_stats.tokens_per_second !== undefined && s.token_stats.tokens_per_second !== null && s.token_stats.tokens_per_second > 0}
                  <span class="inline-flex items-center gap-0.5 text-[9px] px-1 py-0.2 rounded bg-emerald-50 text-emerald-700 border border-emerald-200 font-mono" title={t("output_speed")}>
                    <Activity class="h-2.5 w-2.5 text-emerald-600 shrink-0" />
                    <span>{s.token_stats.tokens_per_second.toFixed(0)} t/s</span>
                  </span>
                {/if}
              {/if}
            </div>
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>
