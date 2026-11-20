import { invoke } from "@tauri-apps/api/core";
import { t } from "$lib/i18n";
import {
  AGENT_KINDS,
  type AgentKind,
  type AgentSettings,
  type AppDocument,
  type ApplyResult,
  type DetectEntry,
  type DiscoveredDiskMcp,
  type InstructionBackupEntry,
  type InstructionBinding,
  type InstructionPreset,
  type InstructionSyncOutcome,
  type MCPServer,
  type ModelEntry,
  type Provider,
  type SidebarItem,
  type StoreToast,
  type TargetScanRow,
} from "./types";
import {
  AGENT_TITLE,
  agentSupportsProviders,
  catalogRefs,
  cloneDoc,
  deepEqual,
  emptyDocument,
  invokeError,
  isAgentKind,
  normalizeDocument,
  normalizeMCP,
  normalizeModel,
  renameProviderRefs,
  resolvedHint,
  settingsFor,
  uniqueMCPName,
  uniqueProviderID,
  deriveModelName,
} from "./document";

function toRetiredProvider(provider: Provider): Provider {
  return {
    ...cloneDoc(provider),
    apiKey: "",
    catalogURL: "",
    enabled: true,
  };
}

function recordRetiredProvider(document: AppDocument, provider: Provider): void {
  const retired = toRetiredProvider(provider);
  const list = [...(document.retiredProviders ?? [])].filter((item) => item.id !== retired.id);
  list.push(retired);
  document.retiredProviders = list;
}

function unretireProviderID(document: AppDocument, id: string): void {
  if (!document.retiredProviders?.length) return;
  document.retiredProviders = document.retiredProviders.filter((item) => item.id !== id);
}

export class ConfigStore {
  document = $state<AppDocument>(emptyDocument());
  baseline = $state<AppDocument>(emptyDocument());
  providerDrafts = $state<Record<string, Provider>>({});
  selectedSidebar = $state<SidebarItem>("agents");
  selectedAgent = $state<AgentKind>("opencode");
  selectedProviderID = $state<string | null>(null);
  selectedPresetID = $state<string | null>(null);
  scannedTargets = $state<TargetScanRow[]>([]);
  scanningTargets = $state(false);
  status = $state("");
  errorMessage = $state("");
  toast = $state<StoreToast | null>(null);
  isExporting = $state(false);
  isSyncingMcp = $state(false);
  isSyncingInstructions = $state(false);
  detector = $state<DetectEntry[]>([]);
  dshProfiles = $state<string[]>([]);
  loaded = $state(false);
  loading = $state(false);

  isDirty = $derived.by(() => {
    if (!deepEqual(this.document, this.baseline)) return true;
    return Object.keys(this.providerDrafts).some((id) => this.isProviderDirty(id));
  });

  selectedProvider = $derived.by(() => {
    const id = this.selectedProviderID;
    if (!id) return null;
    return this.providerDrafts[id] ?? this.document.providers.find((provider) => provider.id === id) ?? null;
  });

  selectedSettings = $derived.by(() => settingsFor(this.document, this.selectedAgent));

  private toastTimer: ReturnType<typeof setTimeout> | null = null;
  private toastSeq = 0;

  rankedAgents(): { kind: AgentKind; installed: boolean }[] {
    const rows = this.detector.filter((entry) => isAgentKind(entry.kind));
    if (rows.length === 0) {
      return AGENT_KINDS.map((kind) => ({ kind, installed: false }));
    }
    return rows.map((entry) => ({ kind: entry.kind as AgentKind, installed: entry.installed }));
  }

  rankedProviderAgents(): { kind: AgentKind; installed: boolean }[] {
    return this.rankedAgents().filter((row) => agentSupportsProviders(row.kind));
  }

  isInstalled(kind: AgentKind): boolean {
    return this.detector.some((entry) => entry.kind === kind && entry.installed);
  }

  async refreshDetector(): Promise<void> {
    try {
      const entries = await invoke<DetectEntry[]>("config_detect");
      this.detector = Array.isArray(entries) ? entries : [];
      if (!this.loaded) {
        const firstInstalled = this.detector.find(
          (entry) => entry.installed && isAgentKind(entry.kind) && agentSupportsProviders(entry.kind),
        );
        if (firstInstalled && isAgentKind(firstInstalled.kind)) {
          this.selectedAgent = firstInstalled.kind;
        }
      }
      if (!agentSupportsProviders(this.selectedAgent)) {
        this.selectedAgent = this.rankedProviderAgents()[0]?.kind ?? "opencode";
      }
    } catch (err) {
      this.errorMessage = invokeError(err);
    }
  }

  async refreshDshProfiles(): Promise<void> {
    try {
      const profiles = await invoke<string[]>("config_dsh_profiles");
      this.dshProfiles = Array.isArray(profiles) ? profiles : [];
    } catch {
      this.dshProfiles = [];
    }
  }

  async ensureLoaded(): Promise<void> {
    if (this.loading) return;
    if (!this.loaded) {
      await this.load();
      return;
    }
    await Promise.all([this.refreshDetector(), this.refreshDshProfiles()]);
  }

  async load(): Promise<void> {
    this.loading = true;
    try {
      const raw = await invoke<unknown>("config_load");
      this.document = normalizeDocument(raw);
      this.baseline = cloneDoc(this.document);
      this.providerDrafts = {};
      if (!this.selectedProviderID || !this.document.providers.some((provider) => provider.id === this.selectedProviderID)) {
        this.selectedProviderID = this.document.providers[0]?.id ?? null;
      }
      if (!this.selectedPresetID || !this.document.instructions.presets.some((p) => p.id === this.selectedPresetID)) {
        this.selectedPresetID = this.document.instructions.presets[0]?.id ?? null;
      }
      await this.refreshDetector();
      await this.refreshDshProfiles();
      await this.scanInstructionTargets();
      this.loaded = true;
      this.errorMessage = "";
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.show({ title: this.errorMessage, detail: "", isError: true, revealedPath: "" });
    } finally {
      this.loading = false;
    }
  }

  updateSelectedSettings(mutate: (settings: AgentSettings) => void): void {
    this.updateAgentSettings(this.selectedAgent, mutate);
  }

  updateAgentSettings(kind: AgentKind, mutate: (settings: AgentSettings) => void): void {
    const next = cloneDoc(this.document);
    const settings = settingsFor(next, kind);
    mutate(settings);
    next.agents[kind] = settings;
    this.document = next;
  }

  isProviderDirty(id: string): boolean {
    const draft = this.providerDrafts[id];
    if (!draft) return false;
    const saved = this.document.providers.find((provider) => provider.id === id);
    return !deepEqual(draft, saved);
  }

  hasUnsavedProviderDrafts(): boolean {
    return Object.keys(this.providerDrafts).some((id) => this.isProviderDirty(id));
  }

  editProvider(id: string, mutate: (provider: Provider) => void): void {
    const source = this.providerDrafts[id] ?? this.document.providers.find((provider) => provider.id === id);
    if (!source) return;
    const draft = cloneDoc(source);
    mutate(draft);
    this.providerDrafts = { ...this.providerDrafts, [id]: draft };
  }

  updateSelectedProvider(mutate: (provider: Provider) => void): void {
    this.editSelectedProvider(mutate);
  }

  editSelectedProvider(mutate: (provider: Provider) => void): void {
    if (!this.selectedProviderID) return;
    this.editProvider(this.selectedProviderID, mutate);
  }

  updateSelectedModel(id: string, mutate: (model: ModelEntry) => void): void {
    this.editSelectedProvider((provider) => {
      const index = provider.models.findIndex((model) => model.id === id);
      if (index < 0) return;
      mutate(provider.models[index]);
    });
  }

  async saveProviderDraft(): Promise<boolean> {
    const selectedProviderID = this.selectedProviderID;
    const draft = selectedProviderID ? this.providerDrafts[selectedProviderID] : undefined;
    if (!selectedProviderID || !draft) {
      if (!deepEqual(this.document, this.baseline)) {
        await this.save();
        return true;
      }
      return false;
    }
    const index = this.document.providers.findIndex((provider) => provider.id === selectedProviderID);
    if (index < 0) {
      const drafts = { ...this.providerDrafts };
      delete drafts[selectedProviderID];
      this.providerDrafts = drafts;
      return false;
    }
    const trimmedID = draft.id.trim();
    if (!trimmedID) {
      this.errorMessage = t("cfg_err_empty_id");
      return false;
    }
    if (trimmedID !== selectedProviderID && this.document.providers.some((provider) => provider.id === trimmedID)) {
      this.errorMessage = t("cfg_err_id_taken", { id: trimmedID });
      return false;
    }
    const drafts = { ...this.providerDrafts };
    delete drafts[selectedProviderID];
    this.providerDrafts = drafts;
    const saved = cloneDoc(draft);
    saved.id = trimmedID;
    const next = cloneDoc(this.document);
    next.providers[index] = saved;
    if (trimmedID !== selectedProviderID) {
      recordRetiredProvider(next, this.document.providers[index]);
      renameProviderRefs(next, selectedProviderID, trimmedID);
    }
    unretireProviderID(next, trimmedID);
    if (deepEqual(next, this.document)) return false;
    this.document = next;
    this.selectedProviderID = trimmedID;
    await this.save();
    return true;
  }

  discardProviderDraft(): void {
    const selectedProviderID = this.selectedProviderID;
    if (!selectedProviderID) return;
    const drafts = { ...this.providerDrafts };
    delete drafts[selectedProviderID];
    this.providerDrafts = drafts;
    if (!this.document.providers.some((provider) => provider.id === selectedProviderID)) {
      this.selectedProviderID = this.document.providers[0]?.id ?? null;
    }
    this.status = t("cfg_discarded_provider");
    this.errorMessage = "";
  }

  async save(reporting = true): Promise<void> {
    this.errorMessage = "";
    try {
      await invoke("config_save", { document: this.document });
      this.baseline = cloneDoc(this.document);
      this.status = t("cfg_saved");
      if (reporting) {
        this.show({ title: t("cfg_saved"), detail: "", isError: false, revealedPath: "" });
      }
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.show({ title: t("cfg_save_failed"), detail: this.errorMessage, isError: true, revealedPath: "" });
    }
  }

  revert(): void {
    if (!this.isDirty) return;
    this.providerDrafts = {};
    this.document = cloneDoc(this.baseline);
    if (this.selectedProviderID && !this.document.providers.some((provider) => provider.id === this.selectedProviderID)) {
      this.selectedProviderID = this.document.providers[0]?.id ?? null;
    }
    this.status = t("cfg_discarded");
    this.errorMessage = "";
  }

  show(toast: Omit<StoreToast, "id">): void {
    this.toast = { ...toast, id: ++this.toastSeq };
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.toastTimer = setTimeout(
      () => {
        this.toast = null;
      },
      toast.isError ? 6000 : 3500,
    );
  }

  dismissToast(): void {
    if (this.toastTimer) clearTimeout(this.toastTimer);
    this.toastTimer = null;
    this.toast = null;
  }

  addProvider(): void {
    const id = uniqueProviderID(this.document.providers);
    const next = cloneDoc(this.document);
    next.providers.push({
      id,
      name: t("cfg_new_provider"),
      baseURL: "",
      api: "responses",
      catalogURL: "",
      apiKey: "",
      enabled: true,
      models: [],
      compatBaseURL: "",
    });
    unretireProviderID(next, id);
    this.document = next;
    this.selectedProviderID = id;
    this.status = t("cfg_added_remember", { id });
  }

  removeSelectedProvider(): void {
    const selectedProviderID = this.selectedProviderID;
    if (!selectedProviderID) return;
    const index = this.document.providers.findIndex((provider) => provider.id === selectedProviderID);
    if (index < 0) return;
    const drafts = { ...this.providerDrafts };
    delete drafts[selectedProviderID];
    this.providerDrafts = drafts;
    const next = cloneDoc(this.document);
    const removedProvider = next.providers[index];
    const removed = removedProvider.id;
    recordRetiredProvider(next, removedProvider);
    next.providers.splice(index, 1);
    const prefix = `${removed}/`;
    for (const kind of AGENT_KINDS) {
      const settings = settingsFor(next, kind);
      settings.pickedModelIDs = settings.pickedModelIDs.filter((ref) => !ref.startsWith(prefix));
      if (settings.model.startsWith(prefix)) settings.model = "";
      if (settings.smallModel.startsWith(prefix)) settings.smallModel = "";
      for (const ref of Object.keys(settings.effortByModel)) {
        if (ref.startsWith(prefix)) delete settings.effortByModel[ref];
      }
      next.agents[kind] = settings;
    }
    this.document = next;
    this.selectedProviderID = this.document.providers[0]?.id ?? null;
    this.status = t("cfg_deleted_remember", { id: removed });
  }

  addMCP(): void {
    const name = uniqueMCPName(this.document.library.mcp);
    const next = cloneDoc(this.document);
    next.library.mcp.push({
      id: name,
      transport: "stdio",
      command: "npx",
      args: [],
      url: "",
      env: {},
    });
    this.document = next;
    this.status = t("cfg_added_remember", { id: name });
  }

  async importMCP(text: string): Promise<boolean> {
    try {
      const parsed = await invoke<MCPServer[]>("config_parse_mcp", { text });
      const servers = Array.isArray(parsed) ? parsed.map(normalizeMCP) : [];
      const next = cloneDoc(this.document);
      for (const server of servers) {
        const index = next.library.mcp.findIndex((item) => item.id === server.id);
        if (index >= 0) next.library.mcp[index] = server;
        else next.library.mcp.push(server);
      }
      this.document = next;
      this.errorMessage = "";
      this.status = t("cfg_parsed_mcp", { n: servers.length });
      return true;
    } catch (err) {
      this.errorMessage = invokeError(err);
      return false;
    }
  }

  removeMCP(id: string): void {
    const next = cloneDoc(this.document);
    next.library.mcp = next.library.mcp.filter((server) => server.id !== id);
    for (const kind of AGENT_KINDS) {
      const settings = settingsFor(next, kind);
      settings.mcpIDs = settings.mcpIDs.filter((item) => item !== id);
      next.agents[kind] = settings;
    }
    this.document = next;
    this.status = t("cfg_deleted_remember", { id });
  }

  async checkCompat(): Promise<void> {
    const provider = this.selectedProvider;
    const providerID = this.selectedProviderID;
    if (!provider || !providerID) {
      this.errorMessage = t("cfg_err_pick_provider");
      return;
    }
    this.errorMessage = "";
    this.status = t("cfg_checking_compat");
    try {
      const url = await invoke<string>("config_check_compat", { provider });
      const compatURL = typeof url === "string" ? url.trim() : "";
      this.editProvider(providerID, (selected) => {
        selected.compatBaseURL = compatURL;
      });
      this.status = t("cfg_compat_ok");
      this.show({
        title: t("cfg_compat_ok"),
        detail: t("cfg_unsaved_detail"),
        isError: false,
        revealedPath: "",
      });
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.status = "";
      this.show({
        title: t("cfg_compat_failed"),
        detail: this.errorMessage,
        isError: true,
        revealedPath: "",
      });
    }
  }

  async pullCatalog(): Promise<void> {
    const provider = this.selectedProvider;
    if (!provider) {
      this.errorMessage = t("cfg_err_pick_provider");
      return;
    }
    const authKey = provider.apiKey.trim();
    if (!authKey) {
      this.errorMessage = t("cfg_err_api_key");
      this.status = "";
      return;
    }
    this.errorMessage = "";
    this.status = t("cfg_pulling");
    try {
      const models = await invoke<ModelEntry[]>("config_pull_catalog", { provider });
      const nextModels = Array.isArray(models) ? models.map(normalizeModel) : [];
      this.updateSelectedProvider((selected) => {
        selected.models = nextModels;
      });
      this.status = t("cfg_pulled", { n: nextModels.length });
      this.show({
        title: t("cfg_pulled_toast", { n: nextModels.length }),
        detail: t("cfg_unsaved_detail"),
        isError: false,
        revealedPath: "",
      });
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.status = "";
    }
  }

  async searchModels(query: string, limit = 50): Promise<ModelEntry[]> {
    const hits = await invoke<ModelEntry[]>("config_search_models", { query, limit });
    return Array.isArray(hits) ? hits.map(normalizeModel) : [];
  }

  async refreshModelsDev(options: { toast?: boolean } = {}): Promise<number> {
    this.errorMessage = "";
    this.status = t("cfg_refreshing_models_dev");
    try {
      const count = await invoke<number>("config_refresh_models_dev");
      this.status = t("cfg_refreshed_models_dev", { n: count });
      if (options.toast !== false) {
        this.show({
          title: t("cfg_refreshed_models_dev", { n: count }),
          detail: "",
          isError: false,
          revealedPath: "",
        });
      }
      return count;
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.status = "";
      throw err;
    }
  }

  addModel(id: string, name: string): void {
    const trimmed = id.trim();
    if (!trimmed) {
      this.errorMessage = t("cfg_err_empty_model");
      return;
    }
    const display = name.trim();
    this.errorMessage = "";
    this.updateSelectedProvider((provider) => {
      const index = provider.models.findIndex((model) => model.id === trimmed);
      if (index >= 0) {
        if (display) provider.models[index].name = display;
        return;
      }
      provider.models.push({
        id: trimmed,
        name: display || deriveModelName(trimmed) || trimmed,
        input: [],
        toolCall: true,
      });
    });
    this.status = t("cfg_added_model", { id: trimmed });
  }

  removeModel(id: string): void {
    this.updateSelectedProvider((provider) => {
      provider.models = provider.models.filter((model) => model.id !== id);
    });
    this.status = t("cfg_deleted_remember", { id });
  }

  async apply(kind: AgentKind): Promise<void> {
    if (this.hasUnsavedProviderDrafts()) {
      this.errorMessage = t("cfg_err_unsaved_providers");
      this.show({
        title: t("cfg_save_first"),
        detail: this.errorMessage,
        isError: true,
        revealedPath: "",
      });
      return;
    }
    await this.save(false);
    if (this.errorMessage) return;
    this.errorMessage = "";
    this.isExporting = true;
    try {
      const result = await invoke<ApplyResult>("config_apply", { kind, document: this.document });
      this.document = normalizeDocument(result.document);
      this.baseline = cloneDoc(this.document);
      this.providerDrafts = {};
      const profile = settingsFor(this.document, kind).profile;
      const hint = resolvedHint(kind, profile);
      const lines = result.written.length ? [...result.written] : [hint];
      if (result.backups?.length) {
        lines.push(t("cfg_toast_backups", { n: result.backups.length }));
      }
      this.status = t("cfg_status_written", { path: hint });
      this.show({
        title: t("cfg_exported", { title: AGENT_TITLE[kind] }),
        detail: lines.join("\n"),
        isError: false,
        revealedPath: result.written[0] ?? "",
      });
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.status = "";
      this.show({ title: t("cfg_export_failed"), detail: this.errorMessage, isError: true, revealedPath: "" });
    } finally {
      this.isExporting = false;
    }
  }

  catalogRefs(): string[] {
    return catalogRefs(this.document);
  }

  appliedLabel(kind: AgentKind): string {
    const raw = this.document.appliedAt[kind];
    if (!raw) return t("cfg_not_applied");
    const date = new Date(raw);
    if (Number.isNaN(date.getTime())) return t("cfg_not_applied");
    return date.toLocaleString(undefined, { dateStyle: "short", timeStyle: "short" });
  }

  setInstructions(value: string): void {
    const next = cloneDoc(this.document);
    next.library.instructions = value;
    this.document = next;
  }

  selectedPreset = $derived.by(() => {
    const id = this.selectedPresetID;
    if (!id) return null;
    return this.document.instructions.presets.find((p) => p.id === id) ?? null;
  });

  addPreset(name?: string, content = "", description = ""): InstructionPreset {
    const existing = new Set(this.document.instructions.presets.map((p) => p.id));
    let n = this.document.instructions.presets.length + 1;
    let id = `preset-${n}`;
    while (existing.has(id)) {
      n += 1;
      id = `preset-${n}`;
    }
    const preset: InstructionPreset = {
      id,
      name: name?.trim() || `${t("inst_new_preset")} ${n}`,
      description,
      tags: [],
      content,
      updatedAt: new Date().toISOString(),
    };
    const next = cloneDoc(this.document);
    next.instructions.presets.push(preset);
    this.document = next;
    this.selectedPresetID = preset.id;
    return preset;
  }

  updatePreset(id: string, mutate: (preset: InstructionPreset) => void): void {
    const next = cloneDoc(this.document);
    const target = next.instructions.presets.find((p) => p.id === id);
    if (!target) return;
    mutate(target);
    target.updatedAt = new Date().toISOString();
    if (next.instructions.presets[0]?.id === id) {
      next.library.instructions = target.content;
    }
    this.document = next;
  }

  removePreset(id: string): void {
    const next = cloneDoc(this.document);
    next.instructions.presets = next.instructions.presets.filter((p) => p.id !== id);
    for (const binding of next.instructions.bindings) {
      if (binding.presetId === id) {
        binding.presetId = "";
        binding.enabled = false;
      }
    }
    this.document = next;
    if (this.selectedPresetID === id) {
      this.selectedPresetID = this.document.instructions.presets[0]?.id ?? null;
    }
  }

  setTargetBinding(targetId: string, mutate: (binding: InstructionBinding) => void): void {
    const next = cloneDoc(this.document);
    let binding = next.instructions.bindings.find((b) => b.targetId === targetId);
    if (!binding) {
      binding = {
        targetId,
        enabled: true,
        presetId: this.selectedPresetID ?? next.instructions.presets[0]?.id ?? "",
      };
      next.instructions.bindings.push(binding);
    }
    mutate(binding);
    // Keep AgentSettings.instructionOn in sync for the 6 exportable agents
    if (isAgentKind(targetId) && next.agents[targetId]) {
      next.agents[targetId].instructionOn = binding.enabled;
    }
    this.document = next;
  }

  addCustomTarget(name: string, path: string, kind: "workspace" | "custom" = "custom", presetId = ""): void {
    const trimmedPath = path.trim();
    if (!trimmedPath) return;
    const next = cloneDoc(this.document);
    const existing = next.instructions.customTargets.find((c) => c.path === trimmedPath);
    if (existing) {
      if (presetId) {
        const b = next.instructions.bindings.find((item) => item.targetId === existing.id);
        if (b) {
          b.enabled = true;
          b.presetId = presetId;
        }
      }
      this.document = next;
      return;
    }
    const id = `custom-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 6)}`;
    next.instructions.customTargets.push({
      id,
      name: name.trim() || trimmedPath.split("/").slice(-2).join("/"),
      path: trimmedPath,
      kind,
    });
    next.instructions.bindings.push({
      targetId: id,
      enabled: true,
      presetId: presetId || this.selectedPresetID || next.instructions.presets[0]?.id || "",
    });
    this.document = next;
  }

  removeCustomTarget(id: string): void {
    const next = cloneDoc(this.document);
    next.instructions.customTargets = next.instructions.customTargets.filter((c) => c.id !== id);
    next.instructions.bindings = next.instructions.bindings.filter((b) => b.targetId !== id);
    this.document = next;
  }

  async scanInstructionTargets(): Promise<void> {
    this.scanningTargets = true;
    try {
      const rows = await invoke<TargetScanRow[]>("instructions_scan_targets", {
        state: this.document.instructions,
      });
      this.scannedTargets = Array.isArray(rows) ? rows : [];
    } catch (err) {
      this.errorMessage = invokeError(err);
    } finally {
      this.scanningTargets = false;
    }
  }

  async readTargetFile(targetId: string): Promise<string> {
    return invoke<string>("instructions_read_target", {
      state: this.document.instructions,
      targetId,
    });
  }

  async syncInstructionTargets(targetIds: string[]): Promise<InstructionSyncOutcome | null> {
    try {
      const [updatedDoc, outcome] = await invoke<[AppDocument, InstructionSyncOutcome]>(
        "instructions_sync_targets",
        {
          document: this.document,
          targetIds,
        },
      );
      this.document = normalizeDocument(updatedDoc);
      this.baseline = cloneDoc(this.document);
      await this.scanInstructionTargets();
      if (outcome.written.length > 0) {
        const lines = [...outcome.written];
        if (outcome.backups?.length) {
          lines.push(t("cfg_toast_backups", { n: outcome.backups.length }));
        }
        this.show({
          title: t("inst_sync_success", { n: outcome.written.length }),
          detail: lines.join("\n"),
          isError: false,
          revealedPath: outcome.written[0] ?? "",
        });
      } else {
        const hasBound = this.document.instructions.bindings.some((b) => b.enabled && b.presetId);
        this.show({
          title: hasBound ? t("inst_sync_noop") : t("inst_saved_no_targets"),
          detail: "",
          isError: false,
          revealedPath: "",
        });
      }
      return outcome;
    } catch (err) {
      const msg = invokeError(err);
      this.errorMessage = msg;
      this.show({ title: t("cfg_save_failed"), detail: msg, isError: true, revealedPath: "" });
      return null;
    }
  }

  async saveAndSyncInstructions(): Promise<void> {
    this.errorMessage = "";
    this.isSyncingInstructions = true;
    try {
      await this.syncInstructionTargets([]);
    } finally {
      this.isSyncingInstructions = false;
    }
  }

  async saveAndSyncMcp(): Promise<void> {
    this.errorMessage = "";
    this.isSyncingMcp = true;
    try {
      const result = await invoke<ApplyResult>("config_sync_mcp", { document: this.document });
      this.document = normalizeDocument(result.document);
      this.baseline = cloneDoc(this.document);
      if (result.written.length > 0) {
        const lines = [...result.written];
        if (result.backups?.length) {
          lines.push(t("cfg_toast_backups", { n: result.backups.length }));
        }
        this.status = t("mcp_sync_success", { n: result.written.length });
        this.show({
          title: t("mcp_sync_success", { n: result.written.length }),
          detail: lines.join("\n"),
          isError: false,
          revealedPath: result.written[0] ?? "",
        });
      } else {
        this.status = t("mcp_sync_noop");
        this.show({
          title: t("mcp_sync_noop"),
          detail: "",
          isError: false,
          revealedPath: "",
        });
      }
    } catch (err) {
      this.errorMessage = invokeError(err);
      this.status = "";
      this.show({
        title: t("cfg_export_failed"),
        detail: this.errorMessage,
        isError: true,
        revealedPath: "",
      });
    } finally {
      this.isSyncingMcp = false;
    }
  }

  async listTargetBackups(targetId: string): Promise<InstructionBackupEntry[]> {
    return invoke<InstructionBackupEntry[]>("instructions_list_backups", {
      state: this.document.instructions,
      targetId,
    });
  }

  async restoreTargetBackup(targetId: string, backupPath: string): Promise<string> {
    const restored = await invoke<string>("instructions_restore_backup", {
      state: this.document.instructions,
      targetId,
      backupPath,
    });
    await this.scanInstructionTargets();
    this.show({
      title: t("inst_restore_backup"),
      detail: targetId,
      isError: false,
      revealedPath: "",
    });
    return restored;
  }

  updateMCP(id: string, mutate: (server: MCPServer) => void): void {
    const next = cloneDoc(this.document);
    const index = next.library.mcp.findIndex((server) => server.id === id);
    if (index < 0) return;
    mutate(next.library.mcp[index]);
    this.document = next;
  }

  async scanDiskMcp(): Promise<DiscoveredDiskMcp[]> {
    const hits = await invoke<DiscoveredDiskMcp[]>("config_scan_disk_mcp", {
      document: this.document,
    });
    return Array.isArray(hits) ? hits : [];
  }

  importDiskMcp(entry: DiscoveredDiskMcp): void {
    const server = normalizeMCP(entry.server);
    if (!server.id) return;
    const next = cloneDoc(this.document);
    const index = next.library.mcp.findIndex((item) => item.id === server.id);
    if (index >= 0) next.library.mcp[index] = server;
    else next.library.mcp.push(server);
    for (const rawKind of entry.agents) {
      if (!isAgentKind(rawKind)) continue;
      const settings = settingsFor(next, rawKind);
      if (!settings.mcpIDs.includes(server.id)) {
        settings.mcpIDs = [...settings.mcpIDs, server.id];
        next.agents[rawKind] = settings;
      }
    }
    this.document = next;
    this.status = t("cfg_added_remember", { id: server.id });
  }

  async revealPath(path: string): Promise<void> {
    if (!path) return;
    try {
      await invoke("reveal_path", { path });
    } catch (err) {
      this.errorMessage = invokeError(err);
    }
  }

  async saveForPage(): Promise<void> {
    switch (this.selectedSidebar) {
      case "agents":
        await this.apply(this.selectedAgent);
        break;
      case "providers":
        await this.saveProviderDraft();
        break;
      case "library":
        await this.save();
        break;
    }
  }
}

export const configStore = new ConfigStore();
