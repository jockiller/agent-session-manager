import { t, i18n } from "./i18n";
import { getPlatformBadgeClass } from "./platforms/registry";

export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0 || !Number.isFinite(bytes)) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function normalizeTimestamp(ts: number | null | undefined): number {
  if (!ts || !Number.isFinite(ts) || ts <= 0) return 0;
  return ts < 100_000_000_000 ? ts * 1000 : ts;
}

export function formatTime(ts: number): string {
  const nts = normalizeTimestamp(ts);
  if (!nts) return "-";
  const d = new Date(nts);
  return d.toLocaleString(i18n.locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatTimeAgo(ts: number): string {
  const nts = normalizeTimestamp(ts);
  if (!nts) return "-";
  const now = Date.now();
  const diffSec = Math.floor((now - nts) / 1000);
  if (diffSec < 0) return t("time_just_now");
  if (diffSec < 60) return t("time_just_now");
  if (diffSec < 3600) return t("time_m_ago", { n: Math.floor(diffSec / 60) });
  if (diffSec < 86400) return t("time_h_ago", { n: Math.floor(diffSec / 3600) });
  const days = Math.floor(diffSec / 86400);
  if (days < 30) return t("time_d_ago", { n: days });
  return t("time_mo_ago", { n: Math.floor(days / 30) });
}

export function getPlatformBadge(platform: string): string {
  return getPlatformBadgeClass(platform);
}

export function toLocalDateString(d: Date): string {
  const year = d.getFullYear();
  const month = String(d.getMonth() + 1).padStart(2, "0");
  const date = String(d.getDate()).padStart(2, "0");
  return `${year}-${month}-${date}`;
}

export function parseLocalDateStart(dateStr: string): number {
  if (!dateStr) return NaN;
  const parts = dateStr.split("-").map(Number);
  if (parts.length === 3 && parts.every(Number.isFinite)) {
    return new Date(parts[0], parts[1] - 1, parts[2], 0, 0, 0, 0).getTime();
  }
  return NaN;
}

export function parseLocalDateEnd(dateStr: string): number {
  if (!dateStr) return NaN;
  const parts = dateStr.split("-").map(Number);
  if (parts.length === 3 && parts.every(Number.isFinite)) {
    return new Date(parts[0], parts[1] - 1, parts[2], 23, 59, 59, 999).getTime();
  }
  return NaN;
}
