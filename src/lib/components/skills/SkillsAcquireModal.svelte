<script lang="ts">
  import {
    AlertTriangle,
    Check,
    Download,
    FolderOpen,
    Globe,
    LoaderCircle,
    Plus,
    RefreshCw,
    Search,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { AGENT_TITLE, invokeError, isAgentKind } from "$lib/config/document";
  import { configStore } from "$lib/config/store.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import type { DiscoveredDiskSkill, RemoteSkillCandidate } from "$lib/config/types";
  import { btnBorder, btnPrimary, chipIdle, chipOn, fieldClass } from "../config/styles";

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();

  type AcquireTab = "discover" | "url" | "local";
  let tab = $state<AcquireTab>("discover");

  let searchQuery = $state("");
  let repoFilter = $state("all");
  let showRepoManager = $state(false);
  let newRepoInput = $state("");
  let newRepoLabel = $state("");

  let urlInput = $state("");
  let urlError = $state("");
  let urlCandidates = $state<RemoteSkillCandidate[]>([]);

  let scanningLocal = $state(false);
  let localSkills = $state<DiscoveredDiskSkill[]>([]);
  let manualLocalPath = $state("");

  let previewCandidate = $state<RemoteSkillCandidate | null>(null);
  let targetAgents = $state<string[]>([]);
  let wasOpen = false;

  let installedAgents = $derived(configStore.rankedAgents());

  let filteredDiscover = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    return skillsStore.remoteCandidates.filter((c) => {
      if (repoFilter !== "all" && c.repoId !== repoFilter) return false;
      if (!q) return true;
      return (
        c.id.toLowerCase().includes(q) ||
        c.name.toLowerCase().includes(q) ||
        c.description.toLowerCase().includes(q)
      );
    });
  });

  $effect(() => {
    if (open && !wasOpen) {
      wasOpen = true;
      targetAgents = configStore
        .rankedAgents()
        .filter((r) => r.installed)
        .map((r) => r.kind);
      if (skillsStore.remoteCandidates.length === 0 && !skillsStore.discovering) {
        void skillsStore.discoverRemote(false).then(() => {
          if (!previewCandidate) {
            previewCandidate = skillsStore.remoteCandidates[0] ?? null;
          }
        });
      } else if (!previewCandidate) {
        previewCandidate = skillsStore.remoteCandidates[0] ?? null;
      }
    } else if (!open) {
      wasOpen = false;
    }
  });

  function switchTab(next: AcquireTab) {
    tab = next;
    if (next === "discover") {
      previewCandidate = filteredDiscover[0] ?? skillsStore.remoteCandidates[0] ?? null;
    } else if (next === "url") {
      previewCandidate = urlCandidates[0] ?? null;
    } else if (next === "local") {
      void refreshLocal();
    }
  }

  function toggleTarget(kind: string) {
    if (targetAgents.includes(kind)) {
      targetAgents = targetAgents.filter((k) => k !== kind);
    } else {
      targetAgents = [...targetAgents, kind];
    }
  }

  async function addRepo() {
    const raw = newRepoInput.trim();
    if (!raw) return;
    await skillsStore.saveRepos([
      ...skillsStore.repos,
      {
        id: raw,
        label: newRepoLabel.trim() || raw,
        url: raw,
        branch: "",
        enabled: true,
        builtin: false,
      },
    ]);
    newRepoInput = "";
    newRepoLabel = "";
    await skillsStore.discoverRemote(true);
  }

  async function toggleRepo(id: string) {
    const next = skillsStore.repos.map((r) =>
      r.id === id ? { ...r, enabled: !r.enabled } : r,
    );
    await skillsStore.saveRepos(next);
  }

  async function removeRepo(id: string) {
    const next = skillsStore.repos.filter((r) => r.id !== id || r.builtin);
    await skillsStore.saveRepos(next);
    if (repoFilter === id) repoFilter = "all";
  }

  async function handleFetchUrl() {
    const raw = urlInput.trim();
    if (!raw) return;
    urlError = "";
    try {
      urlCandidates = await skillsStore.fetchRemoteUrl(raw);
      previewCandidate = urlCandidates[0] ?? null;
    } catch (err) {
      urlError = invokeError(err);
      urlCandidates = [];
      previewCandidate = null;
    }
  }

  async function handleInstall(item: RemoteSkillCandidate) {
    const installed = await skillsStore.installRemote(item, targetAgents);
    if (installed) {
      urlCandidates = urlCandidates.map((c) =>
        c.id === item.id ? { ...c, inLibrary: true } : c,
      );
      if (previewCandidate?.id === item.id) {
        previewCandidate = { ...previewCandidate, inLibrary: true };
      }
    }
  }

  async function refreshLocal() {
    scanningLocal = true;
    try {
      localSkills = await skillsStore.scanDisk();
    } finally {
      scanningLocal = false;
    }
  }

  async function handleImportLocal(item: DiscoveredDiskSkill) {
    const linkAgents = item.sourceAgents.filter((tag) => isAgentKind(tag));
    const entry = await skillsStore.importFromDisk(item.sourcePath, item.id, linkAgents);
    if (entry) {
      localSkills = localSkills.map((s) =>
        s.id === item.id ? { ...s, inLibrary: true } : s,
      );
    }
  }

  async function handleImportManual() {
    const raw = manualLocalPath.trim();
    if (!raw) return;
    const entry = await skillsStore.importFromDisk(raw, null, targetAgents);
    if (entry) {
      manualLocalPath = "";
      onClose();
    }
  }

  function sourceTagLabel(tag: string): string {
    if (tag === "shared_agents") return t("skills_shared_agents");
    return isAgentKind(tag) ? AGENT_TITLE[tag] : tag;
  }
</script>

{#snippet candidateListAndPreview(items: RemoteSkillCandidate[], emptyText: string)}
  <div class="flex-1 min-h-0 flex overflow-hidden">
    <!-- Left candidates list -->
    <div class="w-[270px] shrink-0 border-r border-slate-200/80 dark:border-slate-800 overflow-y-auto p-3 space-y-1.5">
      {#if items.length === 0}
        <p class="py-10 text-center text-xs text-slate-400">{emptyText}</p>
      {:else}
        {#each items as item (`${item.repoId}:${item.subpath}:${item.id}`)}
          {@const selected = previewCandidate?.id === item.id && previewCandidate?.repoId === item.repoId && previewCandidate?.subpath === item.subpath}
          <button
            type="button"
            onclick={() => (previewCandidate = item)}
            class="w-full text-left p-2.5 rounded-xl border transition cursor-pointer space-y-1 {selected
              ? 'border-claude-500 bg-claude-50/50 dark:bg-claude-950/30'
              : 'border-slate-200/80 dark:border-slate-700/80 hover:border-slate-300 dark:hover:border-slate-600'}"
          >
            <div class="flex items-center justify-between gap-1.5">
              <span class="text-xs font-semibold text-slate-800 dark:text-slate-100 truncate">{item.name}</span>
              {#if item.inLibrary}
                <span class="px-1.5 py-0.2 rounded text-[9px] font-medium bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-300 border border-emerald-200/80 dark:border-emerald-800 shrink-0">
                  {t("mcp_in_library")}
                </span>
              {/if}
            </div>
            {#if item.description}
              <div class="text-[11px] text-slate-500 dark:text-slate-400 line-clamp-2">{item.description}</div>
            {/if}
            <div class="flex items-center gap-1.5 text-[10px] font-mono text-slate-400 truncate">
              <span class="truncate">{item.repoLabel}</span>
              {#if item.hasScripts}
                <span class="text-amber-600 dark:text-amber-400 shrink-0">· scripts</span>
              {/if}
            </div>
          </button>
        {/each}
      {/if}
    </div>

    <!-- Right preview + install pane -->
    <div class="flex-1 min-w-0 flex flex-col min-h-0 bg-slate-50/50 dark:bg-slate-950/40">
      {#if !previewCandidate}
        <div class="flex-1 flex items-center justify-center p-6 text-xs text-slate-400 text-center">
          {t("skills_select_preview")}
        </div>
      {:else}
        <div class="p-4 border-b border-slate-200/80 dark:border-slate-800 space-y-2.5 bg-white/80 dark:bg-slate-900/80">
          <div class="flex items-start justify-between gap-2">
            <div class="min-w-0">
              <div class="flex items-center gap-2 flex-wrap">
                <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{previewCandidate.name}</span>
                <span class="px-1.5 py-0.5 rounded text-[10px] font-mono bg-slate-100 dark:bg-slate-800 text-slate-500">{previewCandidate.id}</span>
                {#if previewCandidate.commit}
                  <span class="px-1.5 py-0.5 rounded text-[10px] font-mono bg-slate-100 dark:bg-slate-800 text-slate-500">@{previewCandidate.commit}</span>
                {/if}
              </div>
              <div class="text-[10px] font-mono text-slate-400 truncate mt-0.5">{previewCandidate.repoUrl}</div>
            </div>
            <button
              type="button"
              class={btnPrimary}
              disabled={skillsStore.saving}
              onclick={() => { if (previewCandidate) void handleInstall(previewCandidate); }}
            >
              {#if skillsStore.saving}
                <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
              {:else}
                <Download class="h-3.5 w-3.5" />
              {/if}
              {previewCandidate.inLibrary ? t("skills_reinstall_btn") : t("skills_install_btn")}
            </button>
          </div>

          {#if previewCandidate.hasScripts}
            <div class="flex items-center gap-2 px-2.5 py-1.5 rounded-lg bg-amber-50 dark:bg-amber-950/40 border border-amber-200/80 dark:border-amber-800 text-[11px] text-amber-800 dark:text-amber-300">
              <AlertTriangle class="h-3.5 w-3.5 shrink-0" />
              <span>{t("skills_script_warning")}</span>
            </div>
          {/if}

          <div class="flex items-center gap-1.5 flex-wrap">
            <span class="text-[10px] text-slate-400 mr-1">{t("skills_link_on_install")}:</span>
            {#each installedAgents as row (row.kind)}
              {@const on = targetAgents.includes(row.kind)}
              <button
                type="button"
                class="{on ? chipOn : chipIdle} {row.installed ? '' : 'opacity-70'}"
                onclick={() => toggleTarget(row.kind)}
              >
                {AGENT_TITLE[row.kind]}
              </button>
            {/each}
          </div>

          {#if previewCandidate.extraFiles.length > 0}
            <div class="flex items-center gap-1 flex-wrap">
              {#each previewCandidate.extraFiles as file (file)}
                <span class="px-1.5 py-0.2 rounded text-[10px] font-mono bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300">
                  {file}
                </span>
              {/each}
            </div>
          {/if}
        </div>

        <pre class="flex-1 min-h-0 overflow-auto p-4 text-[11px] leading-relaxed font-mono text-slate-700 dark:text-slate-300 whitespace-pre-wrap select-text">{previewCandidate.skillMdPreview}</pre>
      {/if}
    </div>
  </div>
{/snippet}

{#if open}
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    role="presentation"
    onclick={onClose}
  >
    <div
      class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-5xl h-[82vh] max-h-[760px] flex flex-col overflow-hidden"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === "Escape") onClose(); }}
    >
      <!-- Modal Header + Tabs -->
      <div class="px-5 py-3 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between gap-3 shrink-0">
        <div class="flex items-center gap-3">
          <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("skills_acquire_title")}</span>
          <div class="flex rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800 p-0.5">
            {#each [
              { id: "discover", label: t("skills_tab_discover") },
              { id: "url", label: t("skills_tab_url") },
              { id: "local", label: t("skills_tab_local") },
            ] as item (item.id)}
              <button
                type="button"
                class="px-3 py-1 rounded-md text-[11px] font-medium transition cursor-pointer {tab === item.id
                  ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
                  : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
                onclick={() => switchTab(item.id as AcquireTab)}
              >
                {item.label}
              </button>
            {/each}
          </div>
        </div>
        <button type="button" onclick={onClose} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer">
          <X class="h-4 w-4" />
        </button>
      </div>

      {#if tab === "discover"}
        <!-- Discover Toolbar -->
        <div class="px-4 py-2.5 border-b border-slate-200/80 dark:border-slate-800 space-y-2 shrink-0 bg-slate-50/70 dark:bg-slate-900/60">
          <div class="flex items-center gap-2 flex-wrap">
            <div class="relative flex-1 min-w-[200px]">
              <Search class="absolute left-2.5 top-2 h-3.5 w-3.5 text-slate-400 pointer-events-none" />
              <input
                type="text"
                bind:value={searchQuery}
                placeholder={t("skills_search_remote")}
                class="{fieldClass} pl-8"
              />
            </div>
            <div class="flex items-center gap-1 flex-wrap">
              <button
                type="button"
                class={repoFilter === "all" ? chipOn : chipIdle}
                onclick={() => (repoFilter = "all")}
              >
                {t("skills_all_repos")}
              </button>
              {#each skillsStore.repos.filter((r) => r.enabled) as repo (repo.id)}
                <button
                  type="button"
                  class={repoFilter === repo.id ? chipOn : chipIdle}
                  onclick={() => (repoFilter = repo.id)}
                >
                  {repo.id}
                </button>
              {/each}
            </div>
            <button
              type="button"
              class={btnBorder}
              onclick={() => (showRepoManager = !showRepoManager)}
            >
              <Globe class="h-3.5 w-3.5" />
              {t("skills_manage_repos")}
            </button>
            <button
              type="button"
              class={btnBorder}
              disabled={skillsStore.discovering}
              onclick={() => { void skillsStore.discoverRemote(true); }}
            >
              <RefreshCw class="h-3.5 w-3.5 {skillsStore.discovering ? 'animate-spin' : ''}" />
              {t("skills_refresh_repos")}
            </button>
          </div>

          {#if showRepoManager}
            <div class="p-2.5 rounded-xl bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 space-y-2">
              <div class="flex items-center gap-1.5 flex-wrap">
                {#each skillsStore.repos as repo (repo.id)}
                  <div class="inline-flex items-center gap-1 px-2 py-1 rounded-lg border border-slate-200 dark:border-slate-700 text-[11px]">
                    <button
                      type="button"
                      class="inline-flex items-center gap-1 cursor-pointer {repo.enabled ? 'text-slate-800 dark:text-slate-100' : 'text-slate-400 line-through'}"
                      onclick={() => { void toggleRepo(repo.id); }}
                    >
                      <span class="h-3.5 w-3.5 rounded flex items-center justify-center border {repo.enabled ? 'bg-claude-600 border-claude-600 text-white' : 'border-slate-300 dark:border-slate-600'}">
                        {#if repo.enabled}<Check class="h-2.5 w-2.5" />{/if}
                      </span>
                      <span>{repo.id}</span>
                    </button>
                    {#if !repo.builtin}
                      <button
                        type="button"
                        class="text-slate-400 hover:text-rose-500 cursor-pointer"
                        onclick={() => { void removeRepo(repo.id); }}
                      >
                        <Trash2 class="h-3 w-3" />
                      </button>
                    {/if}
                  </div>
                {/each}
              </div>
              <div class="flex items-center gap-2">
                <input
                  class="{fieldClass} flex-1"
                  placeholder={t("skills_add_repo_placeholder")}
                  bind:value={newRepoInput}
                />
                <input
                  class="{fieldClass} w-36"
                  placeholder={t("cfg_optional")}
                  bind:value={newRepoLabel}
                />
                <button
                  type="button"
                  class={btnPrimary}
                  disabled={!newRepoInput.trim()}
                  onclick={() => { void addRepo(); }}
                >
                  <Plus class="h-3.5 w-3.5" />
                  {t("skills_add_repo_btn")}
                </button>
              </div>
            </div>
          {/if}

          {#if skillsStore.remoteWarnings.length > 0}
            <div class="text-[11px] text-amber-700 dark:text-amber-300 bg-amber-50 dark:bg-amber-950/40 border border-amber-200/80 dark:border-amber-800 rounded-lg px-2.5 py-1.5">
              {skillsStore.remoteWarnings.join(" · ")}
            </div>
          {/if}
        </div>

        {#if skillsStore.discovering && skillsStore.remoteCandidates.length === 0}
          <div class="flex-1 flex items-center justify-center gap-2 text-xs text-slate-400">
            <LoaderCircle class="h-4 w-4 animate-spin" />
          </div>
        {:else}
          {@render candidateListAndPreview(filteredDiscover, t("skills_import_empty"))}
        {/if}
      {:else if tab === "url"}
        <div class="px-4 py-3 border-b border-slate-200/80 dark:border-slate-800 space-y-1.5 shrink-0 bg-slate-50/70 dark:bg-slate-900/60">
          <div class="flex items-center gap-2">
            <input
              class="{fieldClass} flex-1 font-mono"
              placeholder={t("skills_url_placeholder")}
              bind:value={urlInput}
              onkeydown={(e) => { if (e.key === "Enter") void handleFetchUrl(); }}
            />
            <button
              type="button"
              class={btnPrimary}
              disabled={!urlInput.trim() || skillsStore.fetchingUrl}
              onclick={() => { void handleFetchUrl(); }}
            >
              {#if skillsStore.fetchingUrl}
                <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
              {:else}
                <Download class="h-3.5 w-3.5" />
              {/if}
              {t("skills_url_fetch")}
            </button>
          </div>
          <p class="text-[11px] text-slate-400 dark:text-slate-500">{t("skills_url_hint")}</p>
          {#if urlError}
            <div class="text-[11px] text-rose-600 dark:text-rose-400">{urlError}</div>
          {/if}
        </div>
        {@render candidateListAndPreview(urlCandidates, t("skills_select_preview"))}
      {:else}
        <div class="flex-1 min-h-0 overflow-y-auto p-4 space-y-2">
          {#if scanningLocal}
            <div class="py-10 flex items-center justify-center gap-2 text-xs text-slate-400">
              <LoaderCircle class="h-4 w-4 animate-spin" />
            </div>
          {:else if localSkills.length === 0}
            <p class="py-8 text-center text-xs text-slate-400">{t("skills_import_empty")}</p>
          {:else}
            {#each localSkills as item (item.id)}
              <button
                type="button"
                onclick={() => { void handleImportLocal(item); }}
                class="w-full text-left p-3 rounded-xl border border-slate-200 dark:border-slate-700 hover:border-claude-400 hover:bg-claude-50/40 dark:hover:bg-claude-950/30 transition cursor-pointer space-y-1.5"
              >
                <div class="flex items-center justify-between gap-2">
                  <span class="text-xs font-bold text-slate-800 dark:text-slate-100">{item.name}</span>
                  <div class="flex items-center gap-1.5 shrink-0">
                    <span class="text-[10px] font-mono text-slate-400">{item.fileCount} files</span>
                    {#if item.inLibrary}
                      <span class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-emerald-50 dark:bg-emerald-950/50 text-emerald-700 dark:text-emerald-300 border border-emerald-200/80 dark:border-emerald-800">
                        {t("mcp_in_library")}
                      </span>
                    {/if}
                  </div>
                </div>
                {#if item.description}
                  <div class="text-[11px] text-slate-500 dark:text-slate-400 line-clamp-2">{item.description}</div>
                {/if}
                <div class="text-[10px] font-mono text-slate-400 truncate">{item.sourcePath}</div>
                <div class="flex items-center gap-1 flex-wrap">
                  {#each item.sourceAgents as tag (tag)}
                    <span class="px-1.5 py-0.2 rounded text-[10px] bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300">
                      {sourceTagLabel(tag)}
                    </span>
                  {/each}
                </div>
              </button>
            {/each}
          {/if}
        </div>
        <div class="px-4 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50/60 dark:bg-slate-900/60 flex items-center gap-2">
          <FolderOpen class="h-4 w-4 text-slate-400 shrink-0" />
          <input
            class="{fieldClass} flex-1"
            placeholder={t("skills_import_path_placeholder")}
            bind:value={manualLocalPath}
          />
          <button
            type="button"
            class={btnPrimary}
            disabled={!manualLocalPath.trim() || skillsStore.saving}
            onclick={() => { void handleImportManual(); }}
          >
            {t("skills_import_path_btn")}
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}
