<script lang="ts">
  import {
    Check,
    ChevronDown,
    ChevronUp,
    FilePlus,
    Globe,
    Import,
    LoaderCircle,
    Plus,
    Puzzle,
    RefreshCw,
    Terminal,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { configStore as store } from "$lib/config/store.svelte";
  import { AGENT_TITLE, isAgentKind, resolvedHint } from "$lib/config/document";
  import { MCP_TRANSPORTS, type AgentKind, type DiscoveredDiskMcp, type MCPServer } from "$lib/config/types";
  import FieldLabel from "./FieldLabel.svelte";
  import SaveBar from "./SaveBar.svelte";
  import { btnBorder, btnGhost, btnPrimary, fieldClass } from "./styles";

  let showPaste = $state(false);
  let pasteText = $state("");
  let expandedID = $state<string | null>(null);
  let bindModalServerID = $state<string | null>(null);
  let showImportModal = $state(false);
  let scanningDiskMcp = $state(false);
  let discoveredMcp = $state<DiscoveredDiskMcp[]>([]);
  let bindModalServer = $derived(
    bindModalServerID
      ? (store.document.library.mcp.find((s) => s.id === bindModalServerID) ?? null)
      : null,
  );

  function mcpSubtitle(server: MCPServer): string {
    if (server.transport === "stdio") {
      const command = [server.command, ...server.args].filter(Boolean).join(" ");
      return command || "stdio";
    }
    return server.url || server.transport;
  }

  function argsText(server: MCPServer): string {
    return server.args.join(" ");
  }

  function envText(server: MCPServer): string {
    return Object.keys(server.env)
      .sort()
      .map((key) => `${key}=${server.env[key]}`)
      .join("\n");
  }

  function setArgs(id: string, value: string) {
    store.updateMCP(id, (server) => {
      server.args = value.split(" ").filter(Boolean);
    });
  }

  function setEnv(id: string, value: string) {
    store.updateMCP(id, (server) => {
      const env: Record<string, string> = {};
      for (const line of value.split(/\r?\n/)) {
        const text = line.trim();
        const eq = text.indexOf("=");
        if (eq < 0) continue;
        const key = text.slice(0, eq).trim();
        const raw = text.slice(eq + 1).trim();
        if (key) env[key] = raw;
      }
      server.env = env;
    });
  }

  function setMcpId(current: string, next: string) {
    store.updateMCP(current, (server) => {
      server.id = next;
    });
    if (expandedID === current) expandedID = next;
  }

  function agentUsesMcp(kind: AgentKind, mcpId: string): boolean {
    return store.document.agents[kind]?.mcpIDs.includes(mcpId) ?? false;
  }

  function toggleAgentMcp(kind: AgentKind, mcpId: string) {
    store.updateAgentSettings(kind, (settings) => {
      if (settings.mcpIDs.includes(mcpId)) {
        settings.mcpIDs = settings.mcpIDs.filter((id) => id !== mcpId);
      } else {
        settings.mcpIDs = [...settings.mcpIDs, mcpId];
      }
    });
  }

  async function parsePaste() {
    const ok = await store.importMCP(pasteText);
    if (ok) {
      showPaste = false;
      pasteText = "";
    }
  }

  async function openImportModal() {
    showImportModal = true;
    scanningDiskMcp = true;
    try {
      discoveredMcp = await store.scanDiskMcp();
    } finally {
      scanningDiskMcp = false;
    }
  }

  function handleImportEntry(entry: DiscoveredDiskMcp) {
    store.importDiskMcp(entry);
    discoveredMcp = discoveredMcp.map((item) =>
      item.server.id === entry.server.id ? { ...item, inLibrary: true } : item,
    );
  }
</script>

<div class="flex-1 min-h-0 flex flex-col bg-slate-50 dark:bg-slate-950">
  <div class="shrink-0 flex items-center gap-2 px-6 pt-5 pb-3 border-b border-slate-200/70 dark:border-slate-800 bg-white/70 dark:bg-slate-900/70">
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2">
        <Puzzle class="h-4 w-4 text-claude-600 dark:text-claude-400" />
        <h1 class="text-base font-bold text-slate-900 dark:text-slate-100">{t("nav_mcp")}</h1>
      </div>
      <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
        {t("cfg_n_servers", { n: store.document.library.mcp.length })} · {t("mcp_assign_hint")}
      </p>
    </div>
    <button type="button" class={btnBorder} onclick={() => { void openImportModal(); }}>
      <Import class="h-3.5 w-3.5" />
      {t("mcp_import_disk")}
    </button>
    <button type="button" class={btnBorder} onclick={() => (showPaste = true)}>
      <FilePlus class="h-3.5 w-3.5" />
      {t("cfg_parse")}
    </button>
    <button type="button" class={btnBorder} onclick={() => store.addMCP()}>
      <Plus class="h-3.5 w-3.5" />
      {t("cfg_add")}
    </button>
    <button
      type="button"
      class={btnPrimary}
      disabled={store.isSyncingMcp}
      onclick={() => { void store.saveAndSyncMcp(); }}
    >
      {#if store.isSyncingMcp}
        <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
      {:else}
        <RefreshCw class="h-3.5 w-3.5" />
      {/if}
      {t("mcp_sync_agents")}
    </button>
  </div>

  {#if store.document.library.mcp.length === 0}
    <div class="flex-1 flex flex-col items-center justify-center gap-2 text-center px-8">
      <Puzzle class="h-8 w-8 text-slate-300 dark:text-slate-600" />
      <div class="text-sm font-semibold text-slate-600 dark:text-slate-300">{t("cfg_no_mcp")}</div>
      <p class="text-xs text-slate-400 dark:text-slate-500">{t("cfg_no_mcp_desc")}</p>
    </div>
  {:else}
    <div class="flex-1 overflow-y-auto px-6 py-4 space-y-2.5 max-w-5xl w-full mx-auto">
      {#each store.document.library.mcp as server, index (`mcp-${index}`)}
        {@const expanded = expandedID === server.id}
        {@const boundRows = store.rankedAgents().filter((row) => agentUsesMcp(row.kind, server.id))}
        <div class="bg-white dark:bg-slate-900 border border-slate-200/90 dark:border-slate-700/80 rounded-xl p-3.5 shadow-2xs space-y-2.5">
          <div class="flex items-center gap-2.5">
            {#if server.transport === "stdio"}
              <Terminal class="h-4 w-4 text-claude-600 dark:text-claude-400 shrink-0" />
            {:else}
              <Globe class="h-4 w-4 text-claude-600 dark:text-claude-400 shrink-0" />
            {/if}
            <div class="min-w-0 flex-1">
              <div class="text-xs font-semibold text-slate-800 dark:text-slate-100 truncate">{server.id}</div>
              <div class="text-[10px] text-slate-400 dark:text-slate-500 truncate">{mcpSubtitle(server)}</div>
            </div>
            <button
              type="button"
              class="p-1 rounded text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer"
              title={expanded ? t("cfg_collapse") : t("cfg_expand")}
              onclick={() => (expandedID = expanded ? null : server.id)}
            >
              {#if expanded}
                <ChevronUp class="h-3.5 w-3.5" />
              {:else}
                <ChevronDown class="h-3.5 w-3.5" />
              {/if}
            </button>
            <button
              type="button"
              class="p-1 rounded text-slate-400 hover:text-rose-600 hover:bg-rose-50 dark:hover:bg-rose-950/60 cursor-pointer"
              title={t("cfg_delete")}
              onclick={() => store.removeMCP(server.id)}
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
          </div>

          <!-- Bound agents (inline compact pills + modal trigger) -->
          <div class="flex items-center gap-1.5 flex-wrap pt-1.5 border-t border-slate-100 dark:border-slate-800">
            <span class="text-[10px] text-slate-400 dark:text-slate-500 mr-1">{t("mcp_enabled_agents")}:</span>
            {#if boundRows.length === 0}
              <span class="text-[11px] text-slate-400 dark:text-slate-500">{t("cfg_none_bound")}</span>
            {:else}
              {#each boundRows as row (row.kind)}
                {@const tip = resolvedHint(row.kind, store.document.agents[row.kind]?.profile ?? "")}
                <button
                  type="button"
                  title="{tip ? `${tip} · ` : ''}{t('cfg_unbind_tip')}"
                  class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-medium bg-claude-600 text-white hover:bg-claude-700 transition cursor-pointer {row.installed ? '' : 'opacity-80'}"
                  onclick={() => toggleAgentMcp(row.kind, server.id)}
                >
                  <span>{AGENT_TITLE[row.kind]}</span>
                  <X class="h-3 w-3 opacity-80" />
                </button>
              {/each}
            {/if}
            <button
              type="button"
              class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-medium border border-dashed border-slate-300 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:border-claude-500 hover:text-claude-600 dark:hover:text-claude-400 transition cursor-pointer"
              onclick={() => (bindModalServerID = server.id)}
            >
              <Plus class="h-3 w-3" />
              <span>{t("cfg_bind_more")}</span>
            </button>
          </div>

          {#if expanded}
            <div class="space-y-2 pt-2 border-t border-slate-100 dark:border-slate-800">
              <FieldLabel title={t("cfg_name")}>
                <input
                  class={fieldClass}
                  placeholder={t("cfg_name")}
                  value={server.id}
                  oninput={(e) => setMcpId(server.id, e.currentTarget.value)}
                />
              </FieldLabel>
              <FieldLabel title={t("cfg_transport")}>
                <div class="flex rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800 p-0.5">
                  {#each MCP_TRANSPORTS as transport}
                    <button
                      type="button"
                      class="flex-1 px-2 py-1 rounded-md text-[11px] font-medium transition cursor-pointer {server.transport === transport
                        ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
                        : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
                      onclick={() => store.updateMCP(server.id, (item) => { item.transport = transport; })}
                    >
                      {transport}
                    </button>
                  {/each}
                </div>
              </FieldLabel>
              {#if server.transport === "stdio"}
                <FieldLabel title={t("cfg_command")}>
                  <input
                    class={fieldClass}
                    placeholder={t("cfg_command")}
                    value={server.command}
                    oninput={(e) => store.updateMCP(server.id, (item) => { item.command = e.currentTarget.value; })}
                  />
                </FieldLabel>
                <FieldLabel title={t("cfg_args")}>
                  <input
                    class={fieldClass}
                    placeholder={t("cfg_args")}
                    value={argsText(server)}
                    oninput={(e) => setArgs(server.id, e.currentTarget.value)}
                  />
                </FieldLabel>
              {:else}
                <FieldLabel title={t("cfg_url")}>
                  <input
                    class={fieldClass}
                    placeholder={t("cfg_url")}
                    value={server.url}
                    oninput={(e) => store.updateMCP(server.id, (item) => { item.url = e.currentTarget.value; })}
                  />
                </FieldLabel>
              {/if}
              <FieldLabel title={t("cfg_env")}>
                <textarea
                  class="{fieldClass} min-h-[72px] resize-y font-mono"
                  placeholder={t("cfg_env")}
                  value={envText(server)}
                  oninput={(e) => setEnv(server.id, e.currentTarget.value)}
                ></textarea>
              </FieldLabel>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <SaveBar
    isDirty={store.isDirty}
    hint={t("mcp_sync_bar_hint")}
    saveTitle={t("cfg_save_and_sync")}
    onSave={() => { void store.saveAndSyncMcp(); }}
    onDiscard={() => store.revert()}
  />
</div>

{#if showPaste}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 backdrop-blur-xs">
    <div class="w-[560px] max-w-[calc(100%-2rem)] h-[420px] max-h-[calc(100%-2rem)] bg-white dark:bg-slate-900 rounded-2xl shadow-xl border border-slate-200 dark:border-slate-700 p-5 flex flex-col gap-3">
      <div class="text-sm font-semibold text-slate-800 dark:text-slate-100">{t("cfg_parse_mcp")}</div>
      <p class="text-xs text-slate-500 dark:text-slate-400 leading-relaxed">{t("cfg_parse_mcp_hint")}</p>
      <textarea
        class="flex-1 min-h-0 w-full font-mono text-xs leading-relaxed bg-slate-50 dark:bg-slate-950 border border-slate-200 dark:border-slate-700 rounded-lg p-2.5 text-slate-800 dark:text-slate-100 focus:outline-none focus:ring-1 focus:ring-claude-500 focus:border-claude-500 select-text resize-none"
        value={pasteText}
        oninput={(e) => (pasteText = e.currentTarget.value)}
      ></textarea>
      <div class="flex items-center justify-end gap-2">
        <button
          type="button"
          class={btnGhost}
          onclick={() => {
            showPaste = false;
            pasteText = "";
          }}
        >
          {t("cfg_cancel")}
        </button>
        <button
          type="button"
          class={btnPrimary}
          disabled={!pasteText.trim()}
          onclick={() => { void parsePaste(); }}
        >
          {t("cfg_parse_and_add")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if bindModalServer}
  {@const server = bindModalServer}
  {@const boundCount = store.rankedAgents().filter((row) => agentUsesMcp(row.kind, server.id)).length}
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    role="presentation"
    onclick={() => (bindModalServerID = null)}
  >
    <div
      class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-lg max-h-[75vh] flex flex-col overflow-hidden"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === "Escape") bindModalServerID = null; }}
    >
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("cfg_bind_modal_title")}</span>
          <span class="px-2 py-0.5 rounded-md text-[10px] font-mono bg-claude-50 dark:bg-claude-950/60 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800 truncate">
            {server.id}
          </span>
        </div>
        <button type="button" onclick={() => (bindModalServerID = null)} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer shrink-0">
          <X class="h-4 w-4" />
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-3 space-y-1.5">
        {#each store.rankedAgents() as row (row.kind)}
          {@const checked = agentUsesMcp(row.kind, server.id)}
          {@const hint = resolvedHint(row.kind, store.document.agents[row.kind]?.profile ?? "")}
          <button
            type="button"
            onclick={() => toggleAgentMcp(row.kind, server.id)}
            class="w-full text-left px-3 py-2.5 rounded-xl border transition cursor-pointer flex items-center gap-3 {checked
              ? 'border-claude-500 bg-claude-50/60 dark:bg-claude-950/30'
              : 'border-slate-200 dark:border-slate-700/80 hover:border-slate-300 dark:hover:border-slate-600'} {row.installed ? '' : 'opacity-70'}"
          >
            <span
              class="h-4 w-4 rounded flex items-center justify-center shrink-0 border {checked
                ? 'bg-claude-600 border-claude-600 text-white'
                : 'border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-800'}"
            >
              {#if checked}
                <Check class="h-3 w-3" />
              {/if}
            </span>
            <span class="min-w-0 flex-1">
              <span class="flex items-center gap-1.5">
                <span class="text-xs font-semibold text-slate-800 dark:text-slate-100">{AGENT_TITLE[row.kind]}</span>
                {#if !row.installed}
                  <span class="px-1.5 py-0.2 rounded text-[9px] bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400">
                    {t("cfg_not_installed")}
                  </span>
                {/if}
              </span>
              {#if hint}
                <span class="block text-[10px] font-mono text-slate-400 dark:text-slate-500 truncate">{hint}</span>
              {/if}
            </span>
          </button>
        {/each}
      </div>
      <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 flex items-center justify-between">
        <span class="text-[11px] text-slate-500 dark:text-slate-400">{t("cfg_bound_count", { n: boundCount })}</span>
        <button type="button" class={btnPrimary} onclick={() => (bindModalServerID = null)}>
          {t("cfg_bind_done")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showImportModal}
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    role="presentation"
    onclick={() => (showImportModal = false)}
  >
    <div
      class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-lg max-h-[75vh] flex flex-col overflow-hidden"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === "Escape") showImportModal = false; }}
    >
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
        <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("mcp_import_title")}</span>
        <button type="button" onclick={() => (showImportModal = false)} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer shrink-0">
          <X class="h-4 w-4" />
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-3 space-y-2">
        {#if scanningDiskMcp}
          <div class="py-10 flex items-center justify-center gap-2 text-xs text-slate-400">
            <LoaderCircle class="h-4 w-4 animate-spin" />
          </div>
        {:else if discoveredMcp.length === 0}
          <p class="py-8 text-center text-xs text-slate-400">{t("mcp_import_empty")}</p>
        {:else}
          {#each discoveredMcp as entry (entry.server.id)}
            <button
              type="button"
              onclick={() => handleImportEntry(entry)}
              class="w-full text-left p-3 rounded-xl border border-slate-200 dark:border-slate-700 hover:border-claude-400 hover:bg-claude-50/40 dark:hover:bg-claude-950/30 transition cursor-pointer space-y-1.5"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="text-xs font-bold text-slate-800 dark:text-slate-100">{entry.server.id}</span>
                {#if entry.inLibrary}
                  <span class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-300 border border-emerald-200/80 dark:border-emerald-800">
                    {t("mcp_in_library")}
                  </span>
                {/if}
              </div>
              <div class="text-[10px] font-mono text-slate-400 truncate">{mcpSubtitle(entry.server)}</div>
              <div class="flex items-center gap-1 flex-wrap">
                {#each entry.agents as rawKind (rawKind)}
                  <span class="px-1.5 py-0.2 rounded text-[10px] bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300">
                    {isAgentKind(rawKind) ? AGENT_TITLE[rawKind] : rawKind}
                  </span>
                {/each}
              </div>
            </button>
          {/each}
        {/if}
      </div>
      <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 flex items-center justify-end">
        <button type="button" class={btnPrimary} onclick={() => (showImportModal = false)}>
          {t("cfg_bind_done")}
        </button>
      </div>
    </div>
  </div>
{/if}
