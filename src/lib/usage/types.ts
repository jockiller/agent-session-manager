export interface UsageSyncStatus {
  scannedSources: number;
  updatedSources: number;
  prunedSources: number;
  totalEvents: number;
  durationMs: number;
  syncedAtMs: number;
}

export interface UsageFilter {
  startMs?: number | null;
  endMs?: number | null;
  platform?: string;
  model?: string;
  workspace?: string;
  sessionId?: string | null;
}

export interface UsageKpis {
  totalCost: number;
  cacheSavings: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  cacheHitRate: number;
  sessionCount: number;
  eventCount: number;
}

export interface TimeseriesPoint {
  bucket: string;
  cost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface BreakdownRow {
  key: string;
  label: string;
  cost: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  totalTokens: number;
  cacheHitRate: number;
  sessionCount: number;
  eventCount: number;
  priced: boolean;
}

export interface TopSessionRow {
  sessionId: string;
  platform: string;
  workspace: string;
  model: string;
  cost: number;
  totalTokens: number;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  lastTsMs: number;
  granularity: string;
}

export interface SessionUsageDetail {
  sessionId: string;
  platform: string;
  cost: number;
  cacheSavings: number;
  currency: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  cacheHitRate: number;
  models: string[];
  eventCount: number;
  granularity: string;
  priced: boolean;
}

export interface UsageDashboardData {
  kpis: UsageKpis;
  hourly: boolean;
  timeseries: TimeseriesPoint[];
  byModel: BreakdownRow[];
  byPlatform: BreakdownRow[];
  byWorkspace: BreakdownRow[];
  topSessions: TopSessionRow[];
  unpricedModels: string[];
  availableModels: string[];
  availableWorkspaces: string[];
  currency: string;
  exchangeRate: number;
}

export interface TodayUsageSummary {
  dayKey: string;
  totalCost: number;
  totalTokens: number;
  currency: string;
  hasPricedEvents: boolean;
}

export interface UsageLogRow {
  id: number;
  tsMs: number;
  platform: string;
  sessionId: string;
  workspace: string;
  modelRaw: string;
  modelNorm: string;
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  cacheHitRate: number;
  cost: number;
  priced: boolean;
  durationMs: number;
  ttftMs: number;
  granularity: string;
}

export interface UsageLogPage {
  items: UsageLogRow[];
  total: number;
  page: number;
  pageSize: number;
  currency: string;
}

export interface PricingRate {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}

export interface PricingOverrideRow {
  modelNorm: string;
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
  updatedAt: number;
}

export interface CurrencyPref {
  currency: string;
  rate: number;
}

export type UsageRangePreset = "today" | "7d" | "30d" | "90d" | "all" | "custom";
