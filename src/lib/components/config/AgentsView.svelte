<script lang="ts">
  import { Brain, Box, Code, Download, LoaderCircle, Sparkles, Terminal } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { configStore as store } from "$lib/config/store.svelte";
  import {
    AGENT_TINT,
    AGENT_TITLE,
    API_TITLE,
    agentWritesPerModelEffort,
    findModel,
    isPicked,
    modelRef,
    pickedChoices,
    providerExportMode,
    resolvedHint,
    setPicked,
    summaryPills,
  } from "$lib/config/document";
  import { EFFORT_LEVELS, type AgentKind, type ModelEntry, type Provider } from "$lib/config/types";
  import ConfigCard from "./ConfigCard.svelte";
  import FieldLabel from "./FieldLabel.svelte";
  import MetaPill from "./MetaPill.svelte";
  import SectionLabel from "./SectionLabel.svelte";
  import { btnGhost, btnPrimary, chipIdle, chipOn, fieldClass, selectClass } from "./styles";

  const AGENT_ICONS = {
    opencode: Code,
    dsh: Terminal,
    zcode: Box,
    grok: Sparkles,
    pi: Brain,
    omp: Brain,
  };

  let catalog = $derived(store.catalogRefs());
  let settings = $derived(store.selectedSettings);
  let choices = $derived(pickedChoices(store.document, store.selectedAgent));
  let modelValue = $derived(choices.some((choice) => choice.ref === settings.model) ? settings.model : "");
  let selectedModel = $derived.by(() => {
    const ref = modelValue || choices[0]?.ref || "";
    return findModel(store.document, ref);
  });
  let perModelEffort = $derived(agentWritesPerModelEffort(store.selectedAgent));
  let effortChoices = $derived(
    perModelEffort ? [...EFFORT_LEVELS] : (selectedModel?.efforts ?? [...EFFORT_LEVELS]),
  );
  let effortValue = $derived(effortChoices.includes(settings.effort) ? settings.effort : "");
  let applied = $derived(store.appliedLabel(store.selectedAgent));
  let noThinking = $derived(!perModelEffort && selectedModel?.efforts?.length === 0);
  let modelsEmpty = $derived(store.document.providers.every((provider) => provider.models.length === 0));

  function setModel(value: string) {
    store.updateSelectedSettings((next) => {
      next.model = value;
      if (perModelEffort) return;
      const model = findModel(store.document, value);
      if (model?.efforts) {
        if (model.efforts.length === 0) {
          next.effort = "";
        } else if (!model.efforts.includes(next.effort)) {
          next.effort = model.efforts.includes("high") ? "high" : (model.efforts.at(-1) ?? "");
        }
      }
    });
  }

  function setProviderPicked(provider: Provider, on: boolean) {
    store.updateSelectedSettings((next) => {
      for (const model of provider.models) {
        setPicked(next, modelRef(provider.id, model.id), on, catalog);
      }
    });
  }

  function togglePicked(ref: string, on: boolean) {
    store.updateSelectedSettings((next) => {
      setPicked(next, ref, on, catalog);
    });
  }

  function modelEffortChoices(model: { efforts?: string[] }): string[] {
    return model.efforts ?? [...EFFORT_LEVELS];
  }

  function rowPills(model: ModelEntry): string[] {
    const pills = summaryPills(model);
    if (!perModelEffort || model.efforts == null) return pills;
    const effortLabel =
      model.efforts.length === 0 ? t("cfg_profile_no_thinking") : model.efforts.join(", ");
    return pills.filter((pill) => pill !== effortLabel);
  }

  function modelEffortValue(ref: string, model: { efforts?: string[] }): string {
    const override = settings.effortByModel[ref];
    return override && modelEffortChoices(model).includes(override) ? override : "";
  }

  function setModelEffort(ref: string, value: string) {
    store.updateSelectedSettings((next) => {
      if (value) next.effortByModel[ref] = value;
      else delete next.effortByModel[ref];
    });
  }

  function toggleMcp(id: string, on: boolean) {
    store.updateSelectedSettings((next) => {
      if (on) {
        if (!next.mcpIDs.includes(id)) next.mcpIDs.push(id);
      } else {
        next.mcpIDs = next.mcpIDs.filter((item) => item !== id);
      }
    });
  }
</script>

<div class="flex h-full min-h-0">
  <div class="w-[220px] shrink-0 border-r border-slate-200/80 bg-slate-50/80 overflow-y-auto p-2 space-y-0.5">
    {#each store.rankedAgents() as row (row.kind)}
      {@const Icon = AGENT_ICONS[row.kind]}
      {@const selected = store.selectedAgent === row.kind}
      <button
        type="button"
        class="w-full flex items-center gap-2.5 px-2 py-1.5 rounded-lg text-left transition cursor-pointer {selected
          ? 'bg-white text-slate-900 font-semibold shadow-2xs border border-slate-200/80'
          : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900 border border-transparent'}"
        style="opacity: {row.installed ? 1 : 0.55}"
        onclick={() => (store.selectedAgent = row.kind as AgentKind)}
      >
        <span
          class="h-[26px] w-[26px] rounded-[7px] flex items-center justify-center shrink-0"
          style="background: {AGENT_TINT[row.kind]}; opacity: {row.installed ? 1 : 0.45}"
        >
          <Icon class="h-3.5 w-3.5 text-white" />
        </span>
        <span class="min-w-0 flex-1">
          <span class="flex items-center gap-1.5">
            <span class="text-xs truncate">{AGENT_TITLE[row.kind]}</span>
            {#if !row.installed}
              <MetaPill text={t("cfg_not_installed")} />
            {/if}
          </span>
          <span class="block text-[10px] text-slate-400 font-normal truncate">{store.appliedLabel(row.kind)}</span>
        </span>
      </button>
    {/each}
  </div>

  <div class="flex-1 min-w-0 relative">
    <div class="absolute inset-0 overflow-y-auto p-5 pb-20 space-y-4">
      <ConfigCard>
        <div class="flex items-start gap-3">
          <div class="min-w-0 flex-1">
            <div class="text-sm font-semibold text-slate-800">{AGENT_TITLE[store.selectedAgent]}</div>
            <div class="text-[11px] text-slate-400 select-text break-all">
              {resolvedHint(store.selectedAgent, settings.profile)}
            </div>
          </div>
          <MetaPill text={applied} emphasized={applied !== t("cfg_not_applied")} />
        </div>

        {#if store.selectedAgent === "dsh"}
          <div class="flex items-end gap-3">
            <FieldLabel title={t("cfg_profile")} class="flex-1">
              <input
                class={fieldClass}
                placeholder="web"
                value={settings.profile}
                oninput={(e) => store.updateSelectedSettings((next) => { next.profile = e.currentTarget.value; })}
              />
            </FieldLabel>
            {#if store.dshProfiles.length > 0}
              <FieldLabel title={t("cfg_existing")} class="w-40 shrink-0">
                <select
                  class={selectClass}
                  title={t("cfg_existing_profile")}
                  value={settings.profile}
                  onchange={(e) => store.updateSelectedSettings((next) => { next.profile = e.currentTarget.value; })}
                >
                  <option value="">{t("cfg_profile_custom")}</option>
                  {#each store.dshProfiles as name}
                    <option value={name}>{name}</option>
                  {/each}
                </select>
              </FieldLabel>
            {/if}
          </div>
        {/if}

        <div class="flex items-end gap-3">
          <FieldLabel title={t("cfg_default_model")} class="flex-1">
            <select class={selectClass} value={modelValue} onchange={(e) => setModel(e.currentTarget.value)}>
              <option value="">{t("cfg_first_picked")}</option>
              {#each choices as choice}
                <option value={choice.ref}>{choice.title}</option>
              {/each}
            </select>
          </FieldLabel>
          <FieldLabel title={t(perModelEffort ? "cfg_effort_default" : "cfg_effort")} class="w-[140px] shrink-0">
            {#if noThinking}
              <div class="h-[30px] flex items-center text-xs text-slate-400">{t("cfg_no_thinking")}</div>
            {:else}
              <select
                class={selectClass}
                value={effortValue}
                onchange={(e) => store.updateSelectedSettings((next) => { next.effort = e.currentTarget.value; })}
              >
                <option value="">{t("cfg_effort_empty")}</option>
                {#each effortChoices as level}
                  <option value={level}>{level}</option>
                {/each}
              </select>
            {/if}
          </FieldLabel>
          <FieldLabel title={t("cfg_small_model")} class="max-w-[220px] flex-1">
            <input
              class={fieldClass}
              placeholder={t("cfg_optional")}
              value={settings.smallModel}
              oninput={(e) => store.updateSelectedSettings((next) => { next.smallModel = e.currentTarget.value; })}
            />
          </FieldLabel>
        </div>
      </ConfigCard>

      <ConfigCard>
        <SectionLabel title={t("cfg_models")} accessory={t("cfg_unchecked_means_all")} />
        {#if modelsEmpty}
          <p class="text-xs text-slate-400 py-3">{t("cfg_models_empty")}</p>
        {:else}
          <div class="space-y-4">
            {#each store.document.providers as provider (provider.id)}
              {#if provider.models.length > 0}
                {@const mode = providerExportMode(store.selectedAgent, provider)}
                {@const enabled = mode !== "skip"}
                <div class="space-y-2 {enabled ? '' : 'opacity-60'}">
                  <div class="flex items-center gap-2">
                    <span class="text-[11px] font-semibold {enabled ? 'text-slate-500' : 'text-slate-400'}">{provider.name || provider.id}</span>
                    <span class="text-[10px] text-slate-400">{provider.models.length}</span>
                    {#if mode === "compat"}
                      <MetaPill text={t("cfg_compat_pill")} title={provider.compatBaseURL} />
                    {/if}
                    <div class="flex-1"></div>
                    <button type="button" class={btnGhost} disabled={!enabled} onclick={() => setProviderPicked(provider, true)}>
                      {t("cfg_select_all")}
                    </button>
                    <button type="button" class={btnGhost} disabled={!enabled} onclick={() => setProviderPicked(provider, false)}>
                      {t("cfg_clear")}
                    </button>
                  </div>
                  {#if mode === "skip"}
                    <p class="text-[11px] text-slate-400 leading-relaxed">
                      {t("cfg_provider_unsupported", {
                        agent: AGENT_TITLE[store.selectedAgent],
                        protocol: API_TITLE[provider.api],
                      })}
                    </p>
                  {/if}
                  <div class="space-y-1">
                    {#each provider.models as model (model.id)}
                      {@const ref = modelRef(provider.id, model.id)}
                      {@const thinkingOff = model.efforts?.length === 0}
                      {@const rowEffortChoices = modelEffortChoices(model)}
                      <div
                        class="flex items-center gap-2 px-2 py-1.5 rounded-lg bg-slate-50/80 min-w-0 {enabled
                          ? ''
                          : 'cursor-not-allowed'}"
                      >
                        <label
                          class="flex items-center gap-2 min-w-0 flex-1 overflow-hidden {enabled
                            ? 'cursor-pointer'
                            : 'cursor-not-allowed'}"
                        >
                          <input
                            type="checkbox"
                            class="rounded border-slate-300 text-sky-600 focus:ring-sky-500 shrink-0"
                            checked={isPicked(settings, ref, catalog)}
                            disabled={!enabled}
                            onchange={(e) => togglePicked(ref, e.currentTarget.checked)}
                          />
                          <span
                            class="text-xs {enabled ? 'text-slate-800' : 'text-slate-500'} min-w-0 flex-1 truncate"
                            title={model.name}
                          >{model.name}</span>
                          {#each rowPills(model) as pill}
                            <MetaPill text={pill} />
                          {/each}
                        </label>
                        {#if perModelEffort && enabled && model.efforts != null}
                          {#if thinkingOff}
                            <span class="text-[10px] text-slate-400 w-20 shrink-0 truncate">{t("cfg_no_thinking")}</span>
                          {:else}
                            <select
                              class="h-6 w-20 shrink-0 bg-white border border-slate-200 rounded-md px-1 text-[11px] text-slate-700 focus:outline-none focus:ring-1 focus:ring-sky-500 focus:border-sky-500"
                              title={t("cfg_effort")}
                              value={modelEffortValue(ref, model)}
                              onchange={(e) => setModelEffort(ref, e.currentTarget.value)}
                            >
                              <option value="">{t("cfg_effort_inherit")}</option>
                              {#each rowEffortChoices as level}
                                <option value={level}>{level}</option>
                              {/each}
                            </select>
                          {/if}
                        {/if}
                      </div>
                    {/each}
                  </div>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      </ConfigCard>

      <ConfigCard>
        <SectionLabel title={t("cfg_tab_library")} />
        <div class="flex items-center gap-2">
          <button
            type="button"
            role="switch"
            aria-checked={settings.instructionOn}
            aria-label={t("cfg_write_agents_md")}
            class="relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors cursor-pointer {settings.instructionOn
              ? 'bg-sky-600'
              : 'bg-slate-300'}"
            onclick={() => store.updateSelectedSettings((next) => { next.instructionOn = !next.instructionOn; })}
          >
            <span
              class="inline-block h-4 w-4 rounded-full bg-white shadow-2xs transition-transform {settings.instructionOn
                ? 'translate-x-[18px]'
                : 'translate-x-0.5'}"
            ></span>
          </button>
          <span class="text-xs text-slate-700">{t("cfg_write_agents_md")}</span>
        </div>
        {#if store.document.library.mcp.length === 0}
          <p class="text-xs text-slate-400">{t("cfg_no_mcp_in_library")}</p>
        {:else}
          <div class="flex flex-wrap gap-1.5">
            {#each store.document.library.mcp as server (server.id)}
              <button
                type="button"
                class={settings.mcpIDs.includes(server.id) ? chipOn : chipIdle}
                onclick={() => toggleMcp(server.id, !settings.mcpIDs.includes(server.id))}
              >
                {server.id}
              </button>
            {/each}
          </div>
        {/if}
      </ConfigCard>
    </div>

    <div class="absolute bottom-5 right-5">
      <button
        type="button"
        class="{btnPrimary} !px-3.5 !py-2 shadow-md"
        disabled={store.isExporting}
        title={t("cfg_save_and_export_help", { title: AGENT_TITLE[store.selectedAgent] })}
        onclick={() => store.apply(store.selectedAgent)}
      >
        {#if store.isExporting}
          <LoaderCircle class="h-3.5 w-3.5 animate-spin" />
          {t("cfg_exporting")}
        {:else}
          <Download class="h-3.5 w-3.5" />
          {t("cfg_save_and_export")}
        {/if}
      </button>
    </div>
  </div>
</div>
