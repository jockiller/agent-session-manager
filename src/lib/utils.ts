import { t, i18n } from "./i18n";

export function formatBytes(bytes: number | null | undefined): string {
  if (!bytes || bytes <= 0 || !Number.isFinite(bytes)) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function formatTime(ts: number): string {
  if (!ts) return "-";
  const d = new Date(ts);
  return d.toLocaleString(i18n.locale, {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatTimeAgo(ts: number): string {
  if (!ts) return "-";
  const now = Date.now();
  const diffSec = Math.floor((now - ts) / 1000);
  if (diffSec < 60) return t("time_just_now");
  if (diffSec < 3600) return t("time_m_ago", { n: Math.floor(diffSec / 60) });
  if (diffSec < 86400) return t("time_h_ago", { n: Math.floor(diffSec / 3600) });
  const days = Math.floor(diffSec / 86400);
  if (days < 30) return t("time_d_ago", { n: days });
  return t("time_mo_ago", { n: Math.floor(days / 30) });
}

export function formatTokens(tokens: number | null | undefined): string {
  if (!tokens || tokens <= 0 || !Number.isFinite(tokens)) return "0";
  if (tokens >= 1_000_000_000) {
    return `${(tokens / 1_000_000_000).toFixed(2)}B`;
  }
  if (tokens >= 1_000_000) {
    return `${(tokens / 1_000_000).toFixed(1)}M`;
  }
  if (tokens >= 1_000) {
    return `${(tokens / 1_000).toFixed(1)}k`;
  }
  return Math.round(tokens).toLocaleString();
}

export function formatNumber(num: number): string {
  if (num === undefined || num === null) return "0";
  return num.toLocaleString();
}

export function getPlatformBadge(platform: string): string {
  if (!platform) return "bg-slate-100 text-slate-800 border-slate-200";
  switch (platform.toLowerCase()) {
    case "zcode":
      return "bg-teal-50 text-teal-800 border-teal-200/90";
    case "grok":
      return "bg-slate-100 text-slate-800 border-slate-300";
    case "antigravity":
      return "bg-sky-50 text-sky-800 border-sky-200/90";
    case "claude":
      return "bg-amber-50 text-amber-800 border-amber-200/90";
    case "openclaude":
      return "bg-orange-50 text-orange-800 border-orange-200/90";
    case "codex":
      return "bg-emerald-50 text-emerald-800 border-emerald-200/90";
    case "dsh":
      return "bg-purple-50 text-purple-800 border-purple-200/90";
    case "pi":
      return "bg-rose-50 text-rose-800 border-rose-200/90";
    case "omp":
      return "bg-pink-50 text-pink-800 border-pink-200/90";
    case "qwen":
      return "bg-blue-50 text-blue-800 border-blue-200/90";
    case "trae":
      return "bg-violet-50 text-violet-800 border-violet-200/90";
    case "qoder":
      return "bg-cyan-50 text-cyan-800 border-cyan-200/90";
    case "cursor":
      return "bg-indigo-50 text-indigo-800 border-indigo-200/90";
    case "codebuddy":
      return "bg-lime-50 text-lime-800 border-lime-200/90";
    case "workbuddy":
      return "bg-emerald-50 text-emerald-800 border-emerald-200/90";
    case "opencode":
      return "bg-blue-50 text-blue-800 border-blue-200/90";
    case "codewiz":
      return "bg-purple-50 text-purple-800 border-purple-200/90";
    case "kimi":
      return "bg-teal-50 text-teal-800 border-teal-200/90";
    case "gemini":
      return "bg-sky-50 text-sky-800 border-sky-200/90";
    case "deepseek":
      return "bg-indigo-50 text-indigo-800 border-indigo-200/90";
    case "openclaw":
      return "bg-red-50 text-red-800 border-red-200/90";
    case "hermes":
      return "bg-orange-50 text-orange-800 border-orange-200/90";
    default:
      return "bg-slate-100 text-slate-800 border-slate-200";
  }
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
