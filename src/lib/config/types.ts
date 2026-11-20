export type AgentKind =
  | "opencode"
  | "dsh"
  | "zcode"
  | "grok"
  | "pi"
  | "omp"
  | "antigravity"
  | "claude"
  | "codex"
  | "gemini"
  | "cursor"
  | "qwen";

export const PROVIDER_AGENT_KINDS: AgentKind[] = ["opencode", "dsh", "zcode", "grok", "pi", "omp"];

export const AGENT_KINDS: AgentKind[] = [
  "opencode",
  "dsh",
  "zcode",
  "grok",
  "pi",
  "omp",
  "antigravity",
  "claude",
  "codex",
  "gemini",
  "cursor",
  "qwen",
];

export type APIKind = "responses" | "completions" | "anthropic" | "google";

export const API_KINDS: APIKind[] = ["responses", "completions", "anthropic", "google"];

export type MCPTransport = "stdio" | "http" | "sse";

export const MCP_TRANSPORTS: MCPTransport[] = ["stdio", "http", "sse"];

export type EffortLevel = "low" | "medium" | "high" | "xhigh" | "max";

export const EFFORT_LEVELS: EffortLevel[] = ["low", "medium", "high", "xhigh", "max"];

export const INPUT_KINDS = ["text", "image", "pdf"] as const;

export type ModelKind = "chat" | "image";

export const MODEL_KINDS: ModelKind[] = ["chat", "image"];

export type SidebarItem = "agents" | "providers" | "library";

export const SIDEBAR_ITEMS: SidebarItem[] = ["agents", "providers", "library"];

export type ModelCost = {
  input?: number;
  output?: number;
  cacheRead?: number;
  cacheWrite?: number;
};

export type ModelEntry = {
  id: string;
  requestId?: string;
  name: string;
  /** OpenCode model type; chat when omitted. */
  type?: ModelKind;
  context?: number;
  output?: number;
  input: string[];
  efforts?: string[];
  cost?: ModelCost;
  toolCall: boolean;
};

export type Provider = {
  id: string;
  name: string;
  baseURL: string;
  api: APIKind;
  catalogURL: string;
  apiKey: string;
  enabled: boolean;
  models: ModelEntry[];
  compatBaseURL?: string;
};

export type MCPServer = {
  id: string;
  transport: MCPTransport;
  command: string;
  args: string[];
  url: string;
  env: Record<string, string>;
};

export type DiscoveredDiskMcp = {
  server: MCPServer;
  agents: string[];
  inLibrary: boolean;
};

export type Library = {
  instructions: string;
  mcp: MCPServer[];
};

export type InstructionPreset = {
  id: string;
  name: string;
  description: string;
  tags: string[];
  content: string;
  updatedAt: string;
};

export type InstructionBinding = {
  targetId: string;
  enabled: boolean;
  presetId: string;
  customPath?: string;
  lastSyncedHash?: string;
  lastSyncedAt?: string;
};

export type CustomInstructionTarget = {
  id: string;
  name: string;
  path: string;
  kind: string;
};

export type InstructionsState = {
  presets: InstructionPreset[];
  bindings: InstructionBinding[];
  customTargets: CustomInstructionTarget[];
};

export type InstructionSyncStatus =
  | "synced"
  | "pending"
  | "conflict"
  | "disabled"
  | "uninstalled";

export type TargetScanRow = {
  targetId: string;
  name: string;
  path: string;
  fileName: string;
  isBuiltin: boolean;
  kind: string;
  installed: boolean;
  fileExists: boolean;
  diskHash?: string;
  diskPreview: string;
  status: InstructionSyncStatus;
  enabled: boolean;
  presetId: string;
  lastSyncedHash?: string;
  lastSyncedAt?: string;
};

export type InstructionBackupEntry = {
  stamp: string;
  backupPath: string;
  sizeBytes: number;
  modifiedMs: number;
};

export type InstructionSyncOutcome = {
  written: string[];
  backups: string[];
  updatedBindings: InstructionBinding[];
};

export type AgentSettings = {
  model: string;
  effort: string;
  /** Per-model thinking level. Agents that cannot write it ignore the map. */
  effortByModel: Record<string, string>;
  smallModel: string;
  pickedModelIDs: string[];
  instructionOn: boolean;
  mcpIDs: string[];
  profile: string;
};

export type AppDocument = {
  version: number;
  providers: Provider[];
  retiredProviders?: Provider[];
  library: Library;
  instructions: InstructionsState;
  agents: Record<string, AgentSettings>;
  appliedAt: Record<string, string>;
};

export type ModelChoice = {
  ref: string;
  title: string;
};

export type DetectEntry = {
  kind: string;
  installed: boolean;
};

export type ApplyResult = {
  document: AppDocument;
  written: string[];
  backups: string[];
};

export type StoreToast = {
  id: number;
  title: string;
  detail: string;
  isError: boolean;
  revealedPath: string;
};

export type SkillFileInfo = {
  relPath: string;
  sizeBytes: number;
};

export type SkillSourceMeta = {
  kind: string;
  repo: string;
  branch: string;
  subpath: string;
  url: string;
  commit: string;
  fetchedAt: string;
};

export type SkillEntry = {
  id: string;
  name: string;
  description: string;
  path: string;
  skillMdPath: string;
  content: string;
  extraFiles: SkillFileInfo[];
  linkedAgents: string[];
  updatedMs: number;
  source?: SkillSourceMeta | null;
};

export type SkillRepo = {
  id: string;
  label: string;
  url: string;
  branch: string;
  enabled: boolean;
  builtin: boolean;
};

export type RemoteSkillCandidate = {
  id: string;
  name: string;
  description: string;
  repoId: string;
  repoLabel: string;
  repoUrl: string;
  branch: string;
  subpath: string;
  commit: string;
  sourceKind: string;
  cacheDir: string;
  skillMdPreview: string;
  extraFiles: string[];
  hasScripts: boolean;
  inLibrary: boolean;
};

export type SkillDiscoverOutcome = {
  candidates: RemoteSkillCandidate[];
  warnings: string[];
};

export type SkillUpdateOutcome = {
  entry: SkillEntry;
  updated: boolean;
  previousCommit: string;
  currentCommit: string;
  backupPath?: string | null;
};

export type DiscoveredDiskSkill = {
  id: string;
  name: string;
  description: string;
  sourcePath: string;
  sourceAgents: string[];
  inLibrary: boolean;
  fileCount: number;
};

export type SkillSyncOutcome = {
  linked: string[];
  unlinked: string[];
  backups: string[];
};
