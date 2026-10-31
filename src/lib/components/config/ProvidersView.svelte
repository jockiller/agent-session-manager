<script lang="ts">
  import { ChevronDown, ChevronRight, LoaderCircle, RefreshCw, Trash2, Unplug } from "@lucide/svelte";
  import { i18n, t } from "$lib/i18n";
  import { configStore as store } from "$lib/config/store.svelte";
  import { AGENT_TITLE, API_SHORT, API_TITLE, agentsWithoutAPI, summaryPills } from "$lib/config/document";
  import { API_KINDS, EFFORT_LEVELS, INPUT_KINDS, type APIKind, type ModelCost, type ModelEntry } from "$lib/config/types";
  import ConfigCard from "./ConfigCard.svelte";
  import FieldLabel from "./FieldLabel.svelte";
  import MetaPill from "./MetaPill.svelte";
  import SaveBar from "./SaveBar.svelte";
  import SectionLabel from "./SectionLabel.svelte";
  import { btnBorder, btnDanger, btnGhost, chipIdle, chipOn, fieldClass, selectClass } from "./styles";

  let newModelID = $state("");
  let newModelName = $state("");
  let expandedModelID = $state<string | null>(null);
  let pulling = $state(false);
  let checking = $state(false);
  let costDrafts = $state<Record<string, string>>({});

  let selectedID = $derived(store.selectedProviderID);

  $effect(() => {
    void selectedID;
    newModelID = "";
    newModelName = "";
    expandedModelID = null;
    checking = false;
    costDrafts = {};
  });

  function draftOf(id: string) {
    return store.providerDrafts[id] ?? store.document.providers.find((provider) => provider.id === id);
  }

  function setProviderField(key: "id" | "name" | "baseURL" | "catalogURL" | "apiKey", value: string) {
    store.updateSelectedProvider((provider) => {
      provider[key] = value;
      if (key === "baseURL") provider.compatBaseURL = "";
    });
  }

  function setIntField(id: string, key: "context" | "output", text: string) {
    store.updateSelectedModel(id, (model) => {
      const digits = text.replace(/\D/g, "");
      model[key] = digits ? Number(digits) : undefined;
    });
  }

  function intText(model: ModelEntry, key: "context" | "output"): string {
    const value = model[key];
    return value == null ? "" : String(value);
  }

  function setCostField(id: string, key: keyof ModelCost, text: string) {
    store.updateSelectedModel(id, (model) => {
      const cost = { ...(model.cost ?? {}) };
      const value = text.trim() === "" ? undefined : Number(text);
      if (value === undefined || !Number.isFinite(value) || value < 0) {
        delete cost[key];
      } else {
        cost[key] = value;
      }
      model.cost = Object.values(cost).some((item) => item !== undefined) ? cost : undefined;
    });
  }

  function costKey(model: ModelEntry, key: keyof ModelCost): string {
    return `${store.selectedProviderID ?? ""}:${model.id}:${key}`;
  }

  function costText(model: ModelEntry, key: keyof ModelCost): string {
    const draft = costDrafts[costKey(model, key)];
    if (draft !== undefined) return draft;
    const value = model.cost?.[key];
    return value == null ? "" : String(value);
  }

  function inputCost(model: ModelEntry, key: keyof ModelCost, text: string) {
    costDrafts[costKey(model, key)] = text;
    setCostField(model.id, key, text);
  }

  function blurCost(model: ModelEntry, key: keyof ModelCost) {
    delete costDrafts[costKey(model, key)];
  }

  function modalityOn(model: ModelEntry, kind: string): boolean {
    return model.input.includes(kind) || (model.input.length === 0 && kind === "text");
  }

  function setModality(id: string, kind: string, on: boolean) {
    store.updateSelectedModel(id, (model) => {
      let input = model.input.length === 0 ? ["text"] : [...model.input];
      if (on) {
        if (!input.includes(kind)) input.push(kind);
        input = INPUT_KINDS.filter((item) => input.includes(item));
      } else {
        input = input.filter((item) => item !== kind);
        if (input.length === 0) input = ["text"];
      }
      model.input = input;
    });
  }

  function effortOn(model: ModelEntry, level: string): boolean {
    return model.efforts?.includes(level) ?? false;
  }

  function setEffort(id: string, level: string, on: boolean) {
    store.updateSelectedModel(id, (model) => {
      let efforts = [...(model.efforts ?? [])];
      if (on) {
        if (!efforts.includes(level)) efforts.push(level);
      } else {
        efforts = efforts.filter((item) => item !== level);
      }
      model.efforts = EFFORT_LEVELS.filter((item) => efforts.includes(item));
    });
  }

  function addModel() {
    store.addModel(newModelID, newModelName);
    newModelID = "";
    newModelName = "";
  }

  async function pullCatalog() {
    pulling = true;
    try {
      await store.pullCatalog();
    } finally {
      pulling = false;
    }
  }

  async function checkCompat() {
    checking = true;
    try {
      await store.checkCompat();
    } finally {
      checking = false;
    }
  }

  function clearCompat() {
    store.updateSelectedProvider((provider) => {
      provider.compatBaseURL = "";
    });
  }
</script>

<div class="flex h-full min-h-0">
  <div class="w-[220px] shrink-0 border-r border-slate-200/80 bg-slate-50/80 flex flex-col min-h-0">
    <div class="flex-1 overflow-y-auto p-2 space-y-0.5">
      {#each store.document.providers as provider (provider.id)}
        {@const draft = draftOf(provider.id)}
        {#if draft}
          {@const selected = store.selectedProviderID === provider.id}
          <button
            type="button"
            class="w-full text-left px-2.5 py-1.5 rounded-lg transition cursor-pointer {selected
              ? 'bg-white text-slate-900 font-semibold shadow-2xs border border-slate-200/80'
              : 'text-slate-600 hover:bg-slate-100 hover:text-slate-900 border border-transparent'}"
            onclick={() => (store.selectedProviderID = provider.id)}
          >
            <span class="flex items-center gap-1.5">
              <span class="text-xs truncate">{draft.name || draft.id}</span>
              {#if store.isProviderDirty(provider.id)}
                <span class="h-1.5 w-1.5 rounded-full bg-orange-500 shrink-0"></span>
              {/if}
            </span>
            <span class="mt-0.5 flex items-center gap-1.5 min-w-0">
              <MetaPill text={API_SHORT[draft.api]} />
              <span class="text-[10px] text-slate-400">{t("cfg_n_models", { n: draft.models.length })}</span>
              {#if draft.id !== provider.id}
                <span class="text-[10px] text-orange-500 truncate">{t("cfg_id_arrow", { id: draft.id })}</span>
              {/if}
            </span>
          </button>
        {/if}
      {/each}
    </div>
    <div class="shrink-0 flex items-center gap-2 px-3 py-2.5 border-t border-slate-200/80">
      <button type="button" class={btnGhost} onclick={() => store.addProvider()}>{t("cfg_add")}</button>
      <button
        type="button"
        class={btnDanger}
        disabled={store.selectedProviderID == null}
        onclick={() => store.removeSelectedProvider()}
      >
        {t("cfg_delete")}
      </button>
    </div>
  </div>

  <div class="flex-1 min-w-0 flex flex-col min-h-0">
    {#if !store.selectedProvider}
      <div class="flex-1 flex flex-col items-center justify-center gap-2 text-center px-8">
        <Unplug class="h-8 w-8 text-slate-300" />
        <div class="text-sm font-semibold text-slate-600">{t("cfg_no_providers")}</div>
        <p class="text-xs text-slate-400">{t("cfg_no_providers_desc")}</p>
      </div>
    {:else}
      {@const provider = store.selectedProvider}
      <div class="flex-1 overflow-y-auto p-5 space-y-4">
        <ConfigCard>
          <SectionLabel title={t("cfg_connection")} />
          <div class="grid grid-cols-2 gap-x-3 gap-y-2.5">
            <FieldLabel title={t("cfg_id")}>
              <input class={fieldClass} placeholder={t("cfg_id_placeholder")} value={provider.id} oninput={(e) => setProviderField("id", e.currentTarget.value)} />
            </FieldLabel>
            <FieldLabel title={t("cfg_display_name")}>
              <input class={fieldClass} placeholder={t("cfg_name_placeholder")} value={provider.name} oninput={(e) => setProviderField("name", e.currentTarget.value)} />
            </FieldLabel>
            <FieldLabel title={t("cfg_base_url")}>
              <input class={fieldClass} placeholder={t("cfg_base_url_placeholder")} value={provider.baseURL} oninput={(e) => setProviderField("baseURL", e.currentTarget.value)} />
            </FieldLabel>
            <FieldLabel title={t("cfg_protocol")} class="max-w-[220px]">
              <select
                class={selectClass}
                value={provider.api}
                onchange={(e) =>
                  store.updateSelectedProvider((next) => {
                    next.api = e.currentTarget.value as APIKind;
                    next.compatBaseURL = "";
                  })}
              >
                {#each API_KINDS as kind}
                  <option value={kind}>{API_TITLE[kind]}</option>
                {/each}
              </select>
            </FieldLabel>
            <FieldLabel title={t("cfg_catalog_url")}>
              <input
                class={fieldClass}
                placeholder={t("cfg_catalog_url_placeholder")}
                value={provider.catalogURL}
                oninput={(e) => setProviderField("catalogURL", e.currentTarget.value)}
              />
            </FieldLabel>
            <FieldLabel title={t("cfg_api_key")}>
              <input
                class={fieldClass}
                type="password"
                placeholder={t("cfg_api_key_placeholder")}
                value={provider.apiKey}
                oninput={(e) => setProviderField("apiKey", e.currentTarget.value)}
              />
            </FieldLabel>
          </div>
          <p class="text-[11px] text-slate-400 leading-relaxed">{t("cfg_catalog_hint")}</p>
          <p class="text-[11px] text-slate-400 leading-relaxed">{t("cfg_base_url_hint")}</p>
          {#if provider.api === "google"}
            <p class="text-[11px] text-slate-400 leading-relaxed">
              {t("cfg_google_protocol_hint", {
                agents: agentsWithoutAPI("google")
                  .map((kind) => AGENT_TITLE[kind])
                  .join(i18n.locale.startsWith("zh") ? "、" : ", "),
              })}
            </p>
            <div class="flex items-center gap-2 flex-wrap">
              <button type="button" class={btnBorder} disabled={checking} onclick={() => checkCompat()}>
                {#if checking}
                  <LoaderCircle class="h-3 w-3 animate-spin" />
                {/if}
                {t("cfg_check_compat")}
              </button>
              {#if provider.compatBaseURL?.trim()}
                <span class="text-[11px] text-slate-500 min-w-0 truncate">{t("cfg_compat_status", { url: provider.compatBaseURL })}</span>
                <button type="button" class={btnGhost} onclick={() => clearCompat()}>{t("cfg_compat_clear")}</button>
              {:else}
                <span class="text-[11px] text-slate-400">{t("cfg_compat_undetected")}</span>
              {/if}
            </div>
          {/if}
        </ConfigCard>

        <ConfigCard>
          <div class="flex items-center gap-2">
            <div class="flex-1 min-w-0">
              <SectionLabel title={t("cfg_models")} accessory={String(provider.models.length)} />
            </div>
            <button type="button" class={btnBorder} disabled={pulling} onclick={() => pullCatalog()}>
              {#if pulling}
                <LoaderCircle class="h-3 w-3 animate-spin" />
              {:else}
                <RefreshCw class="h-3 w-3" />
              {/if}
              {t("cfg_pull_catalog")}
            </button>
          </div>

          {#if provider.models.length === 0}
            <p class="text-xs text-slate-400 py-2">{t("cfg_no_models")}</p>
          {:else}
            <div class="space-y-1.5">
              {#each provider.models as model (model.id)}
                {@const expanded = expandedModelID === model.id}
                <div class="rounded-[10px] bg-slate-50/80 p-2 space-y-2">
                  <div class="flex items-center gap-2">
                    <button
                      type="button"
                      class="flex-1 min-w-0 flex items-center gap-2 text-left cursor-pointer"
                      onclick={() => (expandedModelID = expanded ? null : model.id)}
                    >
                      {#if expanded}
                        <ChevronDown class="h-3 w-3 text-slate-400 shrink-0" />
                      {:else}
                        <ChevronRight class="h-3 w-3 text-slate-400 shrink-0" />
                      {/if}
                      <span class="min-w-0 flex-1">
                        <span class="block text-xs font-medium text-slate-800 truncate">{model.name}</span>
                        <span class="block text-[10px] text-slate-400 truncate">{model.id}</span>
                      </span>
                      {#each summaryPills(model) as pill}
                        <MetaPill text={pill} />
                      {/each}
                    </button>
                    <button
                      type="button"
                      class="p-1 rounded text-slate-400 hover:text-rose-600 hover:bg-rose-50 cursor-pointer"
                      title={t("cfg_delete")}
                      onclick={() => store.removeModel(model.id)}
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </button>
                  </div>
                  {#if expanded}
                    <div class="rounded-lg bg-white/80 border border-slate-100 p-2.5 space-y-2.5">
                      <div class="flex items-end gap-3">
                        <FieldLabel title={t("cfg_context")} class="flex-1">
                          <input
                            class={fieldClass}
                            inputmode="numeric"
                            placeholder={t("cfg_tokens")}
                            value={intText(model, "context")}
                            oninput={(e) => setIntField(model.id, "context", e.currentTarget.value)}
                          />
                        </FieldLabel>
                        <FieldLabel title={t("cfg_output")} class="flex-1">
                          <input
                            class={fieldClass}
                            inputmode="numeric"
                            placeholder={t("cfg_tokens")}
                            value={intText(model, "output")}
                            oninput={(e) => setIntField(model.id, "output", e.currentTarget.value)}
                          />
                        </FieldLabel>
                      </div>
                      <FieldLabel title={t("cfg_cost")}>
                        <div class="grid grid-cols-2 gap-2">
                          <div class="flex items-center gap-2 min-w-0">
                            <span class="w-16 shrink-0 text-[11px] text-slate-400">{t("cfg_cost_input")}</span>
                            <input
                              class={fieldClass}
                              inputmode="decimal"
                              placeholder="0"
                              value={costText(model, "input")}
                              oninput={(e) => inputCost(model, "input", e.currentTarget.value)}
                              onblur={() => blurCost(model, "input")}
                            />
                          </div>
                          <div class="flex items-center gap-2 min-w-0">
                            <span class="w-16 shrink-0 text-[11px] text-slate-400">{t("cfg_cost_output")}</span>
                            <input
                              class={fieldClass}
                              inputmode="decimal"
                              placeholder="0"
                              value={costText(model, "output")}
                              oninput={(e) => inputCost(model, "output", e.currentTarget.value)}
                              onblur={() => blurCost(model, "output")}
                            />
                          </div>
                          <div class="flex items-center gap-2 min-w-0">
                            <span class="w-16 shrink-0 text-[11px] text-slate-400">{t("cfg_cost_cache_read")}</span>
                            <input
                              class={fieldClass}
                              inputmode="decimal"
                              placeholder="0"
                              value={costText(model, "cacheRead")}
                              oninput={(e) => inputCost(model, "cacheRead", e.currentTarget.value)}
                              onblur={() => blurCost(model, "cacheRead")}
                            />
                          </div>
                          <div class="flex items-center gap-2 min-w-0">
                            <span class="w-16 shrink-0 text-[11px] text-slate-400">{t("cfg_cost_cache_write")}</span>
                            <input
                              class={fieldClass}
                              inputmode="decimal"
                              placeholder="0"
                              value={costText(model, "cacheWrite")}
                              oninput={(e) => inputCost(model, "cacheWrite", e.currentTarget.value)}
                              onblur={() => blurCost(model, "cacheWrite")}
                            />
                          </div>
                        </div>
                      </FieldLabel>
                      <p class="text-[10px] text-slate-400 leading-relaxed">{t("cfg_cost_hint")}</p>
                      <FieldLabel title={t("cfg_modality")}>
                        <div class="flex flex-wrap gap-1.5">
                          {#each INPUT_KINDS as kind}
                            <button
                              type="button"
                              class={modalityOn(model, kind) ? chipOn : chipIdle}
                              onclick={() => setModality(model.id, kind, !modalityOn(model, kind))}
                            >
                              {kind}
                            </button>
                          {/each}
                        </div>
                      </FieldLabel>
                      <FieldLabel title={t("cfg_effort")}>
                        <div class="flex flex-wrap gap-1.5">
                          {#each EFFORT_LEVELS as level}
                            <button
                              type="button"
                              class={effortOn(model, level) ? chipOn : chipIdle}
                              onclick={() => setEffort(model.id, level, !effortOn(model, level))}
                            >
                              {level}
                            </button>
                          {/each}
                        </div>
                      </FieldLabel>
                    </div>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}

          <div class="flex items-end gap-2">
            <FieldLabel title={t("cfg_model_id")} class="flex-1">
              <input
                class={fieldClass}
                placeholder={t("cfg_model_id_placeholder")}
                value={newModelID}
                oninput={(e) => (newModelID = e.currentTarget.value)}
              />
            </FieldLabel>
            <FieldLabel title={t("cfg_display_name")} class="w-[180px] shrink-0">
              <input
                class={fieldClass}
                placeholder={t("cfg_model_name_placeholder")}
                value={newModelName}
                oninput={(e) => (newModelName = e.currentTarget.value)}
              />
            </FieldLabel>
            <button
              type="button"
              class="{btnBorder} h-[30px] shrink-0"
              disabled={!newModelID.trim()}
              onclick={addModel}
            >
              {t("cfg_add_model")}
            </button>
          </div>
        </ConfigCard>
      </div>
      <SaveBar
        isDirty={store.isDirty}
        hint={t("cfg_save_hint_providers")}
        saveTitle={t("cfg_save")}
        onSave={() => { void store.saveProviderDraft(); }}
        onDiscard={() => store.revert()}
      />
    {/if}
  </div>
</div>
