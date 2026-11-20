<script lang="ts">
  import {
    Folder,
    Terminal,
    Trash2,
    Copy,
    Check,
    Clock,
    Wrench,
    Sparkles,
    User,
    Bot,
    ChevronDown,
    ChevronRight,
    ExternalLink,
    Search,
    Download,
    FileText,
    Star,
    CornerDownRight,
    ArrowLeft,
    Play,
    Database,
    X,
    MessageSquare,
    LayoutDashboard,
    MoreHorizontal,
    Activity,
    AppWindow,
  } from "@lucide/svelte";
  import type { ChatMessage, SessionSummary } from "../types";
  import { formatBytes, formatTime, getPlatformBadge } from "../utils";
  import { buildResumeCommand, getResumeAction } from "$lib/platforms/registry";
  import { appStore } from "$lib/stores/app.svelte";
  import { renderMarkdown } from "$lib/utils/markdown";
  import { formatCompactTokens, formatMoney, usageStore } from "$lib/usage/store.svelte";
  import type { SessionUsageDetail, UsageLogPage } from "$lib/usage/types";
  import UsageLogTable from "./usage/UsageLogTable.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "../i18n";

  let {
    session = null,
    messages = [],
    isLoadingMessages = false,
    parentSession = null,
    childSubagents = [],
    isStarred = false,
    onDeleteSingle,
    onToggleStar,
    onJumpSession,
  }: {
    session: SessionSummary | null;
    messages: ChatMessage[];
    isLoadingMessages: boolean;
    parentSession: SessionSummary | null;
    childSubagents: SessionSummary[];
    isStarred: boolean;
    onDeleteSingle: (s: SessionSummary) => void;
    onToggleStar: (id: string) => void;
    onJumpSession: (s: SessionSummary) => void;
  } = $props();

  let copiedId = $state(false);
  let copiedResume = $state(false);
  let isResuming = $state(false);
  let copiedPath = $state(false);
  let inChatSearch = $state("");
  let expandedThinking = $state<Record<number, boolean>>({});
  let expandedTools = $state<Record<number, boolean>>({});
  let isConfirmingDelete = $state(false);
  let showMoreMenu = $state(false);
  let moreMenuEl: HTMLDivElement | undefined = $state();
  let activeTab = $state<"chat" | "overview" | "logs">("chat");
  let sessionUsage = $state<SessionUsageDetail | null>(null);
  let sessionLogs = $state<UsageLogPage | null>(null);
  let sessionLogsLoading = $state(false);
  let usageKeyLoaded = "";
  let logsKeyLoaded = "";
  let syncTriggered = false;
  let resumeAction = $derived(session ? getResumeAction(session) : null);

  function loadSessionUsageDetail(s: SessionSummary) {
    const key = `${s.platform}:${s.id}`;
    usageKeyLoaded = key;
    void invoke<SessionUsageDetail | null>("usage_session_detail", {
      platform: s.platform,
      sessionId: s.id,
    })
      .then((res) => {
        if (usageKeyLoaded === key) sessionUsage = res;
      })
      .catch(() => {
        if (usageKeyLoaded === key) sessionUsage = null;
      });
  }

  function loadSessionLogs(page = 1) {
    const s = session;
    if (!s) return;
    const key = `${s.platform}:${s.id}`;
    logsKeyLoaded = key;
    sessionLogsLoading = true;
    void invoke<UsageLogPage>("usage_query_logs", {
      filter: { platform: s.platform, sessionId: s.id },
      page,
      pageSize: 50,
    })
      .then((res) => {
        if (logsKeyLoaded === key) sessionLogs = res;
      })
      .catch(() => {
        if (logsKeyLoaded === key) sessionLogs = null;
      })
      .finally(() => {
        if (logsKeyLoaded === key) sessionLogsLoading = false;
      });
  }

  $effect(() => {
    const s = session;
    const tab = activeTab;
    if (!s) {
      sessionUsage = null;
      sessionLogs = null;
      usageKeyLoaded = "";
      logsKeyLoaded = "";
      return;
    }
    const key = `${s.platform}:${s.id}`;
    if ((tab === "overview" || tab === "logs") && usageKeyLoaded !== key) {
      sessionUsage = null;
      loadSessionUsageDetail(s);
    }
    if (tab === "logs" && logsKeyLoaded !== key) {
      loadSessionLogs(1);
    }
    if ((tab === "overview" || tab === "logs") && !syncTriggered) {
      syncTriggered = true;
      void usageStore.syncInBackground().then(() => {
        const cur = session;
        if (cur && `${cur.platform}:${cur.id}` === key) {
          loadSessionUsageDetail(cur);
          if (activeTab === "logs") loadSessionLogs(1);
        }
      });
    }
  });

  function copyMainPath() {
    if (!session || !session.main_path) return;
    navigator.clipboard.writeText(session.main_path);
    copiedPath = true;
    setTimeout(() => (copiedPath = false), 2000);
  }

  $effect(() => {
    if (session) {
      isConfirmingDelete = false;
      showMoreMenu = false;
    }
  });

  // Filter messages by in-session search query
  let visibleMessages = $derived(
    inChatSearch.trim()
      ? messages.filter((m) => {
          const q = inChatSearch.toLowerCase();
          const matchText = m.text?.toLowerCase().includes(q);
          const matchThinking = m.thinking?.toLowerCase().includes(q);
          const matchTool = m.tool_calls?.some(
            (t) => t.name.toLowerCase().includes(q) || t.args?.toLowerCase().includes(q)
          );
          return matchText || matchThinking || matchTool;
        })
      : messages
  );

  function copySessionId() {
    if (!session) return;
    navigator.clipboard.writeText(session.id);
    copiedId = true;
    setTimeout(() => (copiedId = false), 2000);
  }

  function getResumeCommand(s: SessionSummary): string {
    return buildResumeCommand(s);
  }

  function copyResumeCmd() {
    if (!session) return;
    const cmd = getResumeCommand(session);
    navigator.clipboard.writeText(cmd);
    copiedResume = true;
    setTimeout(() => (copiedResume = false), 2000);
  }

  async function handleResume() {
    if (!session || !resumeAction) return;
    try {
      if (resumeAction.kind === "app") {
        isResuming = true;
        await invoke("open_app", {
          appName: resumeAction.appName,
          targetPath: resumeAction.targetPath ?? null,
        });
      } else {
        await navigator.clipboard.writeText(resumeAction.command);
        copiedResume = true;
        isResuming = true;
        await invoke("run_in_terminal", {
          command: resumeAction.command,
          terminal: appStore.preferredTerminal,
        });
      }
    } catch (e) {
      console.error("Failed to resume session:", e);
    } finally {
      setTimeout(() => {
        isResuming = false;
        copiedResume = false;
      }, 2500);
    }
  }

  async function openTerminal() {
    if (!session || !session.cwd) return;
    try {
      await invoke("open_terminal", {
        cwd: session.cwd,
        terminal: appStore.preferredTerminal,
      });
    } catch (e) {
      console.error(e);
    }
  }

  async function revealFinder() {
    if (!session || !session.main_path) return;
    try {
      await invoke("reveal_path", { path: session.main_path });
    } catch (e) {
      console.error(e);
    }
  }

  function toggleThinking(idx: number) {
    expandedThinking = { ...expandedThinking, [idx]: !expandedThinking[idx] };
  }

  function toggleTool(idx: number) {
    expandedTools = { ...expandedTools, [idx]: !expandedTools[idx] };
  }

  function exportMarkdown() {
    if (!session) return;
    let md = `# ${session.title || "Agent Session"}\n\n`;
    md += `- **${t("md_platform")}**: ${session.platform} (${session.flavor || ""})\n`;
    md += `- **${t("md_session_id")}**: \`${session.id}\`\n`;
    md += `- **${t("md_workspace")}**: \`${session.cwd || session.dirname || ""}\`\n`;
    md += `- **${t("updated_time")}**: ${formatTime(session.updated_at)}\n`;
    md += `\n---\n\n`;

    for (let i = 0; i < messages.length; i++) {
      const msg = messages[i];
      if (msg.role === "user") {
        md += `### ${t("user_question")} (${msg.time || ""})\n\n${msg.text}\n\n`;
      } else if (msg.role === "assistant") {
        md += `### ${t("ai_assistant")} (${msg.time || ""})\n\n`;
        if (msg.thinking) {
          md += `> **${t("md_thinking_block")}**:\n>\n> ${msg.thinking.replace(/\n/g, "\n> ")}\n\n`;
        }
        if (msg.tool_calls && msg.tool_calls.length > 0) {
          md += `**${t("md_tool_calls_heading")}**:\n\n`;
          for (const tc of msg.tool_calls) {
            md += `\`\`\`bash\n# ${tc.name}\n${tc.args || ""}\n\`\`\`\n\n`;
          }
        }
        if (msg.text) {
          md += `${msg.text}\n\n`;
        }
      } else if (msg.role === "subagent") {
        md += `> **${t("md_subagent_call")}**: ${msg.text}\n\n`;
      }
    }

    const blob = new Blob([md], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    const safeTitle = (session.title || session.id).slice(0, 30).replace(/[^a-zA-Z0-9_\u4e00-\u9fa5-]/g, "_");
    a.href = url;
    a.download = `session_${safeTitle}.md`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function exportJson() {
    if (!session) return;
    const payload = {
      session,
      messages,
      exported_at: new Date().toISOString(),
    };
    const blob = new Blob([JSON.stringify(payload, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `session_${session.id.slice(0, 12)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }
</script>

<svelte:window
  onclick={(e) => {
    if (showMoreMenu && moreMenuEl && !moreMenuEl.contains(e.target as Node)) {
      showMoreMenu = false;
    }
  }}
  onkeydown={(e) => {
    if (e.key === "Escape" && showMoreMenu) {
      showMoreMenu = false;
    }
  }}
/>

<div class="flex-1 flex flex-col h-full bg-slate-50/30 dark:bg-slate-950/60 overflow-hidden">
  {#if !session}
    <div class="h-full flex flex-col items-center justify-center text-slate-400 text-xs">
      <Bot class="h-12 w-12 mb-3 stroke-1 text-slate-300 dark:text-slate-600" />
      <p class="font-medium text-slate-600 dark:text-slate-300 text-sm">{t("select_session_prompt")}</p>
      <p class="text-slate-400 dark:text-slate-500 mt-1">{t("select_session_prompt_desc")}</p>
    </div>
  {:else}
    <!-- Unified Top Header: Cohesive, Professional, No Multi-Stripe Border Stack -->
    <div class="px-5 pt-3 pb-2.5 bg-white dark:bg-slate-900 border-b border-slate-200/90 dark:border-slate-800 shrink-0 space-y-2 shadow-2xs">
      <!-- Top Row: Badges & Parent Breadcrumb (Left) + Quick Actions (Right) -->
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <!-- Badges & Subagent Link -->
        <div class="flex items-center gap-1.5 flex-wrap min-w-0">
          <span class="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border shrink-0 {getPlatformBadge(session.platform)}">
            {session.platform}
          </span>

          {#if session.flavor && session.flavor !== session.platform && !session.flavor.startsWith("agy-") && session.flavor !== "ag"}
            <span class="text-[10px] font-mono text-slate-600 dark:text-slate-300 bg-slate-50 dark:bg-slate-800 px-1.5 py-0.5 rounded border border-slate-200 dark:border-slate-700 shrink-0 font-medium" title={session.flavor}>
              {session.flavor}
            </span>
          {/if}

          {#if session.is_running}
            <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] bg-emerald-50 dark:bg-emerald-950/60 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800/70 font-medium shrink-0">
              <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
              {t("running")}
            </span>
          {/if}

          {#if session.is_subagent && parentSession}
            <button
              onclick={() => onJumpSession(parentSession!)}
              class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium bg-claude-50 dark:bg-claude-950/70 hover:bg-claude-100 dark:hover:bg-claude-900/70 text-claude-700 dark:text-claude-300 border border-claude-200 dark:border-claude-800 transition cursor-pointer shrink-0"
              title={t("jump_to_parent")}
            >
              <CornerDownRight class="h-2.5 w-2.5 text-claude-600 dark:text-claude-400" />
              <span class="truncate max-w-[130px]">{t("parent_session")}: {parentSession.title || parentSession.id}</span>
              <ExternalLink class="h-2.5 w-2.5" />
            </button>
          {/if}
        </div>

        <!-- Action Buttons: Unified h-7 alignment & Claude Code styling -->
        <div class="flex items-center gap-1.5 shrink-0">
          <!-- Primary CTA: Open App or Resume in Terminal -->
          <button
            type="button"
            onclick={handleResume}
            class="h-7 px-2.5 rounded-lg transition cursor-pointer inline-flex items-center justify-center gap-1.5 text-[11px] font-semibold leading-none shadow-2xs whitespace-nowrap {isResuming || copiedResume
              ? 'bg-emerald-50 dark:bg-emerald-950/60 hover:bg-emerald-100 border border-emerald-300 dark:border-emerald-700 text-emerald-700 dark:text-emerald-300'
              : 'bg-claude-600 hover:bg-claude-700 active:bg-claude-800 text-white border border-claude-600 dark:border-claude-500'}"
            title={resumeAction?.kind === "app"
              ? `${resumeAction.appLabel}${resumeAction.targetPath ? `: ${resumeAction.targetPath}` : ""}`
              : session
                ? `Terminal: ${getResumeCommand(session)}`
                : t("resume_in_terminal")}
          >
            {#if isResuming || copiedResume}
              <Check class="h-3.5 w-3.5 shrink-0" />
              <span>{resumeAction?.kind === "app" ? t("opened_in_app", { app: resumeAction.appLabel }) : t("resumed_in_terminal")}</span>
            {:else if resumeAction?.kind === "app"}
              <AppWindow class="h-3.5 w-3.5 shrink-0" />
              <span>{t("open_in_app", { app: resumeAction.appLabel })}</span>
            {:else}
              <Play class="h-3.5 w-3.5 fill-current shrink-0" />
              <span>{t("resume")}</span>
            {/if}
          </button>

          <!-- Compact Segmented Utility Pill: Star / Finder / Terminal / More Menu -->
          <div
            bind:this={moreMenuEl}
            class="relative inline-flex items-center h-7 rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800/90 shadow-2xs divide-x divide-slate-200/80 dark:divide-slate-700/80"
          >
            <button
              type="button"
              onclick={() => onToggleStar(session.id)}
              class="h-full w-7 inline-flex items-center justify-center transition cursor-pointer first:rounded-l-[7px] {isStarred
                ? 'bg-amber-50 dark:bg-amber-950/50 text-amber-600 dark:text-amber-400'
                : 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/60 hover:text-slate-800 dark:hover:text-slate-100'}"
              title={isStarred ? t("unstar_session") : t("star_session")}
            >
              <Star class="h-3.5 w-3.5 {isStarred ? 'fill-amber-500 text-amber-500' : ''}" />
            </button>

            {#if session.main_path}
              <button
                type="button"
                onclick={revealFinder}
                class="h-full w-7 inline-flex items-center justify-center text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/60 hover:text-slate-800 dark:hover:text-slate-100 transition cursor-pointer"
                title={t("open_in_finder")}
              >
                <Folder class="h-3.5 w-3.5" />
              </button>
            {/if}

            {#if session.cwd}
              <button
                type="button"
                onclick={openTerminal}
                class="h-full w-7 inline-flex items-center justify-center text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/60 hover:text-slate-800 dark:hover:text-slate-100 transition cursor-pointer"
                title={t("open_in_terminal")}
              >
                <Terminal class="h-3.5 w-3.5" />
              </button>
            {/if}

            <button
              type="button"
              onclick={() => (showMoreMenu = !showMoreMenu)}
              class="h-full w-7 inline-flex items-center justify-center last:rounded-r-[7px] transition cursor-pointer {showMoreMenu
                ? 'bg-slate-100 dark:bg-slate-700 text-slate-900 dark:text-slate-100'
                : 'text-slate-500 dark:text-slate-400 hover:bg-slate-50 dark:hover:bg-slate-700/60 hover:text-slate-800 dark:hover:text-slate-100'}"
              title={t("more_actions")}
            >
              <MoreHorizontal class="h-3.5 w-3.5" />
            </button>

            {#if showMoreMenu}
              <div class="absolute right-0 top-full mt-1.5 w-44 rounded-xl border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-900 shadow-lg py-1 z-30 text-xs">
                <button
                  type="button"
                  onclick={() => { showMoreMenu = false; exportMarkdown(); }}
                  class="w-full px-3 py-1.5 flex items-center gap-2 text-left text-slate-700 dark:text-slate-200 hover:bg-slate-100/80 dark:hover:bg-slate-800 transition cursor-pointer"
                >
                  <FileText class="h-3.5 w-3.5 text-slate-400" />
                  <span>{t("export_markdown")}</span>
                </button>
                <button
                  type="button"
                  onclick={() => { showMoreMenu = false; exportJson(); }}
                  class="w-full px-3 py-1.5 flex items-center gap-2 text-left text-slate-700 dark:text-slate-200 hover:bg-slate-100/80 dark:hover:bg-slate-800 transition cursor-pointer"
                >
                  <Download class="h-3.5 w-3.5 text-slate-400" />
                  <span>{t("export_json")}</span>
                </button>
                <div class="my-1 border-t border-slate-100 dark:border-slate-800"></div>
                {#if resumeAction?.kind === "terminal"}
                  <button
                    type="button"
                    onclick={() => { showMoreMenu = false; copyResumeCmd(); }}
                    class="w-full px-3 py-1.5 flex items-center gap-2 text-left text-slate-700 dark:text-slate-200 hover:bg-slate-100/80 dark:hover:bg-slate-800 transition cursor-pointer"
                  >
                    <Terminal class="h-3.5 w-3.5 text-slate-400" />
                    <span>{t("copy_resume_cmd")}</span>
                  </button>
                {/if}
                <button
                  type="button"
                  onclick={() => { showMoreMenu = false; copySessionId(); }}
                  class="w-full px-3 py-1.5 flex items-center gap-2 text-left text-slate-700 dark:text-slate-200 hover:bg-slate-100/80 dark:hover:bg-slate-800 transition cursor-pointer"
                >
                  <Copy class="h-3.5 w-3.5 text-slate-400" />
                  <span>{t("copy_session_id")}</span>
                </button>
              </div>
            {/if}
          </div>

          <!-- Delete / Inline Confirmation -->
          {#if isConfirmingDelete}
            <div class="h-7 inline-flex items-center gap-1 bg-rose-50 dark:bg-rose-950/60 border border-rose-300 dark:border-rose-800 rounded-lg px-2 text-[11px]">
              <span class="text-rose-800 dark:text-rose-300 font-medium whitespace-nowrap">{t("confirm_delete_q")}</span>
              <button
                type="button"
                onclick={() => {
                  isConfirmingDelete = false;
                  onDeleteSingle(session);
                }}
                class="px-1.5 py-0.5 rounded bg-rose-600 hover:bg-rose-700 text-white font-bold text-[10px] leading-none transition cursor-pointer"
              >
                {t("confirm")}
              </button>
              <button
                type="button"
                onclick={() => (isConfirmingDelete = false)}
                class="px-1 py-0.5 rounded text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 text-[10px] leading-none transition cursor-pointer"
              >
                {t("cancel")}
              </button>
            </div>
          {:else}
            <button
              type="button"
              onclick={() => (isConfirmingDelete = true)}
              class="h-7 w-7 inline-flex items-center justify-center rounded-lg border border-slate-200 dark:border-slate-700 bg-white dark:bg-slate-800/90 text-slate-400 dark:text-slate-500 hover:bg-rose-50 dark:hover:bg-rose-950/50 hover:border-rose-200 dark:hover:border-rose-800 hover:text-rose-600 dark:hover:text-rose-400 transition cursor-pointer shadow-2xs"
              title={t("delete")}
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          {/if}
        </div>
      </div>

      <!-- Middle: Session Title (Clean, no divider lines) -->
      <div>
        <h2
          class="text-base sm:text-lg font-bold text-slate-900 dark:text-slate-100 tracking-tight leading-snug select-text line-clamp-2"
          title={session.title}
        >
          {session.title || t("untitled_session")}
        </h2>
      </div>

      <!-- Bottom: Unified Info Card (A single soft rounded card container instead of stripes) -->
      <div class="bg-slate-50/80 dark:bg-slate-800/70 rounded-xl p-2.5 border border-slate-200/70 dark:border-slate-700 flex items-center gap-2 text-xs text-slate-600 dark:text-slate-300">
        <!-- Metadata: Session ID & CWD & Updated -->
        <div class="flex items-center gap-2.5 text-[11px] font-mono min-w-0 flex-1 overflow-hidden">
          <!-- Session ID with Copy -->
          <button
            onclick={copySessionId}
            class="flex items-center gap-1 text-slate-600 dark:text-slate-300 hover:text-claude-600 dark:hover:text-claude-400 transition cursor-pointer shrink-0"
            title="ID: {session.id}"
          >
            <span class="text-slate-400 font-sans">ID:</span>
            <span>{session.id.slice(0, 8)}...</span>
            {#if copiedId}
              <Check class="h-3 w-3 text-emerald-600 shrink-0" />
            {:else}
              <Copy class="h-3 w-3 text-slate-400 shrink-0" />
            {/if}
          </button>

          <span class="text-slate-300 dark:text-slate-600">·</span>

          <!-- CWD / Workspace -->
          <div class="flex items-center gap-1 truncate max-w-[220px] text-slate-500 dark:text-slate-400 font-sans" title={session.cwd || session.dirname}>
            <Folder class="h-3 w-3 text-slate-400 shrink-0" />
            <span class="truncate">{session.cwd || session.dirname || t("unknown_path")}</span>
          </div>

          <span class="text-slate-300 dark:text-slate-600 shrink-0">·</span>

          <!-- Updated time -->
          <div class="flex items-center gap-1 text-slate-400 font-sans text-[10px] shrink-0">
            <Clock class="h-3 w-3 shrink-0" />
            <span>{formatTime(session.updated_at)}</span>
          </div>
        </div>
      </div>

      <!-- Child Subagents List (If any) -->
      {#if childSubagents.length > 0}
        <div class="flex items-center gap-1.5 overflow-x-auto text-[11px] pt-0.5">
          <span class="font-bold text-[10px] text-claude-700 dark:text-claude-300 uppercase tracking-wider shrink-0 flex items-center gap-1">
            <Sparkles class="h-3 w-3" />
            <span>{t("mounted_subtasks", { n: childSubagents.length })}</span>
          </span>
          {#each childSubagents as sub}
            <button
              onclick={() => onJumpSession(sub)}
              class="inline-flex items-center gap-1 px-2 py-0.5 rounded-lg bg-claude-50/70 dark:bg-claude-950/70 hover:bg-claude-100 dark:hover:bg-claude-900/70 border border-claude-200 dark:border-claude-800 text-[11px] text-claude-800 dark:text-claude-300 transition cursor-pointer shrink-0 font-medium"
            >
              <CornerDownRight class="h-2.5 w-2.5 text-claude-600 dark:text-claude-400" />
              <span class="truncate max-w-[130px]">{sub.title || sub.id}</span>
              <span class="text-[9px] text-slate-400 font-mono">({formatBytes(sub.size_bytes)})</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Segmented Tab Bar: Breaks Monotony & Controls View -->
    <div class="px-5 py-2 bg-slate-100/70 dark:bg-slate-900/70 border-b border-slate-200/80 dark:border-slate-800 flex items-center justify-between gap-3 text-xs shrink-0 select-none">
      <!-- Left: Segmented Switcher Pills -->
      <div class="flex items-center bg-slate-200/70 dark:bg-slate-800 p-0.5 rounded-lg border border-slate-300/50 dark:border-slate-700">
        <button
          onclick={() => (activeTab = "chat")}
          class="flex items-center gap-1.5 px-3 py-1 rounded-md text-xs transition cursor-pointer font-medium {activeTab === 'chat'
            ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs font-semibold'
            : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'}"
        >
          <MessageSquare class="h-3.5 w-3.5 text-claude-600 dark:text-claude-400" />
          <span>{t("tab_chat")}</span>
          <span class="px-1.5 py-0.2 rounded-full text-[10px] font-mono {activeTab === 'chat' ? 'bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300' : 'text-slate-400'}">
            {messages.length}
          </span>
        </button>

        <button
          onclick={() => (activeTab = "overview")}
          class="flex items-center gap-1.5 px-3 py-1 rounded-md text-xs transition cursor-pointer font-medium {activeTab === 'overview'
            ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs font-semibold'
            : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'}"
        >
          <LayoutDashboard class="h-3.5 w-3.5 text-claude-600 dark:text-claude-400" />
          <span>{t("tab_overview")}</span>
        </button>

        <button
          onclick={() => (activeTab = "logs")}
          class="flex items-center gap-1.5 px-3 py-1 rounded-md text-xs transition cursor-pointer font-medium {activeTab === 'logs'
            ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs font-semibold'
            : 'text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-slate-200'}"
        >
          <Activity class="h-3.5 w-3.5 text-claude-600 dark:text-claude-400" />
          <span>{t("tab_logs")}</span>
          {#if (sessionLogs?.total ?? sessionUsage?.eventCount ?? 0) > 0}
            <span class="px-1.5 py-0.2 rounded-full text-[10px] font-mono {activeTab === 'logs' ? 'bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300' : 'text-slate-400'}">
              {sessionLogs?.total ?? sessionUsage?.eventCount}
            </span>
          {/if}
        </button>
      </div>

      <!-- Right: Platform Info & Flavor -->
      <div class="text-[11px] text-slate-500 dark:text-slate-400 font-mono flex items-center gap-1.5">
        <span>{session.platform}</span>
        {#if session.flavor}
          <span class="text-slate-300 dark:text-slate-600">/</span>
          <span class="text-slate-400 dark:text-slate-500">{session.flavor}</span>
        {/if}
      </div>
    </div>

    <!-- Row 2: In-Chat Search Bar (Dedicated Sub-header) -->
    {#if activeTab === "chat"}
      <div class="px-5 py-1.5 bg-white/70 dark:bg-slate-900/60 border-b border-slate-200/70 dark:border-slate-800 flex items-center justify-between gap-3 shrink-0">
        <div class="relative flex-1 max-w-sm">
          <Search class="absolute left-2.5 top-2 h-3.5 w-3.5 text-slate-400 pointer-events-none" />
          <input
            type="text"
            bind:value={inChatSearch}
            placeholder={t("search_chat")}
            class="w-full bg-slate-50/80 dark:bg-slate-800 hover:bg-white dark:hover:bg-slate-800 focus:bg-white dark:focus:bg-slate-900 border border-slate-200/90 dark:border-slate-700 focus:border-claude-500 rounded-lg pl-7.5 pr-7 py-1 text-xs text-slate-800 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-claude-500 transition shadow-2xs"
          />
          {#if inChatSearch}
            <button
              onclick={() => (inChatSearch = "")}
              class="absolute right-2 top-1.5 text-slate-400 hover:text-slate-600 p-0.5 cursor-pointer"
              title={t("clear_search")}
            >
              <X class="h-3 w-3" />
            </button>
          {/if}
        </div>

        {#if inChatSearch}
          <span class="text-[11px] text-claude-700 dark:text-claude-300 bg-claude-50 dark:bg-claude-950/70 border border-claude-200/80 dark:border-claude-800 px-2 py-0.5 rounded-md font-mono shrink-0">
            {t("matched_messages", { n: visibleMessages.length })}
          </span>
        {/if}
      </div>
    {/if}

    {#if activeTab === "chat"}
      <!-- Chat Timeline Stream (Occupies 85%+ screen height) -->
      <div class="flex-1 overflow-y-auto p-4 space-y-4">
        {#if isLoadingMessages}
          <div class="h-48 flex items-center justify-center text-slate-400 text-xs gap-2">
            <div class="h-4 w-4 rounded-full border-2 border-claude-500 border-t-transparent animate-spin"></div>
            <span>{t("loading_messages")}</span>
          </div>
        {:else if visibleMessages.length === 0}
          <div class="h-48 flex flex-col items-center justify-center text-slate-400 text-xs">
            <FileText class="h-8 w-8 mb-2 stroke-1 text-slate-300" />
            <p class="font-medium text-slate-500">
              {inChatSearch ? t("no_messages") : t("no_messages")}
            </p>
            {#if !session.has_transcript}
              <p class="text-[11px] text-amber-600 mt-1">{t("empty_transcript_hint")}</p>
            {/if}
          </div>
        {:else}
          {#each visibleMessages as msg, idx}
            {#if msg.role === "user"}
              <!-- User Message Card -->
              <div class="flex gap-3 max-w-4xl mx-auto">
                <div class="h-7 w-7 rounded-lg bg-slate-200/80 dark:bg-slate-800 text-slate-700 dark:text-slate-200 flex items-center justify-center shrink-0 mt-0.5 shadow-2xs">
                  <User class="h-4 w-4" />
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("user_question")}</span>
                    {#if msg.time}
                      <span class="text-[10px] text-slate-400 font-mono">{msg.time}</span>
                    {/if}
                  </div>
                  <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-700/80 rounded-2xl p-3.5 text-xs text-slate-800 dark:text-slate-100 leading-relaxed shadow-2xs whitespace-pre-wrap break-words select-text selection:bg-claude-100 dark:selection:bg-claude-900">
                    {msg.text}
                  </div>
                </div>
              </div>
            {:else if msg.role === "assistant"}
              <!-- Assistant Message Card -->
              <div class="flex gap-3 max-w-4xl mx-auto">
                <div class="h-7 w-7 rounded-lg bg-claude-100 dark:bg-claude-950 text-claude-700 dark:text-claude-300 flex items-center justify-center shrink-0 mt-0.5 shadow-2xs">
                  <Bot class="h-4 w-4" />
                </div>
                <div class="flex-1 min-w-0 space-y-2">
                  <div class="flex items-center gap-2">
                    <span class="text-xs font-bold text-claude-900 dark:text-claude-300">{t("ai_assistant")}</span>
                    {#if msg.time}
                      <span class="text-[10px] text-slate-400 font-mono">{msg.time}</span>
                    {/if}
                  </div>

                  <!-- Thinking / CoT Block (Collapsible) -->
                  {#if msg.thinking}
                    <div class="rounded-xl border border-amber-200/80 dark:border-amber-800/70 bg-amber-50/40 dark:bg-amber-950/30 overflow-hidden text-xs">
                      <button
                        onclick={() => toggleThinking(idx)}
                        class="w-full px-3 py-1.5 flex items-center justify-between text-amber-900 dark:text-amber-300 hover:bg-amber-100/50 dark:hover:bg-amber-900/30 transition cursor-pointer text-left"
                      >
                        <div class="flex items-center gap-1.5 font-medium">
                          <Sparkles class="h-3.5 w-3.5 text-amber-600 dark:text-amber-400" />
                          <span>{t("thinking_process")}</span>
                        </div>
                        <div class="flex items-center text-slate-400">
                          {#if expandedThinking[idx]}
                            <ChevronDown class="h-3.5 w-3.5" />
                          {:else}
                            <ChevronRight class="h-3.5 w-3.5" />
                          {/if}
                        </div>
                      </button>
                      {#if expandedThinking[idx]}
                        <div class="px-3 py-2 border-t border-amber-200/60 dark:border-amber-800/60 bg-white/70 dark:bg-slate-900/80 text-[11px] text-slate-700 dark:text-slate-300 leading-relaxed whitespace-pre-wrap break-words font-mono select-text">
                          {msg.thinking}
                        </div>
                      {/if}
                    </div>
                  {/if}

                  <!-- Tool Calls Block (Collapsible) -->
                  {#if msg.tool_calls && msg.tool_calls.length > 0}
                    <div class="rounded-xl border border-slate-200 dark:border-slate-700 bg-slate-50/80 dark:bg-slate-900/80 overflow-hidden text-xs">
                      <button
                        onclick={() => toggleTool(idx)}
                        class="w-full px-3 py-1.5 flex items-center justify-between text-slate-800 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800 transition cursor-pointer text-left"
                      >
                        <div class="flex items-center gap-1.5 font-medium">
                          <Wrench class="h-3.5 w-3.5 text-claude-600 dark:text-claude-400" />
                          <span>{t("tool_calls", { n: msg.tool_calls.length })}</span>
                        </div>
                        <div class="flex items-center text-slate-400">
                          {#if expandedTools[idx]}
                            <ChevronDown class="h-3.5 w-3.5" />
                          {:else}
                            <ChevronRight class="h-3.5 w-3.5" />
                          {/if}
                        </div>
                      </button>
                      {#if expandedTools[idx]}
                        <div class="px-3 py-2 border-t border-slate-200 dark:border-slate-700 space-y-2 bg-slate-900 text-slate-200 font-mono text-[11px] rounded-b-xl overflow-x-auto select-text">
                          {#each msg.tool_calls as tc}
                            <div class="space-y-1">
                              <div class="text-claude-400 font-bold flex items-center gap-1">
                                <span>$ {tc.name}</span>
                              </div>
                              {#if tc.args}
                                <pre class="text-slate-300 text-[10px] bg-slate-800/80 p-1.5 rounded overflow-x-auto whitespace-pre-wrap">{tc.args}</pre>
                              {/if}
                              {#if tc.output}
                                <pre class="text-slate-400 text-[10px] bg-slate-950/80 p-1.5 rounded overflow-x-auto whitespace-pre-wrap max-h-40">{tc.output}</pre>
                              {/if}
                            </div>
                          {/each}
                        </div>
                      {/if}
                    </div>
                  {/if}

                  <!-- Text Body (Markdown) -->
                  {#if msg.text}
                    <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-700/80 rounded-2xl p-3.5 text-slate-800 dark:text-slate-100 shadow-2xs select-text selection:bg-claude-100 dark:selection:bg-claude-900 asm-prose">
                      {@html renderMarkdown(msg.text)}
                    </div>
                  {/if}
                </div>
              </div>
            {:else if msg.role === "subagent"}
              <!-- Subagent Activity Banner -->
              <div class="max-w-4xl mx-auto flex items-center gap-2 bg-purple-50/70 border border-purple-200 px-3.5 py-2 rounded-xl text-xs text-purple-900">
                <Sparkles class="h-3.5 w-3.5 text-purple-600 shrink-0" />
                <span class="font-bold shrink-0">{t("subagent_dispatch")}</span>
                <span class="font-mono truncate">{msg.text}</span>
              </div>
            {/if}
          {/each}
        {/if}
      </div>
    {:else if activeTab === "logs"}
      <!-- Session Request & Usage Logs (Tab 3) -->
      <div class="flex-1 min-h-0 flex flex-col overflow-hidden p-4 bg-slate-50/60 dark:bg-slate-950/60">
        <div class="flex-1 min-h-0 max-w-4xl w-full mx-auto flex flex-col gap-3">
          {#if sessionUsage}
            <div class="shrink-0 grid grid-cols-2 sm:grid-cols-4 gap-2.5">
              <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-2.5">
                <div class="text-[10px] text-slate-400">{t("usage_total_cost")}</div>
                <div class="text-sm font-bold font-mono text-slate-900 dark:text-slate-100 mt-0.5">
                  {sessionUsage.priced ? formatMoney(sessionUsage.cost, sessionUsage.currency) : "-"}
                </div>
              </div>
              <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-2.5">
                <div class="text-[10px] text-slate-400">{t("usage_total_tokens")}</div>
                <div class="text-sm font-bold font-mono text-slate-900 dark:text-slate-100 mt-0.5">
                  {formatCompactTokens(sessionUsage.totalTokens)}
                </div>
              </div>
              <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-2.5">
                <div class="text-[10px] text-slate-400">Input / Output</div>
                <div class="text-sm font-bold font-mono text-slate-900 dark:text-slate-100 mt-0.5">
                  {formatCompactTokens(sessionUsage.inputTokens)} / {formatCompactTokens(sessionUsage.outputTokens)}
                </div>
              </div>
              <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-xl p-2.5">
                <div class="text-[10px] text-slate-400">{t("usage_cache_hit")}</div>
                <div class="text-sm font-bold font-mono text-emerald-600 dark:text-emerald-400 mt-0.5">
                  {sessionUsage.cacheHitRate.toFixed(1)}%
                </div>
              </div>
            </div>
          {/if}

          <div class="flex-1 min-h-0 flex flex-col bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl shadow-2xs overflow-hidden">
            <UsageLogTable
              logs={sessionLogs}
              loading={sessionLogsLoading}
              showSessionCols={false}
              fillHeight={true}
              onPageChange={(p) => loadSessionLogs(p)}
            />
          </div>
        </div>
      </div>
    {:else}
      <!-- Session Overview & Configuration Dashboard (Tab 2) -->
      <div class="flex-1 overflow-y-auto p-5 bg-slate-50/60 dark:bg-slate-950/60 space-y-4">
        <div class="max-w-4xl mx-auto space-y-4">

          <!-- Card 1: Desktop App Launch OR Terminal Resume -->
          {#if resumeAction?.kind === "app"}
            <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl p-4 shadow-2xs flex flex-wrap items-center justify-between gap-3">
              <div class="flex items-center gap-3 min-w-0">
                <div class="h-9 w-9 rounded-xl bg-claude-50 dark:bg-claude-950/70 text-claude-600 dark:text-claude-400 flex items-center justify-center shrink-0 border border-claude-200/70 dark:border-claude-800/70">
                  <AppWindow class="h-4.5 w-4.5" />
                </div>
                <div class="min-w-0">
                  <div class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("app_resume_title")} · {resumeAction.appLabel}</div>
                  <div class="text-[11px] text-slate-500 dark:text-slate-400 truncate mt-0.5">
                    {resumeAction.targetPath || t("app_resume_hint", { app: resumeAction.appLabel })}
                  </div>
                </div>
              </div>
              <div class="flex items-center gap-2 shrink-0">
                <button
                  type="button"
                  onclick={handleResume}
                  class="h-7 px-3 inline-flex items-center gap-1.5 text-xs font-semibold rounded-lg bg-claude-600 hover:bg-claude-700 text-white transition cursor-pointer shadow-2xs"
                >
                  {#if isResuming}
                    <Check class="h-3.5 w-3.5" />
                    <span>{t("opened_in_app", { app: resumeAction.appLabel })}</span>
                  {:else}
                    <AppWindow class="h-3.5 w-3.5" />
                    <span>{t("open_in_app", { app: resumeAction.appLabel })}</span>
                  {/if}
                </button>
                <button
                  type="button"
                  onclick={copySessionId}
                  class="h-7 px-2.5 inline-flex items-center gap-1.5 text-xs font-medium rounded-lg bg-slate-100 dark:bg-slate-800 hover:bg-slate-200 dark:hover:bg-slate-700 text-slate-700 dark:text-slate-200 transition cursor-pointer border border-slate-200 dark:border-slate-700"
                >
                  {#if copiedId}
                    <Check class="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-400" />
                    <span>{t("copied")}</span>
                  {:else}
                    <Copy class="h-3.5 w-3.5 text-slate-400" />
                    <span>{t("copy_session_id")}</span>
                  {/if}
                </button>
              </div>
            </div>
          {:else}
            <div class="bg-slate-900 rounded-2xl overflow-hidden shadow-xs border border-slate-800">
              <div class="px-4 py-2.5 bg-slate-950/80 border-b border-slate-800/80 flex items-center justify-between gap-3">
                <div class="flex items-center gap-2">
                  <span class="h-2.5 w-2.5 rounded-full bg-rose-500/80"></span>
                  <span class="h-2.5 w-2.5 rounded-full bg-amber-500/80"></span>
                  <span class="h-2.5 w-2.5 rounded-full bg-emerald-500/80"></span>
                  <span class="text-[11px] font-mono text-slate-400 pl-1.5">Terminal Resume</span>
                </div>

                <div class="flex items-center gap-2">
                  <button
                    onclick={handleResume}
                    class="flex items-center gap-1.5 px-3 py-1 text-xs font-semibold rounded-lg bg-claude-600 hover:bg-claude-500 active:bg-claude-700 text-white transition cursor-pointer shadow-2xs"
                    title={session ? `Terminal: ${getResumeCommand(session)}` : t("resume_in_terminal")}
                  >
                    {#if isResuming || copiedResume}
                      <Check class="h-3.5 w-3.5" />
                      <span>{t("resumed_in_terminal")}</span>
                    {:else}
                      <Play class="h-3.5 w-3.5 fill-current" />
                      <span>{t("resume_in_terminal")}</span>
                    {/if}
                  </button>

                  <button
                    onclick={copyResumeCmd}
                    class="flex items-center gap-1.5 px-2.5 py-1 text-xs font-medium rounded-lg bg-slate-800 hover:bg-slate-700 text-slate-200 transition cursor-pointer border border-slate-700"
                    title="CLI Command"
                  >
                    {#if copiedResume && !isResuming}
                      <Check class="h-3.5 w-3.5 text-emerald-400" />
                      <span class="text-emerald-400">{t("copied")}</span>
                    {:else}
                      <Copy class="h-3.5 w-3.5 text-slate-400" />
                      <span>{t("copy_code")}</span>
                    {/if}
                  </button>
                </div>
              </div>

              <div class="p-3.5 text-emerald-400 font-mono text-xs select-all overflow-x-auto leading-relaxed">
                <code>$ {getResumeCommand(session)}</code>
              </div>
            </div>
          {/if}

          <!-- Card 2: 存储位置与工作目录 (Workspace & Storage) -->
          <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl p-4 shadow-2xs space-y-3">
            <div class="flex items-center gap-2">
              <div class="h-6 w-6 rounded-lg bg-claude-50 dark:bg-claude-950 text-claude-600 dark:text-claude-400 flex items-center justify-center">
                <Folder class="h-3.5 w-3.5" />
              </div>
              <h3 class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("storage_and_workspace")}</h3>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-3 pt-1">
              <!-- Storage Main Path -->
              <div class="bg-slate-50 dark:bg-slate-800/70 border border-slate-100 dark:border-slate-700 rounded-xl p-3 space-y-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-[11px] font-medium text-slate-500 dark:text-slate-400 flex items-center gap-1">
                    <Database class="h-3 w-3 text-slate-400" />
                    {t("data_source_file")}
                  </span>
                  <div class="flex items-center gap-1">
                    <button
                      onclick={copyMainPath}
                      class="text-[10px] text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200 flex items-center gap-0.5 p-1 rounded hover:bg-slate-200/60 dark:hover:bg-slate-700 transition cursor-pointer"
                      title={t("copy")}
                    >
                      {#if copiedPath}
                        <Check class="h-3 w-3 text-emerald-600" />
                        <span class="text-emerald-700 dark:text-emerald-400">{t("copied")}</span>
                      {:else}
                        <Copy class="h-3 w-3" />
                        <span>{t("copy")}</span>
                      {/if}
                    </button>
                    {#if session.main_path}
                      <button
                        onclick={revealFinder}
                        class="text-[10px] text-claude-600 dark:text-claude-400 hover:text-claude-800 flex items-center gap-0.5 p-1 rounded hover:bg-claude-50 dark:hover:bg-claude-950/60 transition cursor-pointer"
                        title={t("open_in_finder")}
                      >
                        <ExternalLink class="h-3 w-3" />
                        <span>{t("reveal")}</span>
                      </button>
                    {/if}
                  </div>
                </div>
                <div class="text-[11px] font-mono text-slate-700 dark:text-slate-200 break-all select-all">
                  {session.main_path || t("unknown_or_consolidated")}
                </div>
              </div>

              <!-- Workspace CWD -->
              <div class="bg-slate-50 dark:bg-slate-800/70 border border-slate-100 dark:border-slate-700 rounded-xl p-3 space-y-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-[11px] font-medium text-slate-500 dark:text-slate-400 flex items-center gap-1">
                    <Folder class="h-3 w-3 text-slate-400" />
                    {t("workspace_cwd")}
                  </span>
                  {#if session.cwd}
                    <button
                      onclick={openTerminal}
                      class="text-[10px] text-emerald-600 dark:text-emerald-400 hover:text-emerald-800 flex items-center gap-0.5 p-1 rounded hover:bg-emerald-50 dark:hover:bg-emerald-950/60 transition cursor-pointer"
                      title={t("open_in_terminal")}
                    >
                      <Terminal class="h-3 w-3" />
                      <span>{t("open")}</span>
                    </button>
                  {/if}
                </div>
                <div class="text-[11px] font-mono text-slate-700 dark:text-slate-200 break-all select-all">
                  {session.cwd || session.dirname || t("unspecified_workspace")}
                </div>
              </div>
            </div>

            <!-- Meta statistics grid -->
            <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-2 border-t border-slate-100 dark:border-slate-800">
              <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                <div class="text-[10px] text-slate-400">{t("disk_usage")}</div>
                <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5 font-mono">{formatBytes(session.size_bytes)}</div>
              </div>
              <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                <div class="text-[10px] text-slate-400">{t("total_messages")}</div>
                <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5 font-mono">{t("messages_count", { n: messages.length })}</div>
              </div>
              <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                <div class="text-[10px] text-slate-400">{t("total_turns")}</div>
                <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5 font-mono">{session.turn_count || 0} {t("turns")}</div>
              </div>
              <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                <div class="text-[10px] text-slate-400">{t("session_structure")}</div>
                <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5">
                  {session.is_subagent ? t("subagent_task") : childSubagents.length > 0 ? t("main_with_children") : t("standalone_session")}
                </div>
              </div>
            </div>
          </div>

          {#if sessionUsage}
            <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl p-4 shadow-2xs space-y-3">
              <div class="flex items-center justify-between gap-2">
                <h3 class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("usage_session_card_title")}</h3>
                <button
                  type="button"
                  onclick={() => appStore.setNav("usage")}
                  class="inline-flex items-center gap-1 text-[11px] font-medium text-claude-600 dark:text-claude-400 hover:underline cursor-pointer"
                >
                  <span>{t("usage_view_in_dashboard")}</span>
                  <ExternalLink class="h-3 w-3" />
                </button>
              </div>
              <div class="grid grid-cols-2 sm:grid-cols-4 gap-2">
                <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                  <div class="text-[10px] text-slate-400">{t("usage_total_cost")}</div>
                  <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5 font-mono">
                    {sessionUsage.priced ? formatMoney(sessionUsage.cost, sessionUsage.currency) : "-"}
                  </div>
                </div>
                <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                  <div class="text-[10px] text-slate-400">{t("usage_total_tokens")}</div>
                  <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5 font-mono">
                    {formatCompactTokens(sessionUsage.totalTokens)}
                  </div>
                </div>
                <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                  <div class="text-[10px] text-slate-400">Input / Output</div>
                  <div class="text-xs font-bold text-slate-800 dark:text-slate-100 mt-0.5 font-mono">
                    {formatCompactTokens(sessionUsage.inputTokens)} / {formatCompactTokens(sessionUsage.outputTokens)}
                  </div>
                </div>
                <div class="p-2 rounded-lg bg-slate-50/70 dark:bg-slate-800/60 text-center">
                  <div class="text-[10px] text-slate-400">{t("usage_cache_hit")}</div>
                  <div class="text-xs font-bold text-emerald-600 dark:text-emerald-400 mt-0.5 font-mono">
                    {sessionUsage.cacheHitRate.toFixed(1)}%
                  </div>
                </div>
              </div>
            </div>
          {/if}

          <!-- Card 3: 任务层级与子代理树 (Hierarchy) -->
          <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-800 rounded-2xl p-4 shadow-2xs space-y-3">
            <div class="flex items-center gap-2">
              <div class="h-6 w-6 rounded-lg bg-purple-50 dark:bg-purple-950 text-purple-600 dark:text-purple-400 flex items-center justify-center">
                <Sparkles class="h-3.5 w-3.5" />
              </div>
              <h3 class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("task_hierarchy")}</h3>
            </div>

            {#if session.is_subagent && parentSession}
              <!-- Parent Session Link -->
              <div class="bg-purple-50/60 border border-purple-200/80 rounded-xl p-3 flex items-center justify-between">
                <div class="flex items-center gap-2.5 min-w-0">
                  <CornerDownRight class="h-4 w-4 text-purple-600 shrink-0" />
                  <div class="min-w-0">
                    <div class="text-[10px] text-purple-700 font-medium">{t("parent_session")}</div>
                    <div class="text-xs font-bold text-slate-800 truncate">{parentSession.title || parentSession.id}</div>
                  </div>
                </div>
                <button
                  onclick={() => onJumpSession(parentSession)}
                  class="flex items-center gap-1 px-2.5 py-1 text-xs font-medium rounded-lg bg-white border border-purple-200 hover:bg-purple-100 text-purple-800 transition cursor-pointer shrink-0 shadow-2xs"
                >
                  <ArrowLeft class="h-3 w-3" />
                  <span>{t("jump_to_parent")}</span>
                </button>
              </div>
            {/if}

            {#if childSubagents && childSubagents.length > 0}
              <!-- Child Subagents List -->
              <div class="space-y-2 pt-1">
                <div class="text-[11px] font-medium text-slate-600">
                  {t("derived_subtasks", { n: childSubagents.length })}
                </div>
                <div class="grid grid-cols-1 sm:grid-cols-2 gap-2">
                  {#each childSubagents as sub}
                    <button
                      onclick={() => onJumpSession(sub)}
                      class="flex items-center justify-between p-2.5 rounded-xl border border-slate-200 bg-slate-50/50 hover:bg-purple-50/60 hover:border-purple-300 transition text-left cursor-pointer group"
                    >
                      <div class="min-w-0">
                        <div class="text-xs font-medium text-slate-800 truncate group-hover:text-purple-900">
                          {sub.title || sub.id}
                        </div>
                        <div class="text-[10px] text-slate-400 font-mono flex items-center gap-2 mt-0.5">
                          <span>{formatTime(sub.updated_at)}</span>
                          <span>{formatBytes(sub.size_bytes)}</span>
                        </div>
                      </div>
                      <ChevronRight class="h-3.5 w-3.5 text-slate-400 group-hover:text-purple-600 shrink-0 ml-2" />
                    </button>
                  {/each}
                </div>
              </div>
            {:else if !session.is_subagent}
              <div class="p-3 rounded-xl bg-slate-50 border border-slate-100 text-center text-xs text-slate-400">
                {t("no_subtasks")}
              </div>
            {/if}
          </div>

        </div>
      </div>
    {/if}
  {/if}
</div>
