import { invoke } from "@tauri-apps/api/core";
import { appStore } from "$lib/stores/app.svelte";
import { t } from "$lib/i18n";
import { parseLocalDateEnd, parseLocalDateStart } from "$lib/utils";
import type {
  CurrencyPref,
  PricingOverrideRow,
  PricingRate,
  TodayUsageSummary,
  UsageDashboardData,
  UsageFilter,
  UsageLogPage,
  UsageRangePreset,
  UsageSyncStatus,
} from "./types";

const CURRENCY_SYMBOLS: Record<string, string> = {
  USD: "$",
  CNY: "¥",
  EUR: "€",
  JPY: "¥",
};

export function currencySymbol(code: string): string {
  return CURRENCY_SYMBOLS[code.toUpperCase()] ?? "$";
}

export function formatMoney(value: number, currency = "USD"): string {
  const sym = currencySymbol(currency);
  const abs = Math.abs(value);
  if (currency.toUpperCase() === "JPY") {
    return `${sym}${Math.round(value).toLocaleString()}`;
  }
  if (abs === 0) return `${sym}0.00`;
  if (abs < 0.01) return `${sym}${value.toFixed(4)}`;
  return `${sym}${value.toFixed(2)}`;
}

export function formatCompactTokens(tokens: number): string {
  if (!tokens || tokens <= 0) return "0";
  if (tokens >= 1_000_000_000) return `${(tokens / 1_000_000_000).toFixed(2)}B`;
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(2)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
  return String(tokens);
}

export function formatDurationMs(ms: number): string {
  if (!ms || ms <= 0 || !Number.isFinite(ms)) return "—";
  if (ms < 1000) return `${Math.round(ms)}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(2)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = ((ms % 60_000) / 1000).toFixed(1);
  return `${mins}m ${secs}s`;
}

class UsageStore {
  rangePreset = $state<UsageRangePreset>("30d");
  customStart = $state("");
  customEnd = $state("");
  platform = $state("all");
  model = $state("all");
  workspace = $state("all");

  dashboard = $state<UsageDashboardData | null>(null);
  logs = $state<UsageLogPage | null>(null);
  logsLoading = $state(false);
  logsPage = $state(1);
  logsPageSize = $state(50);
  syncStatus = $state<UsageSyncStatus | null>(null);
  overrides = $state<PricingOverrideRow[]>([]);
  currencyPref = $state<CurrencyPref>({ currency: "USD", rate: 1 });
  syncing = $state(false);
  loading = $state(false);
  error = $state("");
  private syncedOnce = false;

  private buildFilter(): UsageFilter {
    const now = Date.now();
    const dayMs = 86_400_000;
    let startMs: number | null = null;
    let endMs: number | null = null;

    if (this.rangePreset === "today") {
      const startOfDay = new Date();
      startOfDay.setHours(0, 0, 0, 0);
      startMs = startOfDay.getTime();
      endMs = now;
    } else if (this.rangePreset === "7d") {
      startMs = now - 7 * dayMs;
      endMs = now;
    } else if (this.rangePreset === "30d") {
      startMs = now - 30 * dayMs;
      endMs = now;
    } else if (this.rangePreset === "90d") {
      startMs = now - 90 * dayMs;
      endMs = now;
    } else if (this.rangePreset === "custom") {
      if (this.customStart) {
        const s = parseLocalDateStart(this.customStart);
        if (Number.isFinite(s)) startMs = s;
      }
      if (this.customEnd) {
        const e = parseLocalDateEnd(this.customEnd);
        if (Number.isFinite(e)) endMs = e;
      }
    }

    return {
      startMs,
      endMs,
      platform: this.platform,
      model: this.model,
      workspace: this.workspace,
    };
  }

  async syncInBackground(): Promise<void> {
    if (this.syncedOnce || this.syncing) return;
    this.syncedOnce = true;
    await this.sync(false, true);
  }

  async sync(force = false, silent = false): Promise<void> {
    if (this.syncing) return;
    this.syncing = true;
    if (!silent) this.error = "";
    try {
      const status = await invoke<UsageSyncStatus>("usage_sync", { force });
      this.syncStatus = status;
      await Promise.all([this.refreshTodayBadge(), this.loadDashboard()]);
    } catch (err) {
      if (!silent) this.error = String(err);
    } finally {
      this.syncing = false;
    }
  }

  async refreshTodayBadge(): Promise<void> {
    try {
      const summary = await invoke<TodayUsageSummary>("usage_today_summary");
      if (summary.hasPricedEvents) {
        appStore.todaySpendLabel = formatMoney(summary.totalCost, summary.currency);
      } else if (summary.totalTokens > 0) {
        appStore.todaySpendLabel = formatCompactTokens(summary.totalTokens);
      } else {
        appStore.todaySpendLabel = "";
      }
    } catch {
      // ignore badge refresh error
    }
  }

  async loadDashboard(): Promise<void> {
    this.loading = true;
    this.error = "";
    try {
      const data = await invoke<UsageDashboardData>("usage_query_dashboard", {
        filter: this.buildFilter(),
      });
      this.dashboard = data;
      this.currencyPref = { currency: data.currency, rate: data.exchangeRate };
      if (this.logs !== null) {
        void this.loadLogs(1);
      }
    } catch (err) {
      this.error = String(err);
    } finally {
      this.loading = false;
    }
  }

  async loadLogs(page = this.logsPage): Promise<void> {
    this.logsLoading = true;
    const targetPage = Math.max(1, page);
    this.logsPage = targetPage;
    try {
      const res = await invoke<UsageLogPage>("usage_query_logs", {
        filter: this.buildFilter(),
        page: targetPage,
        pageSize: this.logsPageSize,
      });
      this.logs = res;
    } catch (err) {
      this.error = String(err);
    } finally {
      this.logsLoading = false;
    }
  }

  async loadPricingOverrides(): Promise<void> {
    try {
      const [rows, pref] = await Promise.all([
        invoke<PricingOverrideRow[]>("usage_list_pricing_overrides"),
        invoke<CurrencyPref>("usage_get_prefs"),
      ]);
      this.overrides = rows;
      this.currencyPref = pref;
    } catch (err) {
      this.error = String(err);
    }
  }

  async savePricingOverride(model: string, rate: PricingRate): Promise<void> {
    await invoke("usage_save_pricing_override", { model, rate });
    await Promise.all([this.loadPricingOverrides(), this.loadDashboard(), this.refreshTodayBadge()]);
  }

  async deletePricingOverride(model: string): Promise<void> {
    await invoke("usage_delete_pricing_override", { model });
    await Promise.all([this.loadPricingOverrides(), this.loadDashboard(), this.refreshTodayBadge()]);
  }

  async setCurrency(currency: string, rate?: number): Promise<void> {
    const pref = await invoke<CurrencyPref>("usage_set_prefs", { currency, rate: rate ?? null });
    this.currencyPref = pref;
    await Promise.all([this.loadDashboard(), this.refreshTodayBadge()]);
  }

  resetFilters(): void {
    this.rangePreset = "30d";
    this.customStart = "";
    this.customEnd = "";
    this.platform = "all";
    this.model = "all";
    this.workspace = "all";
    void this.loadDashboard();
  }

  todayLabel(): string {
    return t("usage_today_prefix");
  }
}

export const usageStore = new UsageStore();
