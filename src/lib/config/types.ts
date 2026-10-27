export type AgentKind = "opencode" | "dsh" | "zcode" | "grok" | "pi" | "omp";

export const AGENT_KINDS: AgentKind[] = ["opencode", "dsh", "zcode", "grok", "pi", "omp"];

export type APIKind = "responses" | "completions" | "anthropic" | "google";

export const API_KINDS: APIKind[] = ["responses", "completions", "anthropic", "google"];

export type MCPTransport = "stdio" | "http" | "sse";

export const MCP_TRANSPORTS: MCPTransport[] = ["stdio", "http", "sse"];

export type EffortLevel = "low" | "medium" | "high" | "xhigh" | "max";

export const EFFORT_LEVELS: EffortLevel[] = ["low", "medium", "high", "xhigh", "max"];

export const INPUT_KINDS = ["text", "image", "pdf"] as const;

export type SidebarItem = "agents" | "providers" | "library";

export const SIDEBAR_ITEMS: SidebarItem[] = ["agents", "providers", "library"];

export type ModelEntry = {
  id: string;
  requestId?: string;
  name: string;
  context?: number;
  output?: number;
  input: string[];
  efforts?: string[];
  toolCall: boolean;
};

export type Provider = {
  id: string;
  name: string;
  baseURL: string;
  api: APIKind;
  catalogURL: string;
  apiKey: string;
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

export type Library = {
  instructions: string;
  mcp: MCPServer[];
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
  library: Library;
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
