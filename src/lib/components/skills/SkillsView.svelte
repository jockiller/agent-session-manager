<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    Download,
    FileText,
    FolderOpen,
    Import,
    LoaderCircle,
    Plus,
    RefreshCw,
    Search,
    Sparkles,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { AGENT_TITLE, resolvedSkillsDirHint } from "$lib/config/document";
  import { configStore } from "$lib/config/store.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";
  import MarkdownEditor from "../instructions/MarkdownEditor.svelte";
  import SaveBar from "../config/SaveBar.svelte";
  import SkillsAcquireModal from "./SkillsAcquireModal.svelte";
  import { btnBorder, btnDanger, btnPrimary, fieldClass } from "../config/styles";

  let query = $state("");
  let showCreateModal = $state(false);
  let showAcquireModal = $state(false);
  let showBindModal = $state(false);
  let showExtraFiles = $state(false);

  let newId = $state("");
  let newName = $state("");
  let newDesc = $state("");

  let selectedSkill = $derived(skillsStore.selectedSkill);

  let filteredSkills = $derived.by(() => {
    const q = query.trim().toLowerCase();
    return skillsStore.skills.filter(
      (s) =>
        !q ||
        s.id.toLowerCase().includes(q) ||
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q),
    );
  });

  let agentRows = $derived(configStore.rankedAgents());
  let boundAgentRows = $derived(
    agentRows.filter((row) => skillsStore.draftLinkedAgents.includes(row.kind)),
  );

  async function submitCreate() {
    if (!newId.trim()) return;
    const ok = await skillsStore.createSkill(newId, newName, newDesc);
    if (ok) {
      showCreateModal = false;
      newId = "";
      newName = "";
      newDesc = "";
    }
  }

  onMount(() => {
    void configStore.ensureLoaded();
    void skillsStore.ensureLoaded();
  });
</script>

<div class="flex-1 min-h-0 flex overflow-hidden bg-slate-50 dark:bg-slate-950">
  <!-- Column 1: Skills List -->
  <div class="w-[230px] shrink-0 border-r border-slate-200/80 dark:border-slate-800 bg-slate-50/80 dark:bg-slate-900/80 flex flex-col min-h-0">
    <div class="p-2.5 border-b border-slate-200/70 dark:border-slate-800 space-y-2">
      <div class="flex items-center justify-between">
        <span class="text-[11px] font-bold uppercase tracking-wider text-slate-500 dark:text-slate-400">
          {t("nav_skills")}
        </span>
        <div class="flex items-center gap-1">
          <button
            type="button"
            onclick={() => (showAcquireModal = true)}
            class="p-1 rounded-md text-slate-500 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-slate-800 hover:text-claude-600 cursor-pointer"
            title={t("skills_acquire_title")}
          >
            <Import class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onclick={() => (showCreateModal = true)}
            class="p-1 rounded-md bg-claude-600 hover:bg-claude-700 text-white cursor-pointer"
            title={t("skills_new")}
          >
            <Plus class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
      <div class="relative">
        <Search class="absolute left-2 top-2 h-3 w-3 text-slate-400 pointer-events-none" />
        <input
          type="text"
          bind:value={query}
          placeholder={t("nav_skills")}
          class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md pl-6.5 pr-2 py-1 text-[11px] text-slate-800 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-claude-500"
        />
      </div>
    </div>

    <div class="flex-1 overflow-y-auto p-2 space-y-1">
      {#each filteredSkills as skill (skill.id)}
        {@const active = skillsStore.selectedID === skill.id}
        <button
          type="button"
          onclick={() => { showExtraFiles = false; skillsStore.select(skill.id); }}
          class="w-full text-left px-2.5 py-2 rounded-lg transition cursor-pointer {active
            ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-semibold shadow-2xs border border-slate-200/90 dark:border-slate-700'
            : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800/60 border border-transparent'}"
        >
          <div class="flex items-center justify-between gap-1.5">
            <span class="text-xs truncate">{skill.name || skill.id}</span>
            {#if skill.linkedAgents.length > 0}
              <span class="px-1.5 py-0.2 rounded-full text-[9px] font-mono bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800 shrink-0">
                {skill.linkedAgents.length}
              </span>
            {/if}
          </div>
          <div class="text-[10px] font-mono text-slate-400 dark:text-slate-500 truncate mt-0.5">{skill.id}</div>
        </button>
      {/each}
    </div>
  </div>

  <!-- Column 2: Selected Skill Editor + Bound Agents -->
  <div class="flex-1 min-w-0 flex flex-col min-h-0">
    {#if !selectedSkill}
      <div class="flex-1 flex flex-col items-center justify-center gap-3 text-center px-8">
        <Sparkles class="h-10 w-10 text-slate-300 dark:text-slate-600" />
        <div>
          <div class="text-sm font-semibold text-slate-700 dark:text-slate-200">{t("skills_title")}</div>
          <p class="text-xs text-slate-400 dark:text-slate-500 mt-1">{t("skills_subtitle")}</p>
        </div>
        <div class="flex items-center gap-2">
          <button type="button" class={btnBorder} onclick={() => (showAcquireModal = true)}>
            <Import class="h-3.5 w-3.5" />
            {t("skills_acquire_title")}
          </button>
          <button type="button" class={btnPrimary} onclick={() => (showCreateModal = true)}>
            <Plus class="h-3.5 w-3.5" />
            {t("skills_new")}
          </button>
        </div>
      </div>
    {:else}
      <div class="flex-1 min-h-0 flex flex-col p-4 gap-3">
        <!-- Skill Header Bar -->
        <div class="flex flex-wrap items-center justify-between gap-2.5">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2 min-w-0">
              <h2 class="text-sm font-bold text-slate-900 dark:text-slate-100 truncate">
                {selectedSkill.name || selectedSkill.id}
              </h2>
              <span class="px-1.5 py-0.5 rounded text-[10px] font-mono bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400 shrink-0">
                {selectedSkill.id}
              </span>
              {#if selectedSkill.extraFiles.length > 0}
                <button
                  type="button"
                  onclick={() => (showExtraFiles = !showExtraFiles)}
                  class="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[10px] font-medium bg-slate-100 dark:bg-slate-800 text-slate-600 dark:text-slate-300 hover:bg-slate-200 dark:hover:bg-slate-700 cursor-pointer shrink-0"
                >
                  <FileText class="h-3 w-3" />
                  <span>{t("skills_extra_files", { n: selectedSkill.extraFiles.length })}</span>
                </button>
              {/if}
              {#if selectedSkill.source}
                <span
                  title={selectedSkill.source.url}
                  class="px-1.5 py-0.5 rounded text-[10px] font-mono bg-claude-50 dark:bg-claude-950/60 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800 truncate max-w-[200px]"
                >
                  {selectedSkill.source.repo}{selectedSkill.source.commit ? ` @${selectedSkill.source.commit}` : ""}
                </span>
              {/if}
            </div>
            {#if selectedSkill.description}
              <p class="text-[11px] text-slate-500 dark:text-slate-400 truncate mt-0.5">
                {selectedSkill.description}
              </p>
            {/if}
          </div>
          <div class="flex items-center gap-1.5 shrink-0">
            {#if selectedSkill.source}
              <button
                type="button"
                class={btnBorder}
                disabled={skillsStore.updatingRemote}
                onclick={() => { void skillsStore.updateSelectedRemote(); }}
                title={selectedSkill.source.url}
              >
                {#if skillsStore.updatingRemote}
                  <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
                {:else}
                  <Download class="h-3.5 w-3.5" />
                {/if}
                {t("skills_check_update")}
              </button>
            {/if}
            <button
              type="button"
              class={btnBorder}
              onclick={() => { void configStore.revealPath(selectedSkill.path); }}
              title={t("skills_open_folder")}
            >
              <FolderOpen class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              class={btnDanger}
              onclick={() => { void skillsStore.deleteSelected(); }}
              title={t("cfg_delete")}
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              class={btnPrimary}
              disabled={skillsStore.saving}
              onclick={() => { void skillsStore.saveAndSyncSelected(); }}
            >
              {#if skillsStore.saving}
                <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
              {:else}
                <RefreshCw class="h-3.5 w-3.5" />
              {/if}
              {t("skills_sync_links")}
            </button>
          </div>
        </div>

        {#if showExtraFiles && selectedSkill.extraFiles.length > 0}
          <div class="px-3 py-2 rounded-xl bg-white dark:bg-slate-900 border border-slate-200/80 dark:border-slate-800 flex flex-wrap gap-1.5">
            {#each selectedSkill.extraFiles as file (file.relPath)}
              <span class="px-2 py-0.5 rounded text-[10px] font-mono bg-slate-50 dark:bg-slate-800 text-slate-600 dark:text-slate-300 border border-slate-200/70 dark:border-slate-700">
                {file.relPath}
              </span>
            {/each}
          </div>
        {/if}

        <!-- Bound Agents Compact Bar (Symlinks) -->
        <div class="flex items-center gap-1.5 flex-wrap px-3 py-2 rounded-xl bg-white dark:bg-slate-900 border border-slate-200/80 dark:border-slate-800">
          <span class="text-[11px] font-medium text-slate-500 dark:text-slate-400 mr-1 shrink-0">
            {t("inst_bound_agents")} ({t("skills_symlink_badge")}):
          </span>
          {#if boundAgentRows.length === 0}
            <span class="text-[11px] text-slate-400 dark:text-slate-500">{t("cfg_none_bound")}</span>
          {:else}
            {#each boundAgentRows as row (row.kind)}
              {@const dirHint = `${resolvedSkillsDirHint(row.kind)}/${selectedSkill.id}`}
              <button
                type="button"
                title="{dirHint} · {t('cfg_unbind_tip')}"
                class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-medium bg-claude-600 text-white hover:bg-claude-700 transition cursor-pointer {row.installed ? '' : 'opacity-80'}"
                onclick={() => skillsStore.toggleDraftAgent(row.kind)}
              >
                <span>{AGENT_TITLE[row.kind]}</span>
                <X class="h-3 w-3 opacity-80" />
              </button>
            {/each}
          {/if}
          <button
            type="button"
            class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-medium border border-dashed border-slate-300 dark:border-slate-600 text-slate-600 dark:text-slate-300 hover:border-claude-500 hover:text-claude-600 dark:hover:text-claude-400 transition cursor-pointer"
            onclick={() => (showBindModal = true)}
          >
            <Plus class="h-3 w-3" />
            <span>{t("cfg_bind_more")}</span>
          </button>
        </div>

        <MarkdownEditor
          value={skillsStore.draftContent}
          onchange={(text) => { skillsStore.draftContent = text; }}
        />
      </div>
    {/if}

    <SaveBar
      isDirty={skillsStore.isDirty}
      hint={t("skills_sync_bar_hint")}
      saveTitle={t("cfg_save_and_sync")}
      onSave={() => { void skillsStore.saveAndSyncSelected(); }}
      onDiscard={() => skillsStore.revert()}
    />
  </div>
</div>

{#if showCreateModal}
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    role="presentation"
    onclick={() => (showCreateModal = false)}
  >
    <div
      class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-md flex flex-col overflow-hidden"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === "Escape") showCreateModal = false; }}
    >
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
        <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("skills_create_title")}</span>
        <button type="button" onclick={() => (showCreateModal = false)} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer">
          <X class="h-4 w-4" />
        </button>
      </div>
      <div class="p-5 space-y-3">
        <label class="block space-y-1">
          <span class="text-[11px] font-medium text-slate-600 dark:text-slate-300">{t("skills_id_label")}</span>
          <input class={fieldClass} placeholder="code-review" bind:value={newId} />
        </label>
        <label class="block space-y-1">
          <span class="text-[11px] font-medium text-slate-600 dark:text-slate-300">{t("skills_name_label")}</span>
          <input class={fieldClass} placeholder="Code Review" bind:value={newName} />
        </label>
        <label class="block space-y-1">
          <span class="text-[11px] font-medium text-slate-600 dark:text-slate-300">{t("skills_desc_label")}</span>
          <textarea
            class="{fieldClass} min-h-[72px] resize-y"
            placeholder={t("cfg_optional")}
            bind:value={newDesc}
          ></textarea>
        </label>
      </div>
      <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 flex items-center justify-end gap-2">
        <button type="button" class={btnBorder} onclick={() => (showCreateModal = false)}>
          {t("cfg_cancel")}
        </button>
        <button
          type="button"
          class={btnPrimary}
          disabled={!newId.trim() || skillsStore.saving}
          onclick={() => { void submitCreate(); }}
        >
          {t("cfg_add")}
        </button>
      </div>
    </div>
  </div>
{/if}

{#if showBindModal && selectedSkill}
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    role="presentation"
    onclick={() => (showBindModal = false)}
  >
    <div
      class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-lg max-h-[75vh] flex flex-col overflow-hidden"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => { if (e.key === "Escape") showBindModal = false; }}
    >
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between gap-2">
        <div class="flex items-center gap-2 min-w-0">
          <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("cfg_bind_modal_title")}</span>
          <span class="px-2 py-0.5 rounded-md text-[10px] font-mono bg-claude-50 dark:bg-claude-950/60 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800 truncate">
            {selectedSkill.id}
          </span>
        </div>
        <button type="button" onclick={() => (showBindModal = false)} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer shrink-0">
          <X class="h-4 w-4" />
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-3 space-y-1.5">
        {#each agentRows as row (row.kind)}
          {@const checked = skillsStore.draftLinkedAgents.includes(row.kind)}
          {@const linkPath = `${resolvedSkillsDirHint(row.kind)}/${selectedSkill.id}`}
          <button
            type="button"
            onclick={() => skillsStore.toggleDraftAgent(row.kind)}
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
              <span class="block text-[10px] font-mono text-slate-400 dark:text-slate-500 truncate">{linkPath}</span>
            </span>
          </button>
        {/each}
      </div>
      <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 flex items-center justify-between">
        <span class="text-[11px] text-slate-500 dark:text-slate-400">{t("cfg_bound_count", { n: boundAgentRows.length })}</span>
        <button type="button" class={btnPrimary} onclick={() => (showBindModal = false)}>
          {t("cfg_bind_done")}
        </button>
      </div>
    </div>
  </div>
{/if}

<SkillsAcquireModal open={showAcquireModal} onClose={() => (showAcquireModal = false)} />
