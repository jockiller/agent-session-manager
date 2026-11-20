<script lang="ts">
  import {
    Folder,
    Clock,
    CheckSquare,
    Square,
    MinusSquare,
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
    X,
    Archive,
    RotateCcw,
    Calendar,
    RefreshCw,
    Layers,
    Activity,
  } from "@lucide/svelte";
  import type { GlobalStats, SessionSummary, SessionTreeNode } from "../types";
  import { appStore } from "$lib/stores/app.svelte";
  import { PLATFORMS, getPlatformLabel } from "$lib/platforms/registry";
  import {
    formatBytes,
    formatTimeAgo,
    getPlatformBadge,
    toLocalDateString,
  } from "../utils";
  import { t } from "../i18n";

  let {
    treeNodes = [],
    flatSessions = [],
    allSessions = [],
    stats = null,
    isLoading = false,
    starredCount = 0,
    selectedPlatform = $bindable("all"),
    selectedFilter = $bindable<"all" | "running" | "starred" | "archived" | "orphans">("all"),
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
    onRefresh,
  }: {
    treeNodes: SessionTreeNode[];
    flatSessions: SessionSummary[];
    allSessions: SessionSummary[];
    stats: GlobalStats | null;
    isLoading: boolean;
    starredCount: number;
    selectedPlatform: string;
    selectedFilter: "all" | "running" | "starred" | "archived" | "orphans";
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
    onRefresh: () => void;
  } = $props();

  let showFilterPopover = $state(false);
  let showPlatformMenu = $state(false);
  let platformMenuQuery = $state("");

  let visiblePlatformOptions = $derived.by(() => {
    const q = platformMenuQuery.trim().toLowerCase();
    return PLATFORMS.filter((p) => {
      if (appStore.isPlatformHidden(p.id) && p.id !== selectedPlatform) return false;
      if (!q) return true;
      return p.id.toLowerCase().includes(q) || p.label.toLowerCase().includes(q);
    }).sort((a, b) => {
      const ca = stats?.platform_distribution[a.id]?.count || 0;
      const cb = stats?.platform_distribution[b.id]?.count || 0;
      if (cb !== ca) return cb - ca;
      return a.label.localeCompare(b.label);
    });
  });

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
    let runningCount = 0;

    for (const s of allVisibleSessions) {
      if (s.is_running) runningCount++;
    }

    return {
      count: allVisibleSessions.length,
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

  // Map parent session ID to its direct subagents' IDs
  let subagentIdsByParent = $derived.by(() => {
    const map = new Map<string, string[]>();

    for (const s of allSessions) {
      if (s.is_subagent && s.parent_id) {
        const list = map.get(s.parent_id) || [];
        if (!list.includes(s.id)) {
          list.push(s.id);
        }
        map.set(s.parent_id, list);
      }
    }

    for (const node of treeNodes) {
      if (node.subagents && node.subagents.length > 0) {
        const list = map.get(node.session.id) || [];
        for (const sub of node.subagents) {
          if (!list.includes(sub.id)) {
            list.push(sub.id);
          }
        }
        map.set(node.session.id, list);
      }
    }

    return map;
  });

  // Recursively collect all descendant subagent IDs of a session
  function getAllDescendantIds(parentId: string): string[] {
    const result: string[] = [];
    const visited = new Set<string>();

    function collect(pId: string) {
      const subs = subagentIdsByParent.get(pId) || [];
      for (const subId of subs) {
        if (!visited.has(subId)) {
          visited.add(subId);
          result.push(subId);
          collect(subId);
        }
      }
    }

    collect(parentId);
    return result;
  }

  // Get selection state of a session and its subagents ("all" | "none" | "partial")
  function getSelectionState(sessionId: string): "all" | "none" | "partial" {
    const isSelfSelected = selectedIds.has(sessionId);
    const subIds = getAllDescendantIds(sessionId);
    if (subIds.length === 0) {
      return isSelfSelected ? "all" : "none";
    }
    const selectedSubCount = subIds.filter((id) => selectedIds.has(id)).length;
    if (isSelfSelected && selectedSubCount === subIds.length) {
      return "all";
    }
    if (!isSelfSelected && selectedSubCount === 0) {
      return "none";
    }
    return "partial";
  }

  function toggleSelect(id: string, e: Event) {
    e.stopPropagation();
    const next = new Set(selectedIds);
    const subIds = getAllDescendantIds(id);
    const currentState = getSelectionState(id);

    if (currentState === "all") {
      // Uncheck session and all its subagent sessions
      next.delete(id);
      for (const subId of subIds) {
        next.delete(subId);
      }
    } else {
      // Check session and automatically check all its subagent sessions
      next.add(id);
      for (const subId of subIds) {
        next.add(subId);
      }
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
      d.setDate(d.getDate() - 7);
      selectedTimeRange = "7d";
      customStartDate = "";
      customEndDate = toLocalDateString(d);
    } else if (presetId === "30d") {
      const d = new Date();
      d.setDate(d.getDate() - 30);
      selectedTimeRange = "30d";
      customStartDate = "";
      customEndDate = toLocalDateString(d);
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
    d7.setDate(d7.getDate() - 7);
    const d7Str = toLocalDateString(d7);
    const d30 = new Date();
    d30.setDate(d30.getDate() - 30);
    const d30Str = toLocalDateString(d30);

    if (customStartDate === todayStr && customEndDate === todayStr) return "today";
    if (customStartDate === yStr && customEndDate === yStr) return "yesterday";
    if (customStartDate === d3Str && customEndDate === todayStr) return "3d";
    if (!customStartDate && customEndDate === d7Str) return "7d";
    if (!customStartDate && customEndDate === d30Str) return "30d";
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
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "r") {
      e.preventDefault();
      if (!isLoading) onRefresh();
    } else if (e.key === "Escape") {
      showPlatformMenu = false;
      if (document.activeElement === searchInputEl) {
        searchQuery = "";
        searchInputEl?.blur();
      }
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} onclick={() => { showPlatformMenu = false; }} />

<div class="w-96 md:w-[420px] shrink-0 flex flex-col h-full bg-slate-50/70 dark:bg-slate-950/60 border-r border-slate-200 dark:border-slate-800 overflow-hidden">
  <!-- Top Toolbar: Quick Views + Platform Selector + Refresh + Search -->
  <div class="p-3 border-b border-slate-200 dark:border-slate-800 bg-white/95 dark:bg-slate-900/95 backdrop-blur-md space-y-2 shrink-0 shadow-2xs">
    <!-- Row 0: Quick View Capsules + Platform Picker + Refresh -->
    <div class="flex items-center justify-between gap-1.5 min-w-0">
      <div class="flex items-center gap-1 overflow-x-auto no-scrollbar min-w-0">
        {#each [
          { id: "all", label: t("all_sessions"), count: stats?.total_sessions ?? 0, icon: Layers, activeCls: "bg-slate-900 dark:bg-claude-600 text-white border-slate-900 dark:border-claude-600" },
          { id: "running", label: t("running"), count: stats?.running_count ?? 0, icon: Activity, activeCls: "bg-emerald-600 text-white border-emerald-600" },
          { id: "starred", label: t("starred"), count: starredCount, icon: Star, activeCls: "bg-amber-500 text-white border-amber-500" },
          { id: "archived", label: t("archived"), count: stats?.archived_count ?? 0, icon: Archive, activeCls: "bg-violet-600 text-white border-violet-600" },
          { id: "orphans", label: t("empty_logs"), count: stats?.orphan_count ?? 0, icon: AlertCircle, activeCls: "bg-rose-600 text-white border-rose-600" },
        ] as tab (tab.id)}
          <button
            type="button"
            onclick={() => { selectedFilter = tab.id as typeof selectedFilter; }}
            class="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-[10px] font-medium border transition cursor-pointer shrink-0 {selectedFilter === tab.id
              ? tab.activeCls
              : 'bg-slate-50 dark:bg-slate-800 text-slate-600 dark:text-slate-300 border-slate-200 dark:border-slate-700 hover:bg-slate-100 dark:hover:bg-slate-700'}"
          >
            <span>{tab.label}</span>
            {#if tab.count > 0}
              <span class="font-mono text-[9px] opacity-80">{tab.count}</span>
            {/if}
          </button>
        {/each}
      </div>

      <button
        type="button"
        onclick={onRefresh}
        disabled={isLoading}
        class="p-1.5 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700 hover:text-claude-600 transition cursor-pointer disabled:opacity-40 shrink-0"
        title={t("rescan_tooltip")}
      >
        <RefreshCw class="h-3.5 w-3.5 {isLoading ? 'animate-spin text-claude-600' : ''}" />
      </button>
    </div>

    <!-- Search + Platform Dropdown Row -->
    <div class="flex items-center gap-1.5">
      <div class="relative flex-1 min-w-0">
        <Search class="absolute left-2.5 top-2.5 h-3.5 w-3.5 text-slate-400" />
        <input
          bind:this={searchInputEl}
          type="text"
          bind:value={searchQuery}
          placeholder={t("search_sessions")}
          class="w-full bg-slate-50 dark:bg-slate-800/80 border border-slate-200 dark:border-slate-700 focus:bg-white dark:focus:bg-slate-900 rounded-lg pl-8 pr-10 py-1.5 text-xs text-slate-900 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:border-claude-500 focus:ring-1 focus:ring-claude-500 transition shadow-2xs"
        />
        {#if searchQuery}
          <button
            onclick={() => (searchQuery = "")}
            class="absolute right-2 top-2 text-slate-400 hover:text-slate-600 dark:hover:text-slate-200 p-0.5 cursor-pointer"
            title={t("clear_search")}
          >
            <X class="h-3.5 w-3.5" />
          </button>
        {:else}
          <span class="absolute right-2 top-2 text-[10px] text-slate-400 font-mono pointer-events-none border border-slate-200/90 dark:border-slate-700 rounded px-1 py-0.2 bg-white/80 dark:bg-slate-900/80">
            ⌘K
          </span>
        {/if}
      </div>

      <!-- Platform Dropdown Trigger -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <div class="relative shrink-0" role="none" onclick={(e) => e.stopPropagation()}>
        <button
          type="button"
          onclick={() => (showPlatformMenu = !showPlatformMenu)}
          class="h-[30px] max-w-[138px] flex items-center gap-1.5 px-2.5 rounded-lg border text-[11px] font-medium transition cursor-pointer {selectedPlatform !== 'all'
            ? 'bg-claude-50 dark:bg-claude-950/70 border-claude-300 dark:border-claude-700 text-claude-800 dark:text-claude-300'
            : 'bg-slate-50 dark:bg-slate-800 border-slate-200 dark:border-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700'}"
        >
          <span class="truncate">
            {selectedPlatform === "all" ? t("all_platforms") : getPlatformLabel(selectedPlatform)}
          </span>
          <ChevronDown class="h-3 w-3 shrink-0 opacity-70" />
        </button>

        {#if showPlatformMenu}
          <div class="absolute right-0 mt-1.5 w-60 z-40 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 shadow-xl p-2 space-y-1.5">
            <input
              type="text"
              bind:value={platformMenuQuery}
              placeholder={t("search_platforms")}
              class="w-full bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 rounded-md px-2 py-1 text-[11px] text-slate-800 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-claude-500"
            />
            <div class="max-h-64 overflow-y-auto space-y-0.5">
              <button
                type="button"
                onclick={() => { selectedPlatform = "all"; showPlatformMenu = false; }}
                class="w-full flex items-center justify-between px-2 py-1.5 rounded-lg text-[11px] transition cursor-pointer {selectedPlatform === 'all'
                  ? 'bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 font-semibold'
                  : 'text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800'}"
              >
                <span>{t("all_platforms")}</span>
                <span class="text-[10px] font-mono text-slate-400">{stats?.total_sessions ?? 0}</span>
              </button>
              {#each visiblePlatformOptions as plat (plat.id)}
                {@const pCount = stats?.platform_distribution[plat.id]?.count ?? 0}
                <button
                  type="button"
                  onclick={() => { selectedPlatform = plat.id; showPlatformMenu = false; }}
                  class="w-full flex items-center justify-between px-2 py-1.5 rounded-lg text-[11px] transition cursor-pointer {selectedPlatform === plat.id
                    ? 'bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 font-semibold'
                    : 'text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800'}"
                >
                  <span class="flex items-center gap-2 min-w-0">
                    <span class="h-2 w-2 rounded-full {plat.dot} shrink-0"></span>
                    <span class="truncate">{plat.label}</span>
                  </span>
                  {#if pCount > 0}
                    <span class="text-[10px] font-mono text-slate-400">{pCount}</span>
                  {/if}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </div>

    <!-- Secondary Controls Row: Select All + Expand/Collapse on Left, View + Sort + Filter on Right -->
    <div class="flex items-center justify-between text-xs text-slate-600 dark:text-slate-300 gap-1.5 min-w-0">
      <!-- Left: Master Select All & Tree Expand/Collapse -->
      <div class="flex items-center gap-1.5 min-w-0">
        <button
          onclick={toggleSelectAll}
          disabled={allVisibleSessions.length === 0}
          class="flex items-center gap-1 px-1.5 py-0.5 rounded-md text-xs font-medium text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800 hover:text-slate-900 dark:hover:text-white transition cursor-pointer select-none whitespace-nowrap disabled:opacity-40 disabled:cursor-not-allowed"
          title={isAllSelected ? t("deselect_all") : t("select_all")}
        >
          {#if isAllSelected}
            <CheckSquare class="h-3.5 w-3.5 text-claude-600 shrink-0" />
            <span class="text-[11px] font-semibold text-claude-700 dark:text-claude-400 whitespace-nowrap">{t("select_all")} ({allVisibleSessions.length})</span>
          {:else if selectedVisibleCount > 0}
            <MinusSquare class="h-3.5 w-3.5 text-claude-600 shrink-0" />
            <span class="text-[11px] font-semibold text-slate-800 dark:text-slate-100 whitespace-nowrap">{t("selected_count", { n: selectedVisibleCount })}/{allVisibleSessions.length}</span>
          {:else}
            <Square class="h-3.5 w-3.5 text-slate-400 shrink-0" />
            <span class="text-[11px] text-slate-600 dark:text-slate-300 whitespace-nowrap">{t("select_all")} ({allVisibleSessions.length})</span>
          {/if}
        </button>

        {#if viewMode === "tree"}
          <div class="flex items-center gap-1 text-[11px] text-slate-400 pl-1 border-l border-slate-200 dark:border-slate-700 whitespace-nowrap">
            <button onclick={expandAll} class="hover:text-claude-600 transition cursor-pointer hover:underline">{t("expand")}</button>
            <span class="text-slate-300 dark:text-slate-600">/</span>
            <button onclick={collapseAll} class="hover:text-claude-600 transition cursor-pointer hover:underline">{t("collapse")}</button>
          </div>
        {/if}
      </div>

      <!-- Right Action Group: View Switcher + Sort + Filter -->
      <div class="flex items-center gap-1 shrink-0">
        <!-- View Mode: Tree vs Flat -->
        <div class="flex items-center bg-slate-100 dark:bg-slate-800 p-0.5 rounded-lg border border-slate-200 dark:border-slate-700 shrink-0">
          <button
            onclick={() => (viewMode = "tree")}
            class="p-1 rounded text-xs transition cursor-pointer {viewMode === 'tree'
              ? 'bg-white dark:bg-slate-700 text-claude-700 dark:text-claude-300 shadow-2xs font-semibold'
              : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
            title={t("view_tree")}
          >
            <ListTree class="h-3.5 w-3.5" />
          </button>
          <button
            onclick={() => (viewMode = "flat")}
            class="p-1 rounded text-xs transition cursor-pointer {viewMode === 'flat'
              ? 'bg-white dark:bg-slate-700 text-claude-700 dark:text-claude-300 shadow-2xs font-semibold'
              : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
            title={t("view_flat")}
          >
            <List class="h-3.5 w-3.5" />
          </button>
        </div>

        <!-- Sort Dropdown -->
        <div class="flex items-center bg-slate-50 dark:bg-slate-800 border border-slate-200 dark:border-slate-700 px-1.5 py-0.5 rounded-lg text-[11px] shrink-0">
          <ArrowUpDown class="h-3 w-3 text-slate-400 mr-1" />
          <select
            bind:value={sortBy}
            class="bg-transparent text-[11px] text-slate-700 dark:text-slate-200 focus:outline-none cursor-pointer pr-1"
          >
            <option value="updated_desc">{t("sort_updated")}</option>
            <option value="created_asc">{t("sort_created")}</option>
            <option value="turns_desc">{t("sort_turns")}</option>
            <option value="size_desc">{t("sort_size")}</option>
          </select>
        </div>

        <!-- Filter Toggle Button (With Active Badge) -->
        <button
          onclick={() => (showFilterPopover = !showFilterPopover)}
          class="relative p-1 rounded-lg border text-xs transition cursor-pointer shrink-0 {showFilterPopover
            ? 'bg-claude-50 dark:bg-claude-950/70 border-claude-300 dark:border-claude-700 text-claude-700 dark:text-claude-300'
            : activeFilterCount > 0
              ? 'bg-claude-50 dark:bg-claude-950/70 border-claude-200 dark:border-claude-800 text-claude-700 dark:text-claude-300'
              : 'bg-white dark:bg-slate-800 border-slate-200 dark:border-slate-700 text-slate-600 dark:text-slate-300 hover:bg-slate-50 dark:hover:bg-slate-700'}"
          title={t("active_filter")}
        >
          <Filter class="h-3.5 w-3.5" />
          {#if activeFilterCount > 0}
            <span class="absolute -top-1 -right-1 h-3.5 w-3.5 bg-claude-600 text-white text-[9px] font-bold rounded-full flex items-center justify-center">
              {activeFilterCount}
            </span>
          {/if}
        </button>
      </div>
    </div>

    <!-- Filter Popover Panel (Collapsible) -->
    {#if showFilterPopover}
      <div class="pt-2 border-t border-slate-100 dark:border-slate-800 flex flex-col gap-2.5 text-[11px] text-slate-600 dark:text-slate-300 bg-slate-50/90 dark:bg-slate-800/70 p-2.5 rounded-lg border border-slate-200/80 dark:border-slate-700 animate-in slide-in-from-top-1 duration-100">
        <!-- Top Row: Workspace & Kind Filters -->
        <div class="flex items-center gap-2">
          <!-- Workspace Filter -->
          <div class="flex-1 min-w-0">
            <span class="text-slate-500 dark:text-slate-400 font-medium block text-[10px] mb-1">{t("workspace")}</span>
            <select
              bind:value={selectedWorkspace}
              class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md px-2 py-1 text-[11px] text-slate-800 dark:text-slate-100 focus:outline-none focus:border-claude-500 focus:ring-1 focus:ring-claude-500 cursor-pointer shadow-2xs"
            >
              <option value="all">{t("all_workspaces", { n: workspaces.length })}</option>
              {#each workspaces as ws}
                <option value={ws}>{ws.split("/").pop() || ws}</option>
              {/each}
            </select>
          </div>

          <!-- Kind Filter -->
          <div class="w-24 shrink-0">
            <span class="text-slate-500 dark:text-slate-400 font-medium block text-[10px] mb-1">{t("hierarchy")}</span>
            <select
              bind:value={selectedKind}
              class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md px-2 py-1 text-[11px] text-slate-800 dark:text-slate-100 focus:outline-none focus:border-claude-500 focus:ring-1 focus:ring-claude-500 cursor-pointer shadow-2xs"
            >
              <option value="all">{t("all_hierarchy")}</option>
              <option value="main">{t("main_session")}</option>
              <option value="sub">{t("subagent_session")}</option>
            </select>
          </div>
        </div>

        <!-- Bottom Section: Time Range Section (Quick Presets + Date Range Pickers) -->
        <div class="pt-2 border-t border-slate-200/70 dark:border-slate-700 space-y-2">
          <div class="flex items-center justify-between">
            <div class="flex items-center gap-1 text-[10px] font-medium text-slate-600 dark:text-slate-300">
              <Calendar class="h-3 w-3 text-slate-500 dark:text-slate-400" />
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
                  ? 'bg-claude-600 text-white border-claude-600 font-medium shadow-2xs'
                  : 'bg-white dark:bg-slate-900 hover:bg-slate-100 dark:hover:bg-slate-800 text-slate-600 dark:text-slate-300 border-slate-200 dark:border-slate-700'}"
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
                class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md px-1.5 py-1 text-[11px] text-slate-800 dark:text-slate-100 focus:outline-none focus:border-claude-500 focus:ring-1 focus:ring-claude-500 font-mono shadow-2xs cursor-pointer"
              />
            </div>
            <span class="text-slate-400 text-[11px] shrink-0 font-medium">{t("to_date")}</span>
            <div class="flex-1 min-w-0">
              <input
                type="date"
                bind:value={customEndDate}
                min={customStartDate || undefined}
                onchange={handleCustomDateInput}
                class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md px-1.5 py-1 text-[11px] text-slate-800 dark:text-slate-100 focus:outline-none focus:border-claude-500 focus:ring-1 focus:ring-claude-500 font-mono shadow-2xs cursor-pointer"
              />
            </div>
            {#if customStartDate || customEndDate}
              <button
                onclick={() => applyTimePreset("all")}
                class="p-1 text-slate-400 hover:text-rose-600 rounded hover:bg-slate-100 dark:hover:bg-slate-800 transition cursor-pointer shrink-0"
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
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-claude-50 dark:bg-claude-950/70 text-claude-800 dark:text-claude-300 border border-claude-200 dark:border-claude-800 whitespace-nowrap">
            <span>{t("workspace_chip_prefix")}{selectedWorkspace.split("/").pop() || selectedWorkspace}</span>
            <button onclick={() => (selectedWorkspace = "all")} class="hover:text-claude-950 dark:hover:text-claude-100 cursor-pointer">
              <X class="h-2.5 w-2.5" />
            </button>
          </span>
        {/if}

        {#if isTimeFilterActive}
          <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-md bg-claude-50 dark:bg-claude-950/70 text-claude-800 dark:text-claude-300 border border-claude-200 dark:border-claude-800 whitespace-nowrap">
            <span>{t("time_chip_prefix")}{timeFilterChipLabel}</span>
            <button onclick={() => applyTimePreset("all")} class="hover:text-claude-950 dark:hover:text-claude-100 cursor-pointer">
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

  <div class="px-3.5 py-1.5 bg-slate-100/70 dark:bg-slate-900/70 border-b border-slate-200/80 dark:border-slate-800 flex items-center text-xs text-slate-500 dark:text-slate-400 shrink-0 select-none whitespace-nowrap">
    <div class="flex items-center gap-2 whitespace-nowrap shrink-0">
      <span class="font-semibold text-slate-700 dark:text-slate-200 text-[11px] whitespace-nowrap">
        {visibleTelemetry.count} {t("sessions_suffix")}
      </span>
      {#if visibleTelemetry.runningCount > 0}
        <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded-full text-[10px] font-medium bg-emerald-50 dark:bg-emerald-950/70 text-emerald-700 dark:text-emerald-300 border border-emerald-200/80 dark:border-emerald-800 whitespace-nowrap">
          <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
          <span>{visibleTelemetry.runningCount} {t("running")}</span>
        </span>
      {/if}
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
              ? 'bg-claude-50/80 dark:bg-claude-950/50 border-claude-400 dark:border-claude-600 shadow-sm ring-1 ring-claude-300/80 dark:ring-claude-700/60'
              : 'bg-white dark:bg-slate-900 hover:bg-slate-50/90 dark:hover:bg-slate-800/80 border-slate-200/90 dark:border-slate-800 hover:border-slate-300 dark:hover:border-slate-700 shadow-2xs'}"
          >
            <!-- Top Row: Checkbox, Platform Badge, Model, Status, Subagents Pill, Star -->
            <div class="flex items-center justify-between gap-1.5 mb-1">
              <div class="flex items-center gap-1.5 flex-wrap min-w-0">
                <button
                  onclick={(e) => toggleSelect(node.session.id, e)}
                  class="text-slate-400 hover:text-claude-600 p-0.5 shrink-0"
                  title={t("checkbox_select_tip")}
                >
                  {#if getSelectionState(node.session.id) === "all"}
                    <CheckSquare class="h-3.5 w-3.5 text-claude-600" />
                  {:else if getSelectionState(node.session.id) === "partial"}
                    <MinusSquare class="h-3.5 w-3.5 text-claude-600" />
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

                {#if node.session.is_archived}
                  <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-violet-50 text-violet-700 border border-violet-200">
                    <Archive class="h-2.5 w-2.5" />
                    {t("archived")}
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
                    class="flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-claude-50 dark:bg-claude-950/70 hover:bg-claude-100 dark:hover:bg-claude-900/70 text-claude-700 dark:text-claude-300 border border-claude-200 dark:border-claude-800 transition cursor-pointer"
                    title={t("expand") + "/" + t("collapse")}
                  >
                    <Sparkles class="h-2.5 w-2.5 text-claude-600 dark:text-claude-400" />
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
            <h3 class="text-xs font-semibold text-slate-900 dark:text-slate-100 line-clamp-2 leading-snug mb-1 group-hover:text-claude-700 dark:group-hover:text-claude-400 transition">
              {node.session.title || t("untitled_session")}
            </h3>

            <div class="flex items-center text-[11px] text-slate-500 gap-1.5 pt-0.5">
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
            </div>
          </div>

          <!-- Mounted Subagents Container (Collapsible) -->
          {#if node.subagents.length > 0 && !collapsedParents.has(node.session.id)}
            <div class="ml-3.5 pl-2.5 border-l-2 border-claude-200/90 dark:border-claude-800 space-y-1 py-0.5">
              {#each node.subagents as sub (sub.id)}
                <div
                  role="button"
                  tabindex="0"
                  onclick={() => onSelectSession(sub)}
                  onkeydown={(e) => e.key === "Enter" && onSelectSession(sub)}
                  class="group relative p-2 rounded-lg border text-left transition cursor-pointer {selectedSessionId === sub.id
                    ? 'bg-claude-50/90 dark:bg-claude-950/60 border-claude-400 dark:border-claude-600 shadow-xs ring-1 ring-claude-300 dark:ring-claude-700'
                    : 'bg-white dark:bg-slate-900 hover:bg-slate-50 dark:hover:bg-slate-800 border-slate-200/80 dark:border-slate-800 hover:border-slate-300 shadow-2xs'}"
                >
                  <div class="flex items-center justify-between gap-1 mb-1">
                    <div class="flex items-center gap-1.5 flex-wrap">
                      <button
                        onclick={(e) => toggleSelect(sub.id, e)}
                        class="text-slate-400 hover:text-claude-600 p-0.5"
                      >
                        {#if selectedIds.has(sub.id)}
                          <CheckSquare class="h-3 w-3 text-claude-600" />
                        {:else}
                          <Square class="h-3 w-3 text-slate-300 group-hover:text-slate-400" />
                        {/if}
                      </button>

                      <span class="inline-flex items-center gap-0.5 px-1.5 py-0.2 rounded text-[9px] font-bold bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 border border-claude-200 dark:border-claude-800">
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

                  <h4 class="text-[11px] font-medium text-slate-800 dark:text-slate-100 line-clamp-1 group-hover:text-claude-700 dark:group-hover:text-claude-400 transition">
                    {sub.title || t("untitled_subagent")}
                  </h4>

                  <div class="flex items-center justify-between text-[10px] text-slate-400 mt-0.5 font-mono">
                    <span class="truncate max-w-[130px] text-slate-500 font-sans">
                      {sub.flavor || sub.dirname}
                    </span>
                    <div class="flex items-center gap-1.5 flex-wrap justify-end">
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
            ? 'bg-claude-50/80 dark:bg-claude-950/50 border-claude-400 dark:border-claude-600 shadow-sm ring-1 ring-claude-300/80 dark:ring-claude-700/60'
            : 'bg-white dark:bg-slate-900 hover:bg-slate-50/90 dark:hover:bg-slate-800/80 border-slate-200/90 dark:border-slate-800 hover:border-slate-300 dark:hover:border-slate-700 shadow-2xs'}"
        >
          <!-- Top Row -->
          <div class="flex items-center justify-between gap-1.5 mb-1">
            <div class="flex items-center gap-1.5 flex-wrap min-w-0">
              <button
                onclick={(e) => toggleSelect(s.id, e)}
                class="text-slate-400 hover:text-claude-600 p-0.5"
              >
                {#if getSelectionState(s.id) === "all"}
                  <CheckSquare class="h-3.5 w-3.5 text-claude-600" />
                {:else if getSelectionState(s.id) === "partial"}
                  <MinusSquare class="h-3.5 w-3.5 text-claude-600" />
                {:else}
                  <Square class="h-3.5 w-3.5 text-slate-300 group-hover:text-slate-400" />
                {/if}
              </button>

              <span class="px-1.5 py-0.2 rounded text-[10px] font-bold uppercase tracking-wider border {getPlatformBadge(s.platform)}">
                {s.platform}
              </span>

              {#if s.is_subagent}
                <span class="inline-flex items-center gap-0.5 px-1.5 py-0.2 rounded text-[10px] font-medium bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 border border-claude-200 dark:border-claude-800">
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

              {#if s.is_archived}
                <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded text-[10px] bg-violet-50 text-violet-700 border border-violet-200">
                  <Archive class="h-2.5 w-2.5" />
                  {t("archived")}
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
          <h3 class="text-xs font-semibold text-slate-900 dark:text-slate-100 line-clamp-2 leading-snug mb-1 group-hover:text-claude-700 dark:group-hover:text-claude-400 transition">
            {s.title || t("untitled_session")}
          </h3>

          <div class="flex items-center text-[11px] text-slate-500 gap-1.5 pt-0.5">
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
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>
