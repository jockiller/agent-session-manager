<script lang="ts">
  import {
    ChevronDown,
    ChevronUp,
    FilePlus,
    FileText,
    Globe,
    Plus,
    Puzzle,
    Terminal,
    Trash2,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { configStore as store } from "$lib/config/store.svelte";
  import { MCP_TRANSPORTS, type MCPServer } from "$lib/config/types";
  import FieldLabel from "./FieldLabel.svelte";
  import SaveBar from "./SaveBar.svelte";
  import { btnBorder, btnGhost, btnPrimary, fieldClass } from "./styles";

  type LibraryPane = "agent" | "mcp";

  let pane = $state<LibraryPane>("agent");
  let showPaste = $state(false);
  let pasteText = $state("");
  let expandedID = $state<string | null>(null);

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

  async function parsePaste() {
    const ok = await store.importMCP(pasteText);
    if (ok) {
      showPaste = false;
      pasteText = "";
    }
  }
</script>

<div class="flex h-full min-h-0">
  <div class="w-[220px] shrink-0 border-r border-slate-200/80 bg-slate-50/80 overflow-y-auto p-2 space-y-0.5">
    <button
      type="button"
      class="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-left transition cursor-pointer {pane === 'agent'
        ? 'bg-white text-slate-900 font-semibold shadow-2xs border border-slate-200/80'
        : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900 border border-transparent'}"
      onclick={() => (pane = "agent")}
    >
      <span class="h-[26px] w-[26px] rounded-[7px] bg-sky-600 flex items-center justify-center shrink-0">
        <FileText class="h-3.5 w-3.5 text-white" />
      </span>
      <span class="min-w-0">
        <span class="block text-xs">{t("cfg_library_agent")}</span>
        <span class="block text-[10px] text-slate-400 font-normal">{t("cfg_library_agent_sub")}</span>
      </span>
    </button>
    <button
      type="button"
      class="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-left transition cursor-pointer {pane === 'mcp'
        ? 'bg-white text-slate-900 font-semibold shadow-2xs border border-slate-200/80'
        : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900 border border-transparent'}"
      onclick={() => (pane = "mcp")}
    >
      <span class="h-[26px] w-[26px] rounded-[7px] bg-sky-600 flex items-center justify-center shrink-0">
        <Puzzle class="h-3.5 w-3.5 text-white" />
      </span>
      <span class="min-w-0">
        <span class="block text-xs">{t("cfg_library_mcp")}</span>
        <span class="block text-[10px] text-slate-400 font-normal">
          {t("cfg_n_servers", { n: store.document.library.mcp.length })}
        </span>
      </span>
    </button>
  </div>

  <div class="flex-1 min-w-0 flex flex-col min-h-0">
    {#if pane === "agent"}
      <div class="flex-1 min-h-0 flex flex-col px-5 pt-4 pb-4 gap-2.5">
        <div>
          <div class="text-sm font-semibold text-slate-800">AGENTS.md</div>
          <p class="text-[11px] text-slate-400">{t("cfg_agents_md_hint")}</p>
        </div>
        <textarea
          class="flex-1 min-h-0 w-full font-mono text-xs leading-relaxed bg-white border border-slate-200/90 rounded-xl p-3 text-slate-800 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-sky-500 focus:border-sky-500 select-text resize-none"
          value={store.document.library.instructions}
          oninput={(e) => store.setInstructions(e.currentTarget.value)}
        ></textarea>
      </div>
    {:else}
      <div class="flex-1 min-h-0 flex flex-col">
        <div class="shrink-0 flex items-center gap-2 px-5 pt-4 pb-2">
          <div class="flex-1 min-w-0">
            <div class="text-sm font-semibold text-slate-800">MCP</div>
            <p class="text-[11px] text-slate-400">{t("cfg_n_servers", { n: store.document.library.mcp.length })}</p>
          </div>
          <button type="button" class={btnBorder} onclick={() => (showPaste = true)}>
            <FilePlus class="h-3 w-3" />
            {t("cfg_parse")}
          </button>
          <button type="button" class={btnBorder} onclick={() => store.addMCP()}>
            <Plus class="h-3 w-3" />
            {t("cfg_add")}
          </button>
        </div>
        {#if store.document.library.mcp.length === 0}
          <div class="flex-1 flex flex-col items-center justify-center gap-2 text-center px-8">
            <Puzzle class="h-8 w-8 text-slate-300" />
            <div class="text-sm font-semibold text-slate-600">{t("cfg_no_mcp")}</div>
            <p class="text-xs text-slate-400">{t("cfg_no_mcp_desc")}</p>
          </div>
        {:else}
          <div class="flex-1 overflow-y-auto px-5 pb-4 space-y-2">
            {#each store.document.library.mcp as server, index (`mcp-${index}`)}
              {@const expanded = expandedID === server.id}
              <div class="bg-white border border-slate-200/90 rounded-xl p-3 shadow-2xs space-y-2">
                <div class="flex items-center gap-2">
                  {#if server.transport === "stdio"}
                    <Terminal class="h-4 w-4 text-sky-600 shrink-0" />
                  {:else}
                    <Globe class="h-4 w-4 text-sky-600 shrink-0" />
                  {/if}
                  <div class="min-w-0 flex-1">
                    <div class="text-xs font-medium text-slate-800 truncate">{server.id}</div>
                    <div class="text-[10px] text-slate-400 truncate">{mcpSubtitle(server)}</div>
                  </div>
                  <button
                    type="button"
                    class="p-1 rounded text-slate-400 hover:text-slate-700 hover:bg-slate-100 cursor-pointer"
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
                    class="p-1 rounded text-slate-400 hover:text-rose-600 hover:bg-rose-50 cursor-pointer"
                    title={t("cfg_delete")}
                    onclick={() => store.removeMCP(server.id)}
                  >
                    <Trash2 class="h-3.5 w-3.5" />
                  </button>
                </div>
                {#if expanded}
                  <div class="space-y-2 pt-1">
                    <FieldLabel title={t("cfg_name")}>
                      <input
                        class={fieldClass}
                        placeholder={t("cfg_name")}
                        value={server.id}
                        oninput={(e) => setMcpId(server.id, e.currentTarget.value)}
                      />
                    </FieldLabel>
                    <FieldLabel title={t("cfg_transport")}>
                      <div class="flex rounded-lg border border-slate-200 bg-slate-100/80 p-0.5">
                        {#each MCP_TRANSPORTS as transport}
                          <button
                            type="button"
                            class="flex-1 px-2 py-1 rounded-md text-[11px] font-medium transition cursor-pointer {server.transport === transport
                              ? 'bg-white text-slate-900 shadow-2xs'
                              : 'text-slate-500 hover:text-slate-800'}"
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
      </div>
    {/if}

    <SaveBar
      isDirty={store.isDirty}
      hint={t("cfg_save_hint_library")}
      saveTitle={t("cfg_save")}
      onSave={() => { void store.save(); }}
      onDiscard={() => store.revert()}
    />
  </div>
</div>

{#if showPaste}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/40">
    <div class="w-[560px] max-w-[calc(100%-2rem)] h-[420px] max-h-[calc(100%-2rem)] bg-white rounded-2xl shadow-xl border border-slate-200 p-5 flex flex-col gap-3">
      <div class="text-sm font-semibold text-slate-800">{t("cfg_parse_mcp")}</div>
      <p class="text-xs text-slate-500 leading-relaxed">{t("cfg_parse_mcp_hint")}</p>
      <textarea
        class="flex-1 min-h-0 w-full font-mono text-xs leading-relaxed bg-slate-50 border border-slate-200 rounded-lg p-2.5 text-slate-800 focus:outline-none focus:ring-1 focus:ring-sky-500 focus:border-sky-500 select-text resize-none"
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
