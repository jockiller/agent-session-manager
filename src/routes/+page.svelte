<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { ChatMessage, CleanupResult, GlobalStats, SessionSummary, SessionTreeNode } from "$lib/types";
  import Sidebar from "$lib/components/Sidebar.svelte";
  import SessionList from "$lib/components/SessionList.svelte";
  import SessionInspector from "$lib/components/SessionInspector.svelte";
  import CleanupModal from "$lib/components/CleanupModal.svelte";
  import AboutModal from "$lib/components/AboutModal.svelte";
  import { Star, Download, Trash2, X, Globe, Info } from "@lucide/svelte";
  import { formatTokens, parseLocalDateStart, parseLocalDateEnd } from "$lib/utils";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { t, i18n, supportedLocales, type Locale } from "$lib/i18n";

  async function handleTitlebarMouseDown(e: MouseEvent) {
    if (e.buttons === 1) {
      try {
        const win = getCurrentWindow();
        await win.startDragging();
      } catch (err) {
        console.warn("startDragging error:", err);
      }
    }
  }

  async function handleTitlebarDblClick() {
    try {
      const win = getCurrentWindow();
      await win.toggleMaximize();
    } catch (err) {
      console.warn("toggleMaximize error:", err);
    }
  }

  let sessions = $state<SessionSummary[]>([]);
  let stats = $state<GlobalStats | null>(null);

  // Filter & Search states
  let searchQuery = $state("");
  let selectedPlatform = $state("all");
  let selectedFilter = $state<"all" | "running" | "starred" | "orphans">("all");
  let selectedWorkspace = $state("all");
  let selectedTimeRange = $state("all");
  let customStartDate = $state("");
  let customEndDate = $state("");
  let selectedKind = $state("all");
  let viewMode = $state<"tree" | "flat">("tree");
  let sortBy = $state("updated_desc");

  // Active selection & Starred items
  let selectedSession = $state<SessionSummary | null>(null);
  let messages = $state<ChatMessage[]>([]);
  let selectedIds = $state<Set<string>>(new Set());
  let starredIds = $state<Set<string>>(new Set());

  // Loading & Cleanup Modal states
  let isLoading = $state(false);
  let isLoadingMessages = $state(false);
  let showCleanupModal = $state(false);
  let isCleaningModal = $state(false);
  let showAboutModal = $state(false);

  // Workspaces list
  let workspaces = $derived(
    Array.from(
      new Set(
        sessions
          .map((s) => s.cwd || s.dirname)
          .filter((w) => Boolean(w && w.trim() && w !== "/"))
      )
    ).sort()
  );

  // Starred persistent management
  function loadStarred() {
    try {
      const saved = localStorage.getItem("asm_starred_sessions");
      if (saved) {
        starredIds = new Set(JSON.parse(saved));
      }
    } catch (e) {
      console.error(e);
    }
  }

  function toggleStar(id: string, e?: Event) {
    if (e) e.stopPropagation();
    const next = new Set(starredIds);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    starredIds = next;
    try {
      localStorage.setItem("asm_starred_sessions", JSON.stringify(Array.from(next)));
    } catch (err) {
      console.error(err);
    }
  }

  // Filtered sessions
  let filteredSessions = $derived.by(() => {
    const now = Date.now();
    const dayMs = 86400000;

    let res = sessions.filter((s) => {
      // Platform filter
      if (selectedPlatform !== "all" && s.platform !== selectedPlatform) {
        return false;
      }

      // Quick view filter
      if (selectedFilter === "running" && !s.is_running) return false;
      if (selectedFilter === "starred" && !starredIds.has(s.id)) return false;
      if (selectedFilter === "orphans" && s.has_transcript) return false;

      // Workspace filter
      if (selectedWorkspace !== "all") {
        const ws = s.cwd || s.dirname;
        if (ws !== selectedWorkspace) return false;
      }

      // Kind filter
      if (selectedKind === "main" && s.is_subagent) return false;
      if (selectedKind === "sub" && !s.is_subagent) return false;

      // Time range filter: custom start/end dates OR relative preset
      if (customStartDate || customEndDate) {
        if (customStartDate) {
          const startMs = parseLocalDateStart(customStartDate);
          if (Number.isFinite(startMs) && s.updated_at < startMs) return false;
        }
        if (customEndDate) {
          const endMs = parseLocalDateEnd(customEndDate);
          if (Number.isFinite(endMs) && s.updated_at > endMs) return false;
        }
      } else if (selectedTimeRange !== "all") {
        if (selectedTimeRange === "today" && now - s.updated_at > dayMs) return false;
        if (selectedTimeRange === "yesterday") {
          const startOfToday = new Date().setHours(0, 0, 0, 0);
          const startOfYesterday = startOfToday - dayMs;
          if (s.updated_at < startOfYesterday || s.updated_at >= startOfToday) return false;
        }
        if (selectedTimeRange === "3d" && now - s.updated_at > 3 * dayMs) return false;
        if (selectedTimeRange === "7d" && now - s.updated_at <= 7 * dayMs) return false;
        if (selectedTimeRange === "30d" && now - s.updated_at <= 30 * dayMs) return false;
        if (selectedTimeRange === "older30d" && now - s.updated_at <= 30 * dayMs) return false;
      }

      // Search Query: multi-term matching across title, directory/cwd, id, model/flavor, platform
      if (searchQuery.trim()) {
        const terms = searchQuery.toLowerCase().trim().split(/\s+/).filter(Boolean);
        const titleStr = s.title?.toLowerCase() || "";
        const cwdStr = (s.cwd || s.dirname)?.toLowerCase() || "";
        const idStr = s.id?.toLowerCase() || "";
        const flavorStr = s.flavor?.toLowerCase() || "";
        const platformStr = s.platform?.toLowerCase() || "";

        const matchesAllTerms = terms.every(
          (t) =>
            titleStr.includes(t) ||
            cwdStr.includes(t) ||
            idStr.includes(t) ||
            flavorStr.includes(t) ||
            platformStr.includes(t)
        );
        if (!matchesAllTerms) return false;
      }

      return true;
    });

    // Sorting
    res.sort((a, b) => {
      if (sortBy === "updated_desc") return b.updated_at - a.updated_at;
      if (sortBy === "created_asc") return a.created_at - b.created_at;
      if (sortBy === "size_desc") return b.size_bytes - a.size_bytes;
      if (sortBy === "turns_desc") return b.turn_count - a.turn_count;
      if (sortBy === "tokens_desc") {
        const ta = a.token_stats?.total_tokens || 0;
        const tb = b.token_stats?.total_tokens || 0;
        return tb - ta;
      }
      return b.updated_at - a.updated_at;
    });

    return res;
  });

  // Build hierarchical tree nodes
  let treeNodes = $derived.by(() => {
    const allMap = new Map<string, SessionSummary>();
    for (const s of sessions) {
      allMap.set(s.id, s);
    }

    const childrenByParent = new Map<string, SessionSummary[]>();
    for (const s of sessions) {
      if (s.is_subagent && s.parent_id) {
        const list = childrenByParent.get(s.parent_id) || [];
        list.push(s);
        childrenByParent.set(s.parent_id, list);
      }
    }

    const attachedSubagentIds = new Set<string>();
    const nodes: SessionTreeNode[] = [];

    if (selectedKind === "sub") {
      for (const s of filteredSessions) {
        nodes.push({ session: s, subagents: [] });
      }
      return nodes;
    }

    for (const s of filteredSessions) {
      if (!s.is_subagent) {
        const subagents = selectedKind === "main" ? [] : (childrenByParent.get(s.id) || []);
        subagents.sort((a, b) => b.updated_at - a.updated_at);
        for (const sub of subagents) {
          attachedSubagentIds.add(sub.id);
        }
        nodes.push({ session: s, subagents });
      } else {
        const parent = s.parent_id ? allMap.get(s.parent_id) : null;
        const parentInFiltered = parent ? filteredSessions.some((p) => p.id === parent.id) : false;

        if (!parentInFiltered && !attachedSubagentIds.has(s.id)) {
          nodes.push({ session: s, subagents: [] });
          attachedSubagentIds.add(s.id);
        }
      }
    }

    return nodes;
  });

  // Associated parent & children of currently selected session
  let parentSession = $derived(
    selectedSession && selectedSession.is_subagent && selectedSession.parent_id
      ? sessions.find((s) => s.id === selectedSession?.parent_id) || null
      : null
  );

  let childSubagents = $derived(
    selectedSession
      ? sessions.filter((s) => s.is_subagent && s.parent_id === selectedSession?.id)
      : []
  );

  async function refreshAll() {
    isLoading = true;
    try {
      const data = await invoke<SessionSummary[]>("scan_all_sessions");
      sessions = data;

      // Update global stats
      const s = await invoke<GlobalStats>("calculate_global_stats", { sessions: data });
      stats = s;

      // Maintain selection or select first
      if (selectedSession) {
        const found = data.find((item) => item.id === selectedSession?.id);
        if (found) {
          selectedSession = found;
        } else {
          selectedSession = data[0] || null;
          if (data[0]) handleSelectSession(data[0]);
        }
      } else if (data.length > 0) {
        handleSelectSession(data[0]);
      }
    } catch (err) {
      console.error("Failed to scan sessions:", err);
    } finally {
      isLoading = false;
    }
  }

  let activeLoadingSessionId = $state<string | null>(null);

  async function handleSelectSession(s: SessionSummary) {
    selectedSession = s;
    activeLoadingSessionId = s.id;
    isLoadingMessages = true;
    messages = [];
    try {
      const msgs = await invoke<ChatMessage[]>("get_session_messages", {
        session: s,
        maxMsgs: 150,
      });
      if (activeLoadingSessionId === s.id) {
        messages = msgs;
      }
    } catch (err) {
      if (activeLoadingSessionId === s.id) {
        console.error("Failed to load messages:", err);
      }
    } finally {
      if (activeLoadingSessionId === s.id) {
        isLoadingMessages = false;
      }
    }
  }

  let selectedSessionsList = $derived(
    sessions.filter((s) => selectedIds.has(s.id))
  );

  async function handleDeleteSingle(s: SessionSummary) {
    try {
      await invoke("execute_cleanup_cmd", {
        allSessions: sessions,
        targetIds: [s.id],
        force: true,
      });
      selectedIds.delete(s.id);
      refreshAll();
    } catch (err) {
      console.error("Failed to delete session:", err);
    }
  }

  function handleOpenCleanup() {
    showCleanupModal = true;
  }

  async function handleConfirmCleanup(force: boolean) {
    if (selectedIds.size === 0 || isCleaningModal) return;
    isCleaningModal = true;
    try {
      const targetSessions = force
        ? selectedSessionsList
        : selectedSessionsList.filter((s) => !s.is_running);
      const targetIds = targetSessions.map((s) => s.id);

      if (targetIds.length === 0) {
        showCleanupModal = false;
        return;
      }

      await invoke<CleanupResult>("execute_cleanup_cmd", {
        allSessions: sessions,
        targetIds,
        force,
      });

      const next = new Set(selectedIds);
      for (const id of targetIds) {
        next.delete(id);
      }
      selectedIds = next;

      if (selectedSession && targetIds.includes(selectedSession.id)) {
        selectedSession = null;
        messages = [];
      }

      await refreshAll();
      showCleanupModal = false;
    } catch (err) {
      console.error("Cleanup failed:", err);
    } finally {
      isCleaningModal = false;
    }
  }

  // Bulk actions
  function bulkToggleStar() {
    const next = new Set(starredIds);
    let allSelectedStarred = true;
    for (const id of selectedIds) {
      if (!next.has(id)) {
        allSelectedStarred = false;
        break;
      }
    }
    for (const id of selectedIds) {
      if (allSelectedStarred) {
        next.delete(id);
      } else {
        next.add(id);
      }
    }
    starredIds = next;
    try {
      localStorage.setItem("asm_starred_sessions", JSON.stringify(Array.from(next)));
    } catch (e) {
      console.error(e);
    }
  }

  function bulkExportJson() {
    const selectedList = sessions.filter((s) => selectedIds.has(s.id));
    const payload = {
      sessions: selectedList,
      count: selectedList.length,
      exported_at: new Date().toISOString(),
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `agent_sessions_export_${selectedList.length}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  let isMac = $state(true);

  onMount(() => {
    if (typeof navigator !== "undefined") {
      isMac = /Mac|iPhone|iPod|iPad/i.test(navigator.userAgent);
    }
    loadStarred();
    refreshAll();
  });
</script>

<div class="h-screen w-screen flex flex-col overflow-hidden bg-slate-50 text-slate-800 font-sans antialiased relative">
  <!-- Top Drag Titlebar (Seamless with App Body, OS-adaptive) -->
  <div
    class="h-8 w-full shrink-0 flex items-center justify-between select-none bg-slate-100/80 border-b border-slate-200/90 {isMac ? 'pl-20' : 'pl-4'} pr-4 text-xs cursor-default"
    style="-webkit-app-region: drag; -webkit-user-select: none;"
    data-tauri-drag-region
    onmousedown={handleTitlebarMouseDown}
    ondblclick={handleTitlebarDblClick}
    role="toolbar"
    tabindex="-1"
  >
    <div class="flex items-center gap-2 pointer-events-none" data-tauri-drag-region>
      <span class="text-xs font-semibold text-slate-700 tracking-tight">{t("app_title")}</span>
      <span class="text-[10px] text-slate-400 font-normal">{t("app_subtitle")}</span>
    </div>
    <div class="flex items-center gap-2.5 text-[11px] text-slate-500 font-mono" data-tauri-drag-region>
      {#if stats}
        <span class="pointer-events-none">{stats.total_sessions} {t("sessions_suffix")}</span>
        <span class="pointer-events-none text-slate-300">·</span>
        <span class="pointer-events-none">{formatTokens(stats.total_tokens)} tok</span>
        <span class="pointer-events-none text-slate-300">·</span>
      {/if}

      <!-- Native Language Selector (Interactive without blocking window drag) -->
      <div
        class="relative flex items-center group shrink-0"
        style="-webkit-app-region: no-drag; pointer-events: auto;"
      >
        <div class="flex items-center gap-1.5 px-2 py-0.5 rounded-md hover:bg-slate-200/80 text-slate-600 hover:text-slate-900 transition cursor-pointer text-[11px] font-sans font-medium border border-transparent hover:border-slate-300/60 shadow-2xs">
          <Globe class="h-3 w-3 text-slate-500 group-hover:text-sky-600 transition" />
          <span>{supportedLocales.find((l) => l.id === i18n.locale)?.label || "Language"}</span>
        </div>
        <select
          value={i18n.locale}
          onchange={(e) => i18n.setLocale(e.currentTarget.value as Locale)}
          class="absolute inset-0 opacity-0 cursor-pointer w-full h-full text-xs"
          title={t("lang_switcher")}
        >
          {#each supportedLocales as loc}
            <option value={loc.id}>{loc.label}</option>
          {/each}
        </select>
      </div>

      <!-- Info / About Button -->
      <button
        type="button"
        onclick={() => (showAboutModal = true)}
        class="flex items-center justify-center p-1 rounded-md hover:bg-slate-200/80 text-slate-500 hover:text-slate-900 transition cursor-pointer border border-transparent hover:border-slate-300/60 shadow-2xs shrink-0"
        style="-webkit-app-region: no-drag; pointer-events: auto;"
        title={t("about_app")}
      >
        <Info class="h-3.5 w-3.5 text-slate-500 hover:text-sky-600 transition" />
      </button>
    </div>
  </div>

  <!-- 3-Pane Body -->
  <div class="flex-1 flex overflow-hidden">
    <!-- Left Navigation Sidebar (Platform & Quick Views) -->
    <Sidebar
      bind:selectedPlatform
      bind:selectedFilter
      {stats}
      {isLoading}
      starredCount={starredIds.size}
      onRefresh={refreshAll}
    />

    <!-- Middle Panel: Session Card List with Search & Sort Toolbar & Linked Telemetry -->
    <SessionList
      {treeNodes}
      flatSessions={filteredSessions}
      allSessions={sessions}
      bind:viewMode
      bind:searchQuery
      bind:sortBy
      bind:selectedWorkspace
      bind:selectedTimeRange
      bind:customStartDate
      bind:customEndDate
      bind:selectedKind
      {workspaces}
      selectedSessionId={selectedSession?.id ?? null}
      bind:selectedIds
      {starredIds}
      onSelectSession={handleSelectSession}
      onToggleStar={toggleStar}
    />

    <!-- Right Panel: Session Inspector & Chat Timeline -->
    <SessionInspector
      session={selectedSession}
      {messages}
      {isLoadingMessages}
      {parentSession}
      {childSubagents}
      isStarred={selectedSession ? starredIds.has(selectedSession.id) : false}
      onDeleteSingle={handleDeleteSingle}
      onToggleStar={toggleStar}
      onJumpSession={handleSelectSession}
    />
  </div>

  <!-- Floating Bulk Actions Bar (Shown when multiple items are selected) -->
  {#if selectedIds.size > 0}
    <div class="absolute bottom-5 left-1/2 -translate-x-1/2 z-30 bg-white/95 backdrop-blur-md border border-slate-300 rounded-2xl shadow-xl px-4 py-2 flex items-center gap-3 text-xs text-slate-700 animate-in fade-in slide-in-from-bottom-2 duration-150">
      <span class="font-bold text-slate-900 flex items-center gap-1.5">
        <span class="h-2 w-2 rounded-full bg-sky-500"></span>
        {t("selected_count", { n: selectedIds.size })}
      </span>

      <div class="h-4 w-px bg-slate-200"></div>

      <button
        onclick={bulkToggleStar}
        class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-amber-50 hover:bg-amber-100 text-amber-800 border border-amber-200 font-medium transition cursor-pointer"
      >
        <Star class="h-3.5 w-3.5 text-amber-600 fill-amber-500" />
        <span>{t("bulk_star")}</span>
      </button>

      <button
        onclick={bulkExportJson}
        class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-slate-100 hover:bg-slate-200 text-slate-700 font-medium transition cursor-pointer"
      >
        <Download class="h-3.5 w-3.5 text-slate-600" />
        <span>{t("bulk_export")}</span>
      </button>

      <button
        onclick={handleOpenCleanup}
        class="flex items-center gap-1 px-2.5 py-1 rounded-lg bg-rose-50 hover:bg-rose-100 text-rose-700 border border-rose-200 font-medium transition cursor-pointer"
      >
        <Trash2 class="h-3.5 w-3.5 text-rose-600" />
        <span>{t("move_to_trash")}</span>
      </button>

      <button
        onclick={() => (selectedIds = new Set())}
        class="p-1 rounded-lg hover:bg-slate-100 text-slate-400 hover:text-slate-600 transition cursor-pointer ml-1"
        title={t("deselect_all")}
      >
        <X class="h-3.5 w-3.5" />
      </button>
    </div>
  {/if}

  <!-- Cleanup Confirmation Modal -->
  <CleanupModal
    isOpen={showCleanupModal}
    selectedSessions={selectedSessionsList}
    isCleaning={isCleaningModal}
    onClose={() => (showCleanupModal = false)}
    onConfirm={handleConfirmCleanup}
  />

  <!-- About App Modal -->
  <AboutModal
    isOpen={showAboutModal}
    onClose={() => (showAboutModal = false)}
  />
</div>
