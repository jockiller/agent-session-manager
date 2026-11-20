<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    Copy,
    FileCode2,
    Import,
    LoaderCircle,
    Plus,
    RefreshCw,
    Search,
    Trash2,
    X,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { AGENT_TITLE, agentSupportsInstructions } from "$lib/config/document";
  import { configStore as store } from "$lib/config/store.svelte";
  import type { TargetScanRow } from "$lib/config/types";
  import MarkdownEditor from "./MarkdownEditor.svelte";
  import SaveBar from "../config/SaveBar.svelte";
  import { btnBorder, btnDanger, btnPrimary, fieldClass } from "../config/styles";

  let presetQuery = $state("");
  let showImportModal = $state(false);
  let showBindModal = $state(false);

  let presets = $derived(store.document.instructions.presets);
  let selectedPreset = $derived(store.selectedPreset);

  let filteredPresets = $derived.by(() => {
    const q = presetQuery.trim().toLowerCase();
    return presets.filter(
      (p) =>
        !q ||
        p.name.toLowerCase().includes(q) ||
        p.description.toLowerCase().includes(q) ||
        p.tags.some((tag) => tag.toLowerCase().includes(q))
    );
  });

  let importableRows = $derived(store.scannedTargets.filter((r) => r.fileExists));
  let scanMap = $derived(new Map(store.scannedTargets.map((r) => [r.targetId, r])));
  let bindingMap = $derived(new Map(store.document.instructions.bindings.map((b) => [b.targetId, b])));
  let presetNameMap = $derived(new Map(store.document.instructions.presets.map((p) => [p.id, p.name || p.id])));
  let boundTargetsForSelected = $derived.by(() => {
    if (!selectedPreset) return [];
    return instructionTargets.filter((target) => {
      const b = bindingMap.get(target.targetId);
      return Boolean(b?.enabled && b.presetId === selectedPreset.id);
    });
  });

  let instructionTargets = $derived.by(() => {
    const list: { targetId: string; label: string; installed: boolean; path: string }[] = [];
    const seen = new Set<string>();
    for (const row of store.rankedAgents()) {
      if (!agentSupportsInstructions(row.kind)) continue;
      const scanned = scanMap.get(row.kind);
      list.push({
        targetId: row.kind,
        label: AGENT_TITLE[row.kind],
        installed: row.installed,
        path: scanned?.path ?? "",
      });
      seen.add(row.kind);
    }
    for (const scanned of store.scannedTargets) {
      if (seen.has(scanned.targetId)) continue;
      const binding = bindingMap.get(scanned.targetId);
      if (scanned.installed || binding?.enabled) {
        list.push({
          targetId: scanned.targetId,
          label: scanned.name,
          installed: scanned.installed,
          path: scanned.path,
        });
      }
    }
    return list.sort((a, b) => Number(b.installed) - Number(a.installed));
  });

  function toggleAgentPreset(targetId: string, presetId: string) {
    const existing = bindingMap.get(targetId);
    if (existing?.enabled && existing.presetId === presetId) {
      store.setTargetBinding(targetId, (b) => {
        b.enabled = false;
        b.presetId = "";
      });
    } else {
      store.setTargetBinding(targetId, (b) => {
        b.enabled = true;
        b.presetId = presetId;
      });
    }
  }

  function duplicateSelectedPreset() {
    if (!selectedPreset) return;
    store.addPreset(`${selectedPreset.name} (Copy)`, selectedPreset.content, selectedPreset.description);
  }

  async function importFromTarget(row: TargetScanRow) {
    try {
      const text = await store.readTargetFile(row.targetId);
      store.addPreset(`${row.name} (${row.fileName})`, text, row.path);
      showImportModal = false;
    } catch (err) {
      store.show({ title: t("cfg_save_failed"), detail: String(err), isError: true, revealedPath: "" });
    }
  }

  onMount(() => {
    void store.ensureLoaded().then(() => store.scanInstructionTargets());
  });
</script>

<div class="flex-1 min-h-0 flex overflow-hidden bg-slate-50 dark:bg-slate-950">
  <!-- Column 1: Presets List -->
  <div class="w-[230px] shrink-0 border-r border-slate-200/80 dark:border-slate-800 bg-slate-50/80 dark:bg-slate-900/80 flex flex-col min-h-0">
    <div class="p-2.5 border-b border-slate-200/70 dark:border-slate-800 space-y-2">
      <div class="flex items-center justify-between">
        <span class="text-[11px] font-bold uppercase tracking-wider text-slate-500 dark:text-slate-400">
          {t("inst_presets")}
        </span>
        <div class="flex items-center gap-1">
          <button
            type="button"
            onclick={() => (showImportModal = true)}
            class="p-1 rounded-md text-slate-500 dark:text-slate-400 hover:bg-slate-200/60 dark:hover:bg-slate-800 hover:text-claude-600 cursor-pointer"
            title={t("inst_import_disk")}
          >
            <Import class="h-3.5 w-3.5" />
          </button>
          <button
            type="button"
            onclick={() => store.addPreset()}
            class="p-1 rounded-md bg-claude-600 hover:bg-claude-700 text-white cursor-pointer"
            title={t("inst_new_preset")}
          >
            <Plus class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>
      <div class="relative">
        <Search class="absolute left-2 top-2 h-3 w-3 text-slate-400 pointer-events-none" />
        <input
          type="text"
          bind:value={presetQuery}
          placeholder={t("inst_presets")}
          class="w-full bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-md pl-6.5 pr-2 py-1 text-[11px] text-slate-800 dark:text-slate-100 placeholder:text-slate-400 focus:outline-none focus:ring-1 focus:ring-claude-500"
        />
      </div>
    </div>

    <div class="flex-1 overflow-y-auto p-2 space-y-1">
      {#each filteredPresets as preset (preset.id)}
        {@const active = store.selectedPresetID === preset.id}
        {@const boundCount = store.document.instructions.bindings.filter((b) => b.enabled && b.presetId === preset.id).length}
        <button
          type="button"
          onclick={() => (store.selectedPresetID = preset.id)}
          class="w-full text-left px-2.5 py-2 rounded-lg transition cursor-pointer {active
            ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-semibold shadow-2xs border border-slate-200/90 dark:border-slate-700'
            : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800/60 border border-transparent'}"
        >
          <div class="flex items-center justify-between gap-1.5">
            <span class="text-xs truncate">{preset.name || preset.id}</span>
            {#if boundCount > 0}
              <span class="px-1.5 py-0.2 rounded-full text-[9px] font-mono bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800 shrink-0">
                {boundCount}
              </span>
            {/if}
          </div>
          {#if preset.description}
            <div class="text-[10px] text-slate-400 dark:text-slate-500 truncate mt-0.5">{preset.description}</div>
          {/if}
        </button>
      {/each}
    </div>
  </div>

  <!-- Column 2: Preset Metadata + CodeMirror Markdown Editor -->
  <div class="flex-1 min-w-0 flex flex-col min-h-0">
    {#if !selectedPreset}
      <div class="flex-1 flex flex-col items-center justify-center gap-3 text-center px-8">
        <FileCode2 class="h-10 w-10 text-slate-300 dark:text-slate-600" />
        <div>
          <div class="text-sm font-semibold text-slate-700 dark:text-slate-200">{t("inst_title")}</div>
          <p class="text-xs text-slate-400 dark:text-slate-500 mt-1">{t("inst_subtitle")}</p>
        </div>
        <div class="flex items-center gap-2">
          <button type="button" class={btnBorder} onclick={() => (showImportModal = true)}>
            <Import class="h-3.5 w-3.5" />
            {t("inst_import_disk")}
          </button>
          <button type="button" class={btnPrimary} onclick={() => store.addPreset()}>
            <Plus class="h-3.5 w-3.5" />
            {t("inst_new_preset")}
          </button>
        </div>
      </div>
    {:else}
      <div class="flex-1 min-h-0 flex flex-col p-4 gap-3">
        <!-- Preset Metadata Bar -->
        <div class="flex flex-wrap items-end gap-2.5">
          <label class="flex flex-col gap-1 w-48 shrink-0">
            <span class="text-[10px] font-medium text-slate-500 dark:text-slate-400">{t("inst_preset_name")}</span>
            <input
              class={fieldClass}
              value={selectedPreset.name}
              oninput={(e) => store.updatePreset(selectedPreset.id, (p) => { p.name = e.currentTarget.value; })}
            />
          </label>
          <label class="flex flex-col gap-1 flex-1 min-w-[160px]">
            <span class="text-[10px] font-medium text-slate-500 dark:text-slate-400">{t("inst_preset_desc")}</span>
            <input
              class={fieldClass}
              placeholder={t("cfg_optional")}
              value={selectedPreset.description}
              oninput={(e) => store.updatePreset(selectedPreset.id, (p) => { p.description = e.currentTarget.value; })}
            />
          </label>
          <div class="flex items-center gap-1.5 pb-0.5">
            <button type="button" class={btnBorder} onclick={duplicateSelectedPreset} title="Duplicate">
              <Copy class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              class={btnDanger}
              onclick={() => store.removePreset(selectedPreset.id)}
              title={t("inst_delete_preset")}
            >
              <Trash2 class="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              class={btnPrimary}
              disabled={store.isSyncingInstructions}
              onclick={() => { void store.saveAndSyncInstructions(); }}
            >
              {#if store.isSyncingInstructions}
                <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
              {:else}
                <RefreshCw class="h-3.5 w-3.5" />
              {/if}
              {t("inst_sync_agents")}
            </button>
          </div>
        </div>

        <!-- Bound Agents Compact Bar -->
        <div class="flex items-center gap-1.5 flex-wrap px-3 py-2 rounded-xl bg-white dark:bg-slate-900 border border-slate-200/80 dark:border-slate-800">
          <span class="text-[11px] font-medium text-slate-500 dark:text-slate-400 mr-1 shrink-0">{t("inst_bound_agents")}:</span>
          {#if boundTargetsForSelected.length === 0}
            <span class="text-[11px] text-slate-400 dark:text-slate-500">{t("cfg_none_bound")}</span>
          {:else}
            {#each boundTargetsForSelected as target (target.targetId)}
              <button
                type="button"
                title={target.path ? `${target.path} · ${t("cfg_unbind_tip")}` : t("cfg_unbind_tip")}
                class="inline-flex items-center gap-1 px-2.5 py-0.5 rounded-full text-[11px] font-medium bg-claude-600 text-white hover:bg-claude-700 transition cursor-pointer {target.installed ? '' : 'opacity-80'}"
                onclick={() => toggleAgentPreset(target.targetId, selectedPreset.id)}
              >
                <span>{target.label}</span>
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
          value={selectedPreset.content}
          onchange={(text) => store.updatePreset(selectedPreset.id, (p) => { p.content = text; })}
        />
      </div>
    {/if}

    <SaveBar
      isDirty={store.isDirty}
      hint={t("inst_sync_bar_hint")}
      saveTitle={t("cfg_save_and_sync")}
      onSave={() => { void store.saveAndSyncInstructions(); }}
      onDiscard={() => store.revert()}
    />
  </div>
</div>

{#if showImportModal}
  <div class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4">
    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-lg max-h-[75vh] flex flex-col overflow-hidden">
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between">
        <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("inst_import_title")}</span>
        <button type="button" onclick={() => (showImportModal = false)} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer">
          <X class="h-4 w-4" />
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-3 space-y-2">
        {#if importableRows.length === 0}
          <p class="py-8 text-center text-xs text-slate-400">{t("inst_import_empty")}</p>
        {:else}
          {#each importableRows as row (row.targetId)}
            <button
              type="button"
              onclick={() => { void importFromTarget(row); }}
              class="w-full text-left p-3 rounded-xl border border-slate-200 dark:border-slate-700 hover:border-claude-400 hover:bg-claude-50/40 dark:hover:bg-claude-950/30 transition cursor-pointer space-y-1"
            >
              <div class="flex items-center justify-between gap-2">
                <span class="text-xs font-bold text-slate-800 dark:text-slate-100">{row.name}</span>
                <span class="text-[10px] font-mono text-claude-600 dark:text-claude-400">{row.fileName}</span>
              </div>
              <div class="text-[10px] font-mono text-slate-400 truncate">{row.path}</div>
              {#if row.diskPreview}
                <div class="text-[11px] text-slate-500 dark:text-slate-400 line-clamp-2 font-mono">{row.diskPreview}</div>
              {/if}
            </button>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if showBindModal && selectedPreset}
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
          <span class="px-2 py-0.5 rounded-md text-[10px] font-medium bg-claude-50 dark:bg-claude-950/60 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800 truncate">
            {selectedPreset.name || selectedPreset.id}
          </span>
        </div>
        <button type="button" onclick={() => (showBindModal = false)} class="p-1 text-slate-400 hover:text-slate-600 cursor-pointer shrink-0">
          <X class="h-4 w-4" />
        </button>
      </div>
      <div class="flex-1 overflow-y-auto p-3 space-y-1.5">
        {#each instructionTargets as target (target.targetId)}
          {@const binding = bindingMap.get(target.targetId)}
          {@const checked = Boolean(binding?.enabled && binding.presetId === selectedPreset.id)}
          {@const otherPresetId = binding?.enabled && binding.presetId && binding.presetId !== selectedPreset.id ? binding.presetId : ""}
          {@const otherPresetName = otherPresetId ? (presetNameMap.get(otherPresetId) ?? otherPresetId) : ""}
          <button
            type="button"
            onclick={() => toggleAgentPreset(target.targetId, selectedPreset.id)}
            class="w-full text-left px-3 py-2.5 rounded-xl border transition cursor-pointer flex items-center gap-3 {checked
              ? 'border-claude-500 bg-claude-50/60 dark:bg-claude-950/30'
              : 'border-slate-200 dark:border-slate-700/80 hover:border-slate-300 dark:hover:border-slate-600'} {target.installed ? '' : 'opacity-70'}"
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
                <span class="text-xs font-semibold text-slate-800 dark:text-slate-100">{target.label}</span>
                {#if !target.installed}
                  <span class="px-1.5 py-0.2 rounded text-[9px] bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400">
                    {t("cfg_not_installed")}
                  </span>
                {/if}
              </span>
              {#if target.path}
                <span class="block text-[10px] font-mono text-slate-400 dark:text-slate-500 truncate">{target.path}</span>
              {/if}
            </span>
            {#if otherPresetName}
              <span class="px-1.5 py-0.5 rounded text-[10px] bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400 shrink-0">
                {t("cfg_other_preset_badge", { name: otherPresetName })}
              </span>
            {/if}
          </button>
        {/each}
      </div>
      <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 flex items-center justify-between">
        <span class="text-[11px] text-slate-500 dark:text-slate-400">{t("cfg_bound_count", { n: boundTargetsForSelected.length })}</span>
        <button type="button" class={btnPrimary} onclick={() => (showBindModal = false)}>
          {t("cfg_bind_done")}
        </button>
      </div>
    </div>
  </div>
{/if}
