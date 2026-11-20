import { t } from "$lib/i18n";
import {
  AGENT_KINDS,
  API_KINDS,
  EFFORT_LEVELS,
  PROVIDER_AGENT_KINDS,
  type AgentKind,
  type AgentSettings,
  type APIKind,
  type AppDocument,
  type CustomInstructionTarget,
  type InstructionBinding,
  type InstructionPreset,
  type InstructionsState,
  type MCPServer,
  type MCPTransport,
  type ModelChoice,
  type ModelCost,
  type ModelEntry,
  type Provider,
} from "./types";

export const DSH_PROFILE_FALLBACK = "web";

export const AGENT_TITLE: Record<AgentKind, string> = {
  opencode: "OpenCode",
  dsh: "DSH",
  zcode: "ZCode",
  grok: "Grok",
  pi: "Pi",
  omp: "omp",
  antigravity: "Antigravity",
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  cursor: "Cursor",
  qwen: "Qwen Code",
};

export const AGENT_TINT: Record<AgentKind, string> = {
  opencode: "rgb(51, 122, 235)",
  dsh: "rgb(230, 122, 41)",
  zcode: "rgb(143, 84, 230)",
  grok: "rgb(41, 158, 148)",
  pi: "rgb(89, 184, 112)",
  omp: "rgb(219, 107, 153)",
  antigravity: "rgb(14, 165, 233)",
  claude: "rgb(217, 119, 87)",
  codex: "rgb(16, 185, 129)",
  gemini: "rgb(2, 132, 199)",
  cursor: "rgb(79, 70, 229)",
  qwen: "rgb(37, 99, 235)",
};

export function agentSupportsProviders(kind: AgentKind): boolean {
  return (PROVIDER_AGENT_KINDS as string[]).includes(kind);
}

export function agentSupportsInstructions(kind: AgentKind): boolean {
  return kind !== "cursor";
}

export function agentInstructionFileName(kind: AgentKind): string {
  switch (kind) {
    case "claude":
      return "CLAUDE.md";
    case "gemini":
      return "GEMINI.md";
    case "qwen":
      return "QWEN.md";
    case "cursor":
      return "";
    default:
      return "AGENTS.md";
  }
}

export const API_TITLE: Record<APIKind, string> = {
  responses: "OpenAI Responses",
  completions: "Chat Completions",
  anthropic: "Anthropic Messages",
  google: "Google",
};

export const API_SHORT: Record<APIKind, string> = {
  responses: "Responses",
  completions: "Completions",
  anthropic: "Anthropic",
  google: "Google",
};

export function isAgentKind(value: string): value is AgentKind {
  return (AGENT_KINDS as string[]).includes(value);
}

export function isAPIKind(value: string): value is APIKind {
  return (API_KINDS as string[]).includes(value);
}

export function agentSupportsAPI(kind: AgentKind, api: APIKind): boolean {
  if (!agentSupportsProviders(kind)) return false;
  switch (kind) {
    case "zcode":
    case "dsh":
    case "grok":
      return api !== "google";
    default:
      return true;
  }
}

export function agentsWithoutAPI(api: APIKind): AgentKind[] {
  return PROVIDER_AGENT_KINDS.filter((kind) => !agentSupportsAPI(kind, api));
}

/** OpenCode writes a thinking level on each model; other agents keep one default. */
export function agentWritesPerModelEffort(kind: AgentKind): boolean {
  return kind === "opencode";
}

export type ProviderExportMode = "native" | "compat" | "skip";

export function providerExportMode(kind: AgentKind, provider: Provider): ProviderExportMode {
  if (agentSupportsAPI(kind, provider.api)) return "native";
  if (provider.api === "google" && provider.compatBaseURL?.trim()) return "compat";
  return "skip";
}

export function cloneDoc<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

export function deepEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== typeof b) return false;
  if (a === null || b === null) return a === b;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return false;
    return a.every((item, index) => deepEqual(item, b[index]));
  }
  if (typeof a === "object" && typeof b === "object") {
    const left = a as Record<string, unknown>;
    const right = b as Record<string, unknown>;
    // cloneDoc drops undefined-valued keys, so they must compare equal to missing ones.
    const defined = (record: Record<string, unknown>) => Object.keys(record).filter((key) => record[key] !== undefined);
    const keys = defined(left);
    if (keys.length !== defined(right).length) return false;
    return keys.every((key) => deepEqual(left[key], right[key]));
  }
  return false;
}

export function emptySettings(kind?: AgentKind): AgentSettings {
  return {
    model: "",
    effort: "high",
    effortByModel: {},
    smallModel: "",
    pickedModelIDs: [],
    instructionOn: kind ? agentSupportsProviders(kind) : true,
    mcpIDs: [],
    profile: "",
  };
}

export function emptyInstructions(): InstructionsState {
  return { presets: [], bindings: [], customTargets: [] };
}

export function emptyDocument(): AppDocument {
  const agents: Record<string, AgentSettings> = {};
  for (const kind of AGENT_KINDS) {
    agents[kind] = emptySettings(kind);
  }
  return {
    version: 1,
    providers: [],
    retiredProviders: [],
    library: { instructions: "", mcp: [] },
    instructions: emptyInstructions(),
    agents,
    appliedAt: {},
  };
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
}

function asString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function asNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
}

function asBool(value: unknown, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function asStringMap(value: unknown): Record<string, string> {
  const row = asRecord(value);
  const out: Record<string, string> = {};
  for (const [key, item] of Object.entries(row)) {
    if (typeof item === "string") out[key] = item;
  }
  return out;
}

function asCost(raw: unknown): ModelCost | undefined {
  const row = asRecord(raw);
  const cost: ModelCost = {
    input: asNumber(row.input),
    output: asNumber(row.output),
    cacheRead: asNumber(row.cacheRead),
    cacheWrite: asNumber(row.cacheWrite),
  };
  return Object.values(cost).some((value) => value !== undefined) ? cost : undefined;
}

export function normalizeModel(raw: unknown): ModelEntry {
  const row = asRecord(raw);
  const id = asString(row.id);
  const name = asString(row.name, id);
  const efforts = Array.isArray(row.efforts) ? asStringArray(row.efforts) : undefined;
  return {
    id,
    requestId: typeof row.requestId === "string" && row.requestId ? row.requestId : undefined,
    name: name || id,
    type: row.type === "image" ? "image" : undefined,
    context: asNumber(row.context),
    output: asNumber(row.output),
    input: asStringArray(row.input),
    efforts,
    cost: asCost(row.cost),
    toolCall: asBool(row.toolCall, true),
  };
}

export function normalizeProvider(raw: unknown): Provider {
  const row = asRecord(raw);
  const apiRaw = asString(row.api, "responses");
  return {
    id: asString(row.id),
    name: asString(row.name),
    baseURL: asString(row.baseURL),
    api: isAPIKind(apiRaw) ? apiRaw : "responses",
    catalogURL: asString(row.catalogURL),
    apiKey: asString(row.apiKey),
    enabled: typeof row.enabled === "boolean" ? row.enabled : true,
    models: Array.isArray(row.models) ? row.models.map(normalizeModel) : [],
    compatBaseURL: asString(row.compatBaseURL),
  };
}

function normalizeTransport(value: unknown): MCPTransport {
  return value === "http" || value === "sse" ? value : "stdio";
}

export function normalizeMCP(raw: unknown): MCPServer {
  const row = asRecord(raw);
  const envRaw = asRecord(row.env);
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(envRaw)) {
    if (typeof value === "string") env[key] = value;
  }
  return {
    id: asString(row.id),
    transport: normalizeTransport(row.transport),
    command: asString(row.command),
    args: asStringArray(row.args),
    url: asString(row.url),
    env,
  };
}

export function normalizeSettings(raw: unknown, kind?: AgentKind): AgentSettings {
  const row = asRecord(raw);
  return {
    model: asString(row.model),
    effort: asString(row.effort, "high") || "high",
    effortByModel: asStringMap(row.effortByModel),
    smallModel: asString(row.smallModel),
    pickedModelIDs: asStringArray(row.pickedModelIDs),
    instructionOn: asBool(row.instructionOn, kind ? agentSupportsProviders(kind) : true),
    mcpIDs: asStringArray(row.mcpIDs),
    profile: asString(row.profile),
  };
}

function normalizePreset(raw: unknown): InstructionPreset {
  const row = asRecord(raw);
  return {
    id: asString(row.id),
    name: asString(row.name),
    description: asString(row.description),
    tags: asStringArray(row.tags),
    content: asString(row.content),
    updatedAt: asString(row.updatedAt),
  };
}

function normalizeBinding(raw: unknown): InstructionBinding {
  const row = asRecord(raw);
  return {
    targetId: asString(row.targetId),
    enabled: asBool(row.enabled, false),
    presetId: asString(row.presetId),
    customPath: typeof row.customPath === "string" && row.customPath ? row.customPath : undefined,
    lastSyncedHash: typeof row.lastSyncedHash === "string" && row.lastSyncedHash ? row.lastSyncedHash : undefined,
    lastSyncedAt: typeof row.lastSyncedAt === "string" && row.lastSyncedAt ? row.lastSyncedAt : undefined,
  };
}

function normalizeCustomTarget(raw: unknown): CustomInstructionTarget {
  const row = asRecord(raw);
  return {
    id: asString(row.id),
    name: asString(row.name),
    path: asString(row.path),
    kind: asString(row.kind, "custom"),
  };
}

export function normalizeInstructions(raw: unknown): InstructionsState {
  const row = asRecord(raw);
  return {
    presets: Array.isArray(row.presets) ? row.presets.map(normalizePreset) : [],
    bindings: Array.isArray(row.bindings) ? row.bindings.map(normalizeBinding) : [],
    customTargets: Array.isArray(row.customTargets) ? row.customTargets.map(normalizeCustomTarget) : [],
  };
}

export function normalizeDocument(raw: unknown): AppDocument {
  const row = asRecord(raw);
  const agents: Record<string, AgentSettings> = {};
  const agentsRaw = asRecord(row.agents);
  for (const kind of AGENT_KINDS) {
    agents[kind] = agentsRaw[kind] ? normalizeSettings(agentsRaw[kind], kind) : emptySettings(kind);
  }
  const appliedAt: Record<string, string> = {};
  const appliedRaw = asRecord(row.appliedAt);
  for (const kind of AGENT_KINDS) {
    const value = appliedRaw[kind];
    if (typeof value === "string" && value) appliedAt[kind] = value;
  }
  const version = asNumber(row.version);
  return {
    version: version ?? 1,
    providers: Array.isArray(row.providers) ? row.providers.map(normalizeProvider) : [],
    retiredProviders: Array.isArray(row.retiredProviders) ? row.retiredProviders.map(normalizeProvider) : [],
    library: {
      instructions: asString(asRecord(row.library).instructions),
      mcp: Array.isArray(asRecord(row.library).mcp)
        ? (asRecord(row.library).mcp as unknown[]).map(normalizeMCP)
        : [],
    },
    instructions: normalizeInstructions(row.instructions),
    agents,
    appliedAt,
  };
}

export function sanitizeDshProfile(raw: string): string {
  const trimmed = raw.trim();
  if (!trimmed) return DSH_PROFILE_FALLBACK;
  const mapped = Array.from(trimmed)
    .map((character) => (/[A-Za-z0-9_-]/.test(character) ? character : "-"))
    .join("");
  const value = mapped.replace(/^-+|-+$/g, "");
  return value || DSH_PROFILE_FALLBACK;
}

export function resolvedHint(kind: AgentKind, profile = ""): string {
  switch (kind) {
    case "opencode":
      return "~/.config/opencode/opencode.json";
    case "dsh":
      return `~/.dsh/profiles/${sanitizeDshProfile(profile)}/cordis.patch.yml`;
    case "zcode":
      return "~/.zcode/v2/config.json";
    case "grok":
      return "~/.grok/config.toml";
    case "pi":
      return "~/.pi/agent/models.json";
    case "omp":
      return "~/.omp/agent/models.yml";
    case "antigravity":
      return "~/.gemini/config/mcp_config.json · ~/.gemini/config/AGENTS.md";
    case "claude":
      return "~/.claude.json · ~/.claude/CLAUDE.md";
    case "codex":
      return "~/.codex/config.toml · ~/.codex/AGENTS.md";
    case "gemini":
      return "~/.gemini/settings.json · ~/.gemini/GEMINI.md";
    case "cursor":
      return "~/.cursor/mcp.json";
    case "qwen":
      return "~/.qwen/settings.json · ~/.qwen/QWEN.md";
  }
}

export function modelRef(providerId: string, modelId: string): string {
  return `${providerId}/${modelId}`;
}

export function catalogRefs(document: AppDocument): string[] {
  return document.providers
    .filter((provider) => provider.enabled)
    .flatMap((provider) => provider.models.map((model) => modelRef(provider.id, model.id)));
}

export function isPicked(settings: AgentSettings, ref: string, _catalog: string[]): boolean {
  if (settings.pickedModelIDs.length === 0) return true;
  return settings.pickedModelIDs.includes(ref);
}

export function setPicked(settings: AgentSettings, ref: string, on: boolean, catalog: string[]): void {
  const selected = new Set(settings.pickedModelIDs.length === 0 ? catalog : settings.pickedModelIDs);
  if (on) selected.add(ref);
  else selected.delete(ref);
  if (catalog.length > 0 && catalog.every((item) => selected.has(item))) {
    settings.pickedModelIDs = [];
  } else {
    settings.pickedModelIDs = catalog.filter((item) => selected.has(item));
  }
  if (settings.model && !isPicked(settings, settings.model, catalog)) settings.model = "";
  if (settings.smallModel && !isPicked(settings, settings.smallModel, catalog)) settings.smallModel = "";
  for (const ref of Object.keys(settings.effortByModel)) {
    if (!isPicked(settings, ref, catalog)) delete settings.effortByModel[ref];
  }
}

export function settingsFor(document: AppDocument, kind: AgentKind): AgentSettings {
  return document.agents[kind] ? cloneDoc(document.agents[kind]) : emptySettings(kind);
}

export function setSettings(document: AppDocument, kind: AgentKind, settings: AgentSettings): void {
  document.agents[kind] = settings;
}

export function pickedChoices(document: AppDocument, kind: AgentKind): ModelChoice[] {
  const settings = settingsFor(document, kind);
  const catalog = catalogRefs(document);
  const supported = document.providers.filter(
    (provider) => provider.enabled && provider.models.length > 0 && providerExportMode(kind, provider) !== "skip",
  );
  const multi = supported.length > 1;
  const choices: ModelChoice[] = [];
  for (const provider of supported) {
    const providerName = provider.name || provider.id;
    for (const model of provider.models) {
      const ref = modelRef(provider.id, model.id);
      if (!isPicked(settings, ref, catalog)) continue;
      choices.push({
        ref,
        title: multi ? `${model.name} · ${providerName}` : model.name,
      });
    }
  }
  return choices;
}

export function findModel(document: AppDocument, ref: string): ModelEntry | undefined {
  const core = ref.split("#")[0] ?? ref;
  const slash = core.indexOf("/");
  if (slash < 0) return undefined;
  const providerId = core.slice(0, slash);
  const modelId = core.slice(slash + 1);
  const provider = document.providers.find((item) => item.id === providerId);
  return provider?.models.find((model) => model.id === modelId);
}

export function renameProviderRefs(document: AppDocument, oldId: string, newId: string): void {
  if (oldId === newId || !oldId || !newId) return;
  const prefix = `${oldId}/`;
  const remap = (ref: string) => (ref.startsWith(prefix) ? `${newId}/${ref.slice(prefix.length)}` : ref);
  for (const kind of AGENT_KINDS) {
    const settings = settingsFor(document, kind);
    settings.model = remap(settings.model);
    settings.smallModel = remap(settings.smallModel);
    settings.pickedModelIDs = settings.pickedModelIDs.map(remap);
    const remapped: Record<string, string> = {};
    for (const [ref, effort] of Object.entries(settings.effortByModel)) {
      remapped[remap(ref)] = effort;
    }
    settings.effortByModel = remapped;
    setSettings(document, kind, settings);
  }
}

export function formatModelTokens(value: number): string {
  if (value >= 1_000_000 && value % 1_000_000 === 0) return `${value / 1_000_000}M`;
  if (value >= 1000 && value % 1000 === 0) return `${value / 1000}k`;
  return String(value);
}

export function profileSummary(model: ModelEntry): string {
  const parts: string[] = [];
  if (model.context != null) parts.push(formatModelTokens(model.context));
  const mods = model.input.length === 0 ? ["text"] : model.input;
  parts.push(mods.join("/"));
  if (model.type === "image") parts.push(t("cfg_image_badge"));
  if (model.efforts) {
    parts.push(model.efforts.length === 0 ? t("cfg_profile_no_thinking") : model.efforts.join(", "));
  }
  return parts.join(" · ");
}

export function summaryPills(model: ModelEntry): string[] {
  return profileSummary(model)
    .split("·")
    .map((part) => part.trim())
    .filter(Boolean);
}

const MODEL_NAME_CASING: Record<string, string> = {
  deepseek: "DeepSeek",
  gpt: "GPT",
  glm: "GLM",
  kimi: "Kimi",
  qwen: "Qwen",
  claude: "Claude",
  gemini: "Gemini",
  grok: "Grok",
  llama: "Llama",
  mistral: "Mistral",
  mixtral: "Mixtral",
  gemma: "Gemma",
  opus: "Opus",
  sonnet: "Sonnet",
  haiku: "Haiku",
  fable: "Fable",
  flash: "Flash",
  pro: "Pro",
  max: "Max",
  mini: "Mini",
  nano: "Nano",
  lite: "Lite",
  turbo: "Turbo",
  thinking: "Thinking",
  reasoner: "Reasoner",
  codex: "Codex",
};

function renderModelToken(token: string): string {
  const branded = MODEL_NAME_CASING[token.toLowerCase()];
  if (branded) return branded;
  if (/^v?\d+(?:\.\d+)*[a-z]?$/i.test(token)) {
    return token.charAt(0).toUpperCase() + token.slice(1);
  }
  if (token === token.toUpperCase() && /[A-Za-z]/.test(token)) return token;
  return token.charAt(0).toUpperCase() + token.slice(1);
}

export function deriveModelName(modelID: string): string {
  const core = modelID.trim();
  if (!core) return "";
  const segment = core.split("/").pop() ?? core;
  const tokens = segment.split(/[-_\s]+/).filter(Boolean);
  if (tokens.length === 0) return "";
  const merged: string[] = [];
  for (const token of tokens) {
    const last = merged.at(-1);
    if (last && token.length === 1 && /^\d$/.test(token) && last.length === 1 && /^\d$/.test(last)) {
      merged[merged.length - 1] = `${last}.${token}`;
    } else {
      merged.push(token);
    }
  }
  return merged.map(renderModelToken).join(" ");
}

export function uniqueProviderID(providers: Provider[]): string {
  let n = providers.length + 1;
  let id = `provider-${n}`;
  const existing = new Set(providers.map((provider) => provider.id));
  while (existing.has(id)) {
    n += 1;
    id = `provider-${n}`;
  }
  return id;
}

export function uniqueMCPName(servers: MCPServer[]): string {
  let n = servers.length + 1;
  let id = `mcp-${n}`;
  const existing = new Set(servers.map((server) => server.id));
  while (existing.has(id)) {
    n += 1;
    id = `mcp-${n}`;
  }
  return id;
}

export function invokeError(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) {
    const message = (err as { message: unknown }).message;
    if (typeof message === "string" && message) return message;
  }
  return String(err);
}

export function resolvedSkillsDirHint(kind: AgentKind): string {
  switch (kind) {
    case "antigravity":
      return "~/.gemini/config/skills";
    case "claude":
      return "~/.claude/skills";
    case "codex":
      return "~/.codex/skills";
    case "gemini":
      return "~/.gemini/skills";
    case "opencode":
      return "~/.config/opencode/skills";
    case "cursor":
      return "~/.cursor/skills";
    case "qwen":
      return "~/.qwen/skills";
    case "pi":
      return "~/.pi/agent/skills";
    case "omp":
      return "~/.omp/agent/skills";
    case "dsh":
      return "~/.dsh/skills";
    case "zcode":
      return "~/.zcode/skills";
    case "grok":
      return "~/.grok/skills";
  }
}

export { EFFORT_LEVELS };
