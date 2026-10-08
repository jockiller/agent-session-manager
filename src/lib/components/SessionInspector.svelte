<script lang="ts">
  import {
    Folder,
    Terminal,
    Trash2,
    Copy,
    Check,
    Coins,
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
    Zap,
    Database,
    Cpu,
    X,
    Activity,
    MessageSquare,
    LayoutDashboard,
    BarChart2,
    FileCode,
  } from "@lucide/svelte";
  import type { ChatMessage, SessionSummary } from "../types";
  import { formatBytes, formatTime, formatTokens, formatNumber, getPlatformBadge } from "../utils";
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
  let activeTab = $state<"chat" | "overview">("chat");

  let tokenBreakdown = $derived.by(() => {
    if (!session || !session.token_stats) return null;
    const p = session.token_stats.prompt_tokens || 0;
    const c = session.token_stats.completion_tokens || 0;
    const cr = session.token_stats.cache_read_tokens || 0;
    const sum = p + c + cr;
    if (sum === 0) return null;

    const formatPct = (val: number) => {
      if (val <= 0) return "0%";
      const pct = (val / sum) * 100;
      if (pct < 0.1) return "<0.1%";
      if (pct < 1) return `${pct.toFixed(1)}%`;
      return `${Math.round(pct)}%`;
    };

    const getBarWidth = (val: number) => {
      if (val <= 0) return 0;
      const rawPct = (val / sum) * 100;
      return Math.max(rawPct, 1.2);
    };

    return {
      promptText: formatPct(p),
      completionText: formatPct(c),
      cacheText: formatPct(cr),
      promptBarWidth: getBarWidth(p),
      completionBarWidth: getBarWidth(c),
      cacheBarWidth: getBarWidth(cr),
    };
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
    const plat = s.platform.toLowerCase();
    let cliCmd = "";

    switch (plat) {
      case "antigravity":
        cliCmd = `agy --conversation "${s.id}"`;
        break;
      case "grok":
        cliCmd = `grok --resume "${s.id}"`;
        break;
      case "claude":
        cliCmd = `claude --resume "${s.id}"`;
        break;
      case "openclaude":
        cliCmd = `openclaude --resume "${s.id}"`;
        break;
      case "codex":
        cliCmd = `codex resume "${s.id}"`;
        break;
      case "zcode":
        cliCmd = `zcode --resume "${s.id}"`;
        break;
      case "dsh":
        cliCmd = `dsh-tui --resume "${s.id}"`;
        break;
      case "pi":
        cliCmd = `pi --session "${s.id}"`;
        break;
      case "omp":
        cliCmd = `omp --resume "${s.id}"`;
        break;
      case "qwen":
        cliCmd = `qwen --resume "${s.id}"`;
        break;
      case "trae":
        cliCmd = `trae`;
        break;
      case "qoder":
        cliCmd = `qoder resume "${s.id}"`;
        break;
      case "cursor":
        cliCmd = `cursor`;
        break;
      case "codebuddy":
        cliCmd = `codebuddy --resume "${s.id}"`;
        break;
      case "workbuddy":
        cliCmd = `workbuddy --session "${s.id}"`;
        break;
      case "opencode":
        cliCmd = `opencode session resume "${s.id}"`;
        break;
      case "codewiz":
        cliCmd = `codewiz session resume "${s.id}"`;
        break;
      case "kimi":
        cliCmd = `kimi-code --resume "${s.id}"`;
        break;
      case "gemini":
        cliCmd = `gemini --resume "${s.id}"`;
        break;
      case "deepseek":
        cliCmd = `deepseek --resume "${s.id}"`;
        break;
      case "openclaw":
        cliCmd = `openclaw session resume "${s.id}"`;
        break;
      case "hermes":
        cliCmd = `hermes session resume "${s.id}"`;
        break;
      default:
        cliCmd = `${plat} --resume "${s.id}"`;
        break;
    }

    const dir = s.cwd || s.dirname;
    if (dir && dir.trim() && dir !== "/") {
      return `cd "${dir}" && ${cliCmd}`;
    }
    return cliCmd;
  }

  function copyResumeCmd() {
    if (!session) return;
    const cmd = getResumeCommand(session);
    navigator.clipboard.writeText(cmd);
    copiedResume = true;
    setTimeout(() => (copiedResume = false), 2000);
  }

  async function resumeInTerminal() {
    if (!session) return;
    const cmd = getResumeCommand(session);
    try {
      // 1. 复制到剪贴板备用
      await navigator.clipboard.writeText(cmd);
      copiedResume = true;
      isResuming = true;

      // 2. 直接在系统终端中运行恢复命令
      await invoke("run_in_terminal", { command: cmd });
    } catch (e) {
      console.error("Failed to run resume command in terminal:", e);
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
      await invoke("open_terminal", { cwd: session.cwd });
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
    if (session.token_stats) {
      md += `- **Tokens**: ${t("md_tokens_summary", {
        total: formatTokens(session.token_stats.total_tokens),
        prompt: formatTokens(session.token_stats.prompt_tokens),
        comp: formatTokens(session.token_stats.completion_tokens)
      })}\n`;
    }
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

<div class="flex-1 flex flex-col h-full bg-slate-50/30 overflow-hidden">
  {#if !session}
    <div class="h-full flex flex-col items-center justify-center text-slate-400 text-xs">
      <Bot class="h-12 w-12 mb-3 stroke-1 text-slate-300" />
      <p class="font-medium text-slate-600 text-sm">{t("select_session_prompt")}</p>
      <p class="text-slate-400 mt-1">{t("select_session_prompt_desc")}</p>
    </div>
  {:else}
    <!-- Unified Top Header: Cohesive, Professional, No Multi-Stripe Border Stack -->
    <div class="px-5 pt-3 pb-2.5 bg-white border-b border-slate-200/90 shrink-0 space-y-2 shadow-2xs">
      <!-- Top Row: Badges & Parent Breadcrumb (Left) + Quick Actions (Right) -->
      <div class="flex items-center justify-between gap-3 flex-wrap">
        <!-- Badges & Subagent Link -->
        <div class="flex items-center gap-1.5 flex-wrap min-w-0">
          <span class="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider border shrink-0 {getPlatformBadge(session.platform)}">
            {session.platform}
          </span>

          {#if session.flavor && session.flavor !== session.platform && !session.flavor.startsWith("agy-") && session.flavor !== "ag"}
            <span class="text-[10px] font-mono text-slate-600 bg-slate-50 px-1.5 py-0.5 rounded border border-slate-200 shrink-0 font-medium" title={session.flavor}>
              {session.flavor}
            </span>
          {/if}

          {#if session.is_running}
            <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] bg-emerald-50 text-emerald-700 border border-emerald-200 font-medium shrink-0">
              <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
              {t("running")}
            </span>
          {/if}

          {#if session.is_subagent && parentSession}
            <button
              onclick={() => onJumpSession(parentSession!)}
              class="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-medium bg-indigo-50 hover:bg-indigo-100 text-indigo-700 border border-indigo-200 transition cursor-pointer shrink-0"
              title={t("jump_to_parent")}
            >
              <CornerDownRight class="h-2.5 w-2.5 text-indigo-600" />
              <span class="truncate max-w-[130px]">{t("parent_session")}: {parentSession.title || parentSession.id}</span>
              <ExternalLink class="h-2.5 w-2.5" />
            </button>
          {/if}
        </div>

        <!-- Action Buttons -->
        <div class="flex items-center gap-1 shrink-0">
          <!-- Star toggle -->
          <button
            onclick={() => onToggleStar(session.id)}
            class="p-1.5 rounded-lg border transition cursor-pointer text-xs flex items-center justify-center {isStarred
              ? 'bg-amber-50 border-amber-300 text-amber-700'
              : 'bg-white border-slate-200 text-slate-500 hover:bg-slate-50 hover:text-slate-700'}"
            title={isStarred ? t("unstar_session") : t("star_session")}
          >
            <Star class="h-3.5 w-3.5 {isStarred ? 'fill-amber-500 text-amber-500' : ''}" />
          </button>

          <!-- Resume in Terminal (Runs command in terminal and copies to clipboard) -->
          <button
            onclick={resumeInTerminal}
            class="px-2.5 py-1 rounded-lg transition cursor-pointer text-xs flex items-center gap-1 font-semibold shadow-2xs {isResuming || copiedResume
              ? 'bg-emerald-50 hover:bg-emerald-100 border border-emerald-300 text-emerald-700'
              : 'bg-emerald-600 hover:bg-emerald-700 active:bg-emerald-800 text-white border border-emerald-600'}"
            title={session ? `Terminal: ${getResumeCommand(session)}` : t("resume_in_terminal")}
          >
            {#if isResuming || copiedResume}
              <Check class="h-3.5 w-3.5 text-emerald-600" />
              <span class="text-[11px]">{t("resumed_in_terminal")}</span>
            {:else}
              <Play class="h-3.5 w-3.5 fill-current" />
              <span class="text-[11px]">{t("resume")}</span>
            {/if}
          </button>

          <div class="h-3.5 w-px bg-slate-200 mx-0.5"></div>

          <!-- Export Markdown -->
          <button
            onclick={exportMarkdown}
            class="p-1.5 rounded-lg bg-white hover:bg-slate-50 border border-slate-200 text-slate-600 hover:text-slate-900 transition cursor-pointer"
            title={t("export_markdown")}
          >
            <FileText class="h-3.5 w-3.5 text-slate-500" />
          </button>

          <!-- Export JSON -->
          <button
            onclick={exportJson}
            class="p-1.5 rounded-lg bg-white hover:bg-slate-50 border border-slate-200 text-slate-600 hover:text-slate-900 transition cursor-pointer"
            title={t("export_json")}
          >
            <Download class="h-3.5 w-3.5 text-slate-500" />
          </button>

          <!-- Open in Finder -->
          <button
            onclick={revealFinder}
            class="p-1.5 rounded-lg bg-white hover:bg-slate-50 border border-slate-200 text-slate-600 hover:text-slate-900 transition cursor-pointer"
            title={t("open_in_finder")}
          >
            <Folder class="h-3.5 w-3.5 text-slate-500" />
          </button>

          <!-- Open Terminal -->
          {#if session.cwd}
            <button
              onclick={openTerminal}
              class="p-1.5 rounded-lg bg-white hover:bg-slate-50 border border-slate-200 text-slate-600 hover:text-slate-900 transition cursor-pointer"
              title={t("open_in_terminal")}
            >
              <Terminal class="h-3.5 w-3.5 text-slate-500" />
            </button>
          {/if}

          <!-- Trash / Delete (Inline Confirmation, No Popup) -->
          {#if isConfirmingDelete}
            <div class="flex items-center gap-1 bg-rose-50 border border-rose-300 rounded-lg px-2 py-0.5 text-[11px] animate-in fade-in ml-0.5">
              <span class="text-rose-800 font-medium">{t("confirm_delete_q")}</span>
              <button
                onclick={() => {
                  isConfirmingDelete = false;
                  onDeleteSingle(session);
                }}
                class="px-1.5 py-0.5 rounded bg-rose-600 hover:bg-rose-700 text-white font-bold text-[10px] transition cursor-pointer"
              >
                {t("confirm")}
              </button>
              <button
                onclick={() => (isConfirmingDelete = false)}
                class="px-1 py-0.5 rounded text-slate-500 hover:text-slate-800 text-[10px] transition cursor-pointer"
              >
                {t("cancel")}
              </button>
            </div>
          {:else}
            <button
              onclick={() => (isConfirmingDelete = true)}
              class="p-1.5 rounded-lg bg-rose-50 hover:bg-rose-100 border border-rose-200 text-rose-700 transition cursor-pointer ml-0.5"
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
          class="text-base sm:text-lg font-bold text-slate-900 tracking-tight leading-snug select-text line-clamp-2"
          title={session.title}
        >
          {session.title || t("untitled_session")}
        </h2>
      </div>

      <!-- Bottom: Unified Info & Telemetry Card (A single soft rounded card container instead of stripes) -->
      <div class="bg-slate-50/80 rounded-xl p-2.5 border border-slate-200/70 flex flex-wrap items-center justify-between gap-2 text-xs text-slate-600">
        <!-- Left Metadata: Session ID & CWD & Updated -->
        <div class="flex items-center gap-2.5 text-[11px] font-mono min-w-0 flex-1 overflow-hidden">
          <!-- Session ID with Copy -->
          <button
            onclick={copySessionId}
            class="flex items-center gap-1 text-slate-600 hover:text-sky-600 transition cursor-pointer shrink-0"
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

          <span class="text-slate-300">·</span>

          <!-- CWD / Workspace -->
          <div class="flex items-center gap-1 truncate max-w-[220px] text-slate-500 font-sans" title={session.cwd || session.dirname}>
            <Folder class="h-3 w-3 text-slate-400 shrink-0" />
            <span class="truncate">{session.cwd || session.dirname || t("unknown_path")}</span>
          </div>

          <span class="text-slate-300 shrink-0">·</span>

          <!-- Updated time -->
          <div class="flex items-center gap-1 text-slate-400 font-sans text-[10px] shrink-0">
            <Clock class="h-3 w-3 shrink-0" />
            <span>{formatTime(session.updated_at)}</span>
          </div>
        </div>

        <!-- Right Telemetry: Pure Token Stats (No Price) -->
        {#if session.token_stats && session.token_stats.total_tokens > 0}
          <div class="flex items-center gap-2 text-[11px] font-mono shrink-0">
            <!-- Total Tokens -->
            <span class="inline-flex items-center gap-1 text-amber-900 font-bold bg-amber-50 px-1.5 py-0.5 rounded border border-amber-200/70" title="Total Tokens">
              <Zap class="h-3 w-3 text-amber-600 shrink-0" />
              <span>{formatTokens(session.token_stats.total_tokens)}</span>
            </span>

            <!-- In / Out -->
            <span class="text-slate-500 bg-white px-1.5 py-0.5 rounded border border-slate-200/70 text-[10px]">
              {t("in_out_tokens", { in: formatTokens(session.token_stats.prompt_tokens), out: formatTokens(session.token_stats.completion_tokens) })}
            </span>

            <!-- Cache Hit Rate -->
            {#if session.token_stats.cache_hit_rate !== undefined && session.token_stats.cache_hit_rate !== null && session.token_stats.cache_hit_rate > 0}
              <span class="inline-flex items-center gap-0.5 text-sky-700 bg-sky-50 px-1.5 py-0.5 rounded border border-sky-200/70 text-[10px] font-semibold">
                <Database class="h-3 w-3 text-sky-600 shrink-0" />
                <span>{t("cache_rate_short", { rate: session.token_stats.cache_hit_rate })}</span>
              </span>
            {/if}

            <!-- Speed tok/s -->
            {#if session.token_stats.tokens_per_second !== undefined && session.token_stats.tokens_per_second !== null && session.token_stats.tokens_per_second > 0}
              <span class="inline-flex items-center gap-0.5 text-emerald-700 bg-emerald-50 px-1.5 py-0.5 rounded border border-emerald-200/70 text-[10px] font-semibold" title="Token Speed">
                <Activity class="h-3 w-3 text-emerald-600 shrink-0" />
                <span>{session.token_stats.tokens_per_second.toFixed(0)} t/s</span>
              </span>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Child Subagents List (If any) -->
      {#if childSubagents.length > 0}
        <div class="flex items-center gap-1.5 overflow-x-auto text-[11px] pt-0.5">
          <span class="font-bold text-[10px] text-indigo-700 uppercase tracking-wider shrink-0 flex items-center gap-1">
            <Sparkles class="h-3 w-3" />
            <span>{t("mounted_subtasks", { n: childSubagents.length })}</span>
          </span>
          {#each childSubagents as sub}
            <button
              onclick={() => onJumpSession(sub)}
              class="inline-flex items-center gap-1 px-2 py-0.5 rounded-lg bg-indigo-50/70 hover:bg-indigo-100 border border-indigo-200 text-[11px] text-indigo-800 transition cursor-pointer shrink-0 font-medium"
            >
              <CornerDownRight class="h-2.5 w-2.5 text-indigo-600" />
              <span class="truncate max-w-[130px]">{sub.title || sub.id}</span>
              <span class="text-[9px] text-slate-400 font-mono">({formatBytes(sub.size_bytes)})</span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <!-- Segmented Tab Bar: Breaks Monotony & Controls View -->
    <div class="px-5 py-2 bg-slate-100/70 border-b border-slate-200/80 flex items-center justify-between gap-3 text-xs shrink-0 select-none">
      <!-- Left: Segmented Switcher Pills -->
      <div class="flex items-center bg-slate-200/70 p-0.5 rounded-lg border border-slate-300/50">
        <button
          onclick={() => (activeTab = "chat")}
          class="flex items-center gap-1.5 px-3 py-1 rounded-md text-xs transition cursor-pointer font-medium {activeTab === 'chat'
            ? 'bg-white text-slate-900 shadow-2xs font-semibold'
            : 'text-slate-600 hover:text-slate-900'}"
        >
          <MessageSquare class="h-3.5 w-3.5 text-sky-600" />
          <span>{t("tab_chat")}</span>
          <span class="px-1.5 py-0.2 rounded-full text-[10px] font-mono {activeTab === 'chat' ? 'bg-sky-50 text-sky-700' : 'text-slate-400'}">
            {messages.length}
          </span>
        </button>

        <button
          onclick={() => (activeTab = "overview")}
          class="flex items-center gap-1.5 px-3 py-1 rounded-md text-xs transition cursor-pointer font-medium {activeTab === 'overview'
            ? 'bg-white text-slate-900 shadow-2xs font-semibold'
            : 'text-slate-600 hover:text-slate-900'}"
        >
          <LayoutDashboard class="h-3.5 w-3.5 text-indigo-600" />
          <span>{t("tab_overview")}</span>
        </button>
      </div>

      <!-- Right: Action area according to active tab -->
      {#if activeTab === "chat"}
        <div class="flex items-center gap-2">
          <div class="relative w-52 sm:w-60">
            <Search class="absolute left-2.5 top-2 h-3.5 w-3.5 text-slate-400 pointer-events-none" />
            <input
              type="text"
              bind:value={inChatSearch}
              placeholder={t("search_chat")}
              class="w-full bg-white border border-slate-200/90 focus:border-sky-500 rounded-lg pl-7.5 pr-6 py-1 text-[11px] text-slate-800 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-sky-500 transition shadow-2xs"
            />
            {#if inChatSearch}
              <button
                onclick={() => (inChatSearch = "")}
                class="absolute right-1.5 top-1.5 text-slate-400 hover:text-slate-600 p-0.5 cursor-pointer"
                title={t("clear_search")}
              >
                <X class="h-3 w-3" />
              </button>
            {/if}
          </div>
          {#if inChatSearch}
            <span class="text-[10px] text-sky-700 font-mono shrink-0">
              {t("matched_messages", { n: visibleMessages.length })}
            </span>
          {/if}
        </div>
      {:else}
        <div class="text-[11px] text-slate-500 font-mono">
          <span>{session.platform} / {session.flavor || "default"}</span>
        </div>
      {/if}
    </div>

    {#if activeTab === "chat"}
      <!-- Chat Timeline Stream (Occupies 85%+ screen height) -->
      <div class="flex-1 overflow-y-auto p-4 space-y-4">
        {#if isLoadingMessages}
          <div class="h-48 flex items-center justify-center text-slate-400 text-xs gap-2">
            <div class="h-4 w-4 rounded-full border-2 border-sky-500 border-t-transparent animate-spin"></div>
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
                <div class="h-7 w-7 rounded-lg bg-sky-100 text-sky-700 flex items-center justify-center shrink-0 mt-0.5 shadow-2xs">
                  <User class="h-4 w-4" />
                </div>
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2 mb-1">
                    <span class="text-xs font-bold text-slate-800">{t("user_question")}</span>
                    {#if msg.time}
                      <span class="text-[10px] text-slate-400 font-mono">{msg.time}</span>
                    {/if}
                  </div>
                  <div class="bg-white border border-slate-200/90 rounded-2xl p-3.5 text-xs text-slate-800 leading-relaxed shadow-2xs whitespace-pre-wrap break-words selection:bg-sky-100">
                    {msg.text}
                  </div>
                </div>
              </div>
            {:else if msg.role === "assistant"}
              <!-- Assistant Message Card -->
              <div class="flex gap-3 max-w-4xl mx-auto">
                <div class="h-7 w-7 rounded-lg bg-indigo-100 text-indigo-700 flex items-center justify-center shrink-0 mt-0.5 shadow-2xs">
                  <Bot class="h-4 w-4" />
                </div>
                <div class="flex-1 min-w-0 space-y-2">
                  <div class="flex items-center gap-2">
                    <span class="text-xs font-bold text-indigo-900">{t("ai_assistant")}</span>
                    {#if msg.time}
                      <span class="text-[10px] text-slate-400 font-mono">{msg.time}</span>
                    {/if}
                  </div>

                  <!-- Thinking / CoT Block (Collapsible) -->
                  {#if msg.thinking}
                    <div class="rounded-xl border border-amber-200/80 bg-amber-50/40 overflow-hidden text-xs">
                      <button
                        onclick={() => toggleThinking(idx)}
                        class="w-full px-3 py-1.5 flex items-center justify-between text-amber-900 hover:bg-amber-100/50 transition cursor-pointer text-left"
                      >
                        <div class="flex items-center gap-1.5 font-medium">
                          <Sparkles class="h-3.5 w-3.5 text-amber-600" />
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
                        <div class="px-3 py-2 border-t border-amber-200/60 bg-white/70 text-[11px] text-slate-700 leading-relaxed whitespace-pre-wrap break-words font-mono">
                          {msg.thinking}
                        </div>
                      {/if}
                    </div>
                  {/if}

                  <!-- Tool Calls Block (Collapsible) -->
                  {#if msg.tool_calls && msg.tool_calls.length > 0}
                    <div class="rounded-xl border border-slate-200 bg-slate-50/80 overflow-hidden text-xs">
                      <button
                        onclick={() => toggleTool(idx)}
                        class="w-full px-3 py-1.5 flex items-center justify-between text-slate-800 hover:bg-slate-100 transition cursor-pointer text-left"
                      >
                        <div class="flex items-center gap-1.5 font-medium">
                          <Wrench class="h-3.5 w-3.5 text-sky-600" />
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
                        <div class="px-3 py-2 border-t border-slate-200 space-y-2 bg-slate-900 text-slate-200 font-mono text-[11px] rounded-b-xl overflow-x-auto">
                          {#each msg.tool_calls as tc}
                            <div class="space-y-1">
                              <div class="text-sky-400 font-bold flex items-center gap-1">
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

                  <!-- Text Body -->
                  {#if msg.text}
                    <div class="bg-white border border-slate-200/90 rounded-2xl p-3.5 text-xs text-slate-800 leading-relaxed shadow-2xs whitespace-pre-wrap break-words selection:bg-indigo-100">
                      {msg.text}
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
    {:else}
      <!-- Session Overview & Configuration Dashboard (Tab 2) -->
      <div class="flex-1 overflow-y-auto p-5 bg-slate-50/60 space-y-4">
        <div class="max-w-4xl mx-auto space-y-4">

          <!-- Card 1: 终端接续指令 (CLI Resume: No verbose descriptions, only buttons and terminal snippet) -->
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
                  onclick={resumeInTerminal}
                  class="flex items-center gap-1.5 px-3 py-1 text-xs font-semibold rounded-lg bg-emerald-600 hover:bg-emerald-500 active:bg-emerald-700 text-white transition cursor-pointer shadow-2xs"
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

          <!-- Card 2: 存储位置与工作目录 (Workspace & Storage) -->
          <div class="bg-white border border-slate-200/90 rounded-2xl p-4 shadow-2xs space-y-3">
            <div class="flex items-center gap-2">
              <div class="h-6 w-6 rounded-lg bg-sky-50 text-sky-600 flex items-center justify-center">
                <Folder class="h-3.5 w-3.5" />
              </div>
              <h3 class="text-xs font-bold text-slate-800">{t("storage_and_workspace")}</h3>
            </div>

            <div class="grid grid-cols-1 md:grid-cols-2 gap-3 pt-1">
              <!-- Storage Main Path -->
              <div class="bg-slate-50 border border-slate-100 rounded-xl p-3 space-y-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-[11px] font-medium text-slate-500 flex items-center gap-1">
                    <Database class="h-3 w-3 text-slate-400" />
                    {t("data_source_file")}
                  </span>
                  <div class="flex items-center gap-1">
                    <button
                      onclick={copyMainPath}
                      class="text-[10px] text-slate-500 hover:text-slate-800 flex items-center gap-0.5 p-1 rounded hover:bg-slate-200/60 transition cursor-pointer"
                      title={t("copy")}
                    >
                      {#if copiedPath}
                        <Check class="h-3 w-3 text-emerald-600" />
                        <span class="text-emerald-700">{t("copied")}</span>
                      {:else}
                        <Copy class="h-3 w-3" />
                        <span>{t("copy")}</span>
                      {/if}
                    </button>
                    {#if session.main_path}
                      <button
                        onclick={revealFinder}
                        class="text-[10px] text-sky-600 hover:text-sky-800 flex items-center gap-0.5 p-1 rounded hover:bg-sky-50 transition cursor-pointer"
                        title={t("open_in_finder")}
                      >
                        <ExternalLink class="h-3 w-3" />
                        <span>{t("reveal")}</span>
                      </button>
                    {/if}
                  </div>
                </div>
                <div class="text-[11px] font-mono text-slate-700 break-all select-all">
                  {session.main_path || t("unknown_or_consolidated")}
                </div>
              </div>

              <!-- Workspace CWD -->
              <div class="bg-slate-50 border border-slate-100 rounded-xl p-3 space-y-1.5">
                <div class="flex items-center justify-between">
                  <span class="text-[11px] font-medium text-slate-500 flex items-center gap-1">
                    <Folder class="h-3 w-3 text-slate-400" />
                    {t("workspace_cwd")}
                  </span>
                  {#if session.cwd}
                    <button
                      onclick={openTerminal}
                      class="text-[10px] text-emerald-600 hover:text-emerald-800 flex items-center gap-0.5 p-1 rounded hover:bg-emerald-50 transition cursor-pointer"
                      title={t("open_in_terminal")}
                    >
                      <Terminal class="h-3 w-3" />
                      <span>{t("open")}</span>
                    </button>
                  {/if}
                </div>
                <div class="text-[11px] font-mono text-slate-700 break-all select-all">
                  {session.cwd || session.dirname || t("unspecified_workspace")}
                </div>
              </div>
            </div>

            <!-- Meta statistics grid -->
            <div class="grid grid-cols-2 sm:grid-cols-4 gap-2 pt-2 border-t border-slate-100">
              <div class="p-2 rounded-lg bg-slate-50/70 text-center">
                <div class="text-[10px] text-slate-400">{t("disk_usage")}</div>
                <div class="text-xs font-bold text-slate-800 mt-0.5 font-mono">{formatBytes(session.size_bytes)}</div>
              </div>
              <div class="p-2 rounded-lg bg-slate-50/70 text-center">
                <div class="text-[10px] text-slate-400">{t("total_messages")}</div>
                <div class="text-xs font-bold text-slate-800 mt-0.5 font-mono">{t("messages_count", { n: messages.length })}</div>
              </div>
              <div class="p-2 rounded-lg bg-slate-50/70 text-center">
                <div class="text-[10px] text-slate-400">{t("total_turns")}</div>
                <div class="text-xs font-bold text-slate-800 mt-0.5 font-mono">{session.turn_count || 0} {t("turns")}</div>
              </div>
              <div class="p-2 rounded-lg bg-slate-50/70 text-center">
                <div class="text-[10px] text-slate-400">{t("session_structure")}</div>
                <div class="text-xs font-bold text-slate-800 mt-0.5">
                  {session.is_subagent ? t("subagent_task") : childSubagents.length > 0 ? t("main_with_children") : t("standalone_session")}
                </div>
              </div>
            </div>
          </div>

          <!-- Card 3: Token 深度分布与模型表现 (Token Stats) -->
          <div class="bg-white border border-slate-200/90 rounded-2xl p-4 shadow-2xs space-y-3">
            <div class="flex items-center justify-between">
              <div class="flex items-center gap-2">
                <div class="h-6 w-6 rounded-lg bg-amber-50 text-amber-600 flex items-center justify-center">
                  <Coins class="h-3.5 w-3.5" />
                </div>
                <h3 class="text-xs font-bold text-slate-800">{t("token_metrics")}</h3>
              </div>
              {#if session.token_stats}
                <span class="text-xs font-mono font-bold text-amber-800 bg-amber-50 px-2 py-0.5 rounded-full border border-amber-200/60">
                  {t("total_tokens_label")} {formatTokens(session.token_stats.total_tokens)}
                </span>
              {/if}
            </div>

            {#if session.token_stats && (session.token_stats.total_tokens || 0) > 0}
              <!-- Proportion bar -->
              {#if tokenBreakdown}
                <div class="space-y-1.5 pt-1">
                  <div class="h-2.5 w-full bg-slate-100 rounded-full overflow-hidden flex">
                    <div
                      class="bg-amber-400 h-full transition-all duration-300"
                      style="width: {tokenBreakdown.promptBarWidth}%"
                      title="Prompt: {tokenBreakdown.promptText}"
                    ></div>
                    <div
                      class="bg-indigo-500 h-full transition-all duration-300"
                      style="width: {tokenBreakdown.completionBarWidth}%"
                      title="Completion: {tokenBreakdown.completionText}"
                    ></div>
                    <div
                      class="bg-sky-400 h-full transition-all duration-300"
                      style="width: {tokenBreakdown.cacheBarWidth}%"
                      title="Cache: {tokenBreakdown.cacheText}"
                    ></div>
                  </div>
                  <div class="flex items-center justify-between text-[10px] text-slate-500 font-medium px-1 flex-wrap gap-2">
                    <div class="flex items-center gap-1.5 whitespace-nowrap">
                      <span class="h-2 w-2 rounded-full bg-amber-400 shrink-0"></span>
                      <span>{t("prompt_input")} ({tokenBreakdown.promptText})</span>
                    </div>
                    <div class="flex items-center gap-1.5 whitespace-nowrap">
                      <span class="h-2 w-2 rounded-full bg-indigo-500 shrink-0"></span>
                      <span>{t("completion_reply")} ({tokenBreakdown.completionText})</span>
                    </div>
                    <div class="flex items-center gap-1.5 whitespace-nowrap">
                      <span class="h-2 w-2 rounded-full bg-sky-400 shrink-0"></span>
                      <span>{t("cache_read")} ({tokenBreakdown.cacheText})</span>
                    </div>
                  </div>
                </div>
              {/if}

              <!-- Token details grid (Optimized numbers & zero wrapping) -->
              <div class="grid grid-cols-2 sm:grid-cols-4 gap-2.5 pt-2">
                <!-- Card 1: Prompt -->
                <div class="p-2.5 rounded-xl border border-amber-200/80 bg-amber-50/40 flex flex-col justify-between min-w-0">
                  <div class="flex items-center justify-between gap-1 text-[11px] text-slate-600 font-medium whitespace-nowrap min-w-0">
                    <span class="truncate">{t("prompt_input")}</span>
                    {#if tokenBreakdown}
                      <span class="text-[10px] text-amber-700 font-mono font-semibold shrink-0">{tokenBreakdown.promptText}</span>
                    {/if}
                  </div>
                  <div class="mt-2 min-w-0">
                    <div class="text-base sm:text-lg font-bold text-amber-950 font-mono tracking-tight leading-none truncate">
                      {formatTokens(session.token_stats.prompt_tokens || 0)}
                    </div>
                    <div class="text-[10px] text-slate-400 font-mono tracking-tight mt-1 truncate select-all" title="{formatNumber(session.token_stats.prompt_tokens || 0)} tokens">
                      {formatNumber(session.token_stats.prompt_tokens || 0)}
                    </div>
                  </div>
                </div>

                <!-- Card 2: Completion -->
                <div class="p-2.5 rounded-xl border border-indigo-200/80 bg-indigo-50/40 flex flex-col justify-between min-w-0">
                  <div class="flex items-center justify-between gap-1 text-[11px] text-slate-600 font-medium whitespace-nowrap min-w-0">
                    <span class="truncate">{t("completion_reply")}</span>
                    {#if tokenBreakdown}
                      <span class="text-[10px] text-indigo-700 font-mono font-semibold shrink-0">{tokenBreakdown.completionText}</span>
                    {/if}
                  </div>
                  <div class="mt-2 min-w-0">
                    <div class="text-base sm:text-lg font-bold text-indigo-950 font-mono tracking-tight leading-none truncate">
                      {formatTokens(session.token_stats.completion_tokens || 0)}
                    </div>
                    <div class="text-[10px] text-slate-400 font-mono tracking-tight mt-1 truncate select-all" title="{formatNumber(session.token_stats.completion_tokens || 0)} tokens">
                      {formatNumber(session.token_stats.completion_tokens || 0)}
                    </div>
                  </div>
                </div>

                <!-- Card 3: Cache Read -->
                <div class="p-2.5 rounded-xl border border-sky-200/80 bg-sky-50/40 flex flex-col justify-between min-w-0">
                  <div class="flex items-center justify-between gap-1 text-[11px] text-slate-600 font-medium whitespace-nowrap min-w-0">
                    <span class="truncate">{t("cache_read")}</span>
                    {#if tokenBreakdown}
                      <span class="text-[10px] text-sky-700 font-mono font-semibold shrink-0">{tokenBreakdown.cacheText}</span>
                    {/if}
                  </div>
                  <div class="mt-2 min-w-0">
                    <div class="text-base sm:text-lg font-bold text-sky-950 font-mono tracking-tight leading-none truncate">
                      {formatTokens(session.token_stats.cache_read_tokens || 0)}
                    </div>
                    <div class="text-[10px] text-slate-400 font-mono tracking-tight mt-1 truncate select-all" title="{formatNumber(session.token_stats.cache_read_tokens || 0)} tokens">
                      {formatNumber(session.token_stats.cache_read_tokens || 0)}
                    </div>
                  </div>
                </div>

                <!-- Card 4: Reasoning / CoT -->
                <div class="p-2.5 rounded-xl border border-purple-200/80 bg-purple-50/40 flex flex-col justify-between min-w-0">
                  <div class="flex items-center justify-between gap-1 text-[11px] text-slate-600 font-medium whitespace-nowrap min-w-0">
                    <span class="truncate">{t("reasoning_thinking")}</span>
                    {#if session.token_stats.reasoning_tokens && session.token_stats.total_tokens}
                      <span class="text-[10px] text-purple-700 font-mono font-semibold shrink-0">
                        {((session.token_stats.reasoning_tokens / session.token_stats.total_tokens) * 100).toFixed(1)}%
                      </span>
                    {/if}
                  </div>
                  <div class="mt-2 min-w-0">
                    {#if session.token_stats.reasoning_tokens && session.token_stats.reasoning_tokens > 0}
                      <div class="text-base sm:text-lg font-bold text-purple-950 font-mono tracking-tight leading-none truncate">
                        {formatTokens(session.token_stats.reasoning_tokens)}
                      </div>
                      <div class="text-[10px] text-slate-400 font-mono tracking-tight mt-1 truncate select-all" title="{formatNumber(session.token_stats.reasoning_tokens)} tokens">
                        {formatNumber(session.token_stats.reasoning_tokens)}
                      </div>
                    {:else}
                      <div class="text-sm font-semibold text-slate-400 font-mono tracking-tight leading-none">
                        -
                      </div>
                      <div class="text-[10px] text-slate-400 tracking-tight mt-1 truncate">
                        {t("no_reasoning")}
                      </div>
                    {/if}
                  </div>
                </div>
              </div>
            {:else}
              <div class="p-4 rounded-xl bg-slate-50 border border-slate-100 text-center text-xs text-slate-400">
                {t("no_token_stats")}
              </div>
            {/if}
          </div>

          <!-- Card 4: 任务层级与子代理树 (Hierarchy) -->
          <div class="bg-white border border-slate-200/90 rounded-2xl p-4 shadow-2xs space-y-3">
            <div class="flex items-center gap-2">
              <div class="h-6 w-6 rounded-lg bg-purple-50 text-purple-600 flex items-center justify-center">
                <Sparkles class="h-3.5 w-3.5" />
              </div>
              <h3 class="text-xs font-bold text-slate-800">{t("task_hierarchy")}</h3>
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
