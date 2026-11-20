import type { SessionSummary } from "$lib/types";

export interface PlatformMeta {
  id: string;
  label: string;
  dot: string;
  badge: string;
  tint: string;
  hasConfigExport: boolean;
  hasUsage: boolean;
  instructionTargetId?: string;
  resumeCli: (session: SessionSummary) => string;
}

export const PLATFORMS: PlatformMeta[] = [
  {
    id: "claude",
    label: "Claude Code",
    dot: "bg-amber-500",
    badge: "bg-amber-50 text-amber-800 border-amber-200/90 dark:bg-amber-950/60 dark:text-amber-300 dark:border-amber-800/70",
    tint: "rgb(217, 119, 6)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "claude",
    resumeCli: (s) => `claude --resume "${s.id}"`,
  },
  {
    id: "codex",
    label: "Codex",
    dot: "bg-emerald-500",
    badge: "bg-emerald-50 text-emerald-800 border-emerald-200/90 dark:bg-emerald-950/60 dark:text-emerald-300 dark:border-emerald-800/70",
    tint: "rgb(16, 185, 129)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "codex",
    resumeCli: (s) => `codex resume "${s.id}"`,
  },
  {
    id: "opencode",
    label: "OpenCode",
    dot: "bg-blue-500",
    badge: "bg-blue-50 text-blue-800 border-blue-200/90 dark:bg-blue-950/60 dark:text-blue-300 dark:border-blue-800/70",
    tint: "rgb(51, 122, 235)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "opencode",
    resumeCli: (s) => `opencode -s "${s.id}"`,
  },
  {
    id: "antigravity",
    label: "Antigravity",
    dot: "bg-sky-500",
    badge: "bg-sky-50 text-sky-800 border-sky-200/90 dark:bg-sky-950/60 dark:text-sky-300 dark:border-sky-800/70",
    tint: "rgb(14, 165, 233)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "antigravity",
    resumeCli: (s) => `agy --conversation "${s.id}"`,
  },
  {
    id: "gemini",
    label: "Gemini CLI",
    dot: "bg-sky-500",
    badge: "bg-sky-50 text-sky-800 border-sky-200/90 dark:bg-sky-950/60 dark:text-sky-300 dark:border-sky-800/70",
    tint: "rgb(2, 132, 199)",
    hasConfigExport: true,
    hasUsage: false,
    instructionTargetId: "gemini",
    resumeCli: (s) => `gemini --resume "${s.id}"`,
  },
  {
    id: "zcode",
    label: "ZCode",
    dot: "bg-teal-500",
    badge: "bg-teal-50 text-teal-800 border-teal-200/90 dark:bg-teal-950/60 dark:text-teal-300 dark:border-teal-800/70",
    tint: "rgb(143, 84, 230)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "zcode",
    resumeCli: (s) => `zcode --resume "${s.id}"`,
  },
  {
    id: "grok",
    label: "Grok",
    dot: "bg-slate-500",
    badge: "bg-slate-100 text-slate-800 border-slate-300 dark:bg-slate-800 dark:text-slate-200 dark:border-slate-700",
    tint: "rgb(41, 158, 148)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "grok",
    resumeCli: (s) => `grok --resume "${s.id}"`,
  },
  {
    id: "dsh",
    label: "DSH",
    dot: "bg-purple-500",
    badge: "bg-purple-50 text-purple-800 border-purple-200/90 dark:bg-purple-950/60 dark:text-purple-300 dark:border-purple-800/70",
    tint: "rgb(230, 122, 41)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "dsh",
    resumeCli: (s) => `dsh-tui --resume "${s.id}"`,
  },
  {
    id: "pi",
    label: "Pi",
    dot: "bg-rose-500",
    badge: "bg-rose-50 text-rose-800 border-rose-200/90 dark:bg-rose-950/60 dark:text-rose-300 dark:border-rose-800/70",
    tint: "rgb(89, 184, 112)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "pi",
    resumeCli: (s) => `pi --session "${s.id}"`,
  },
  {
    id: "omp",
    label: "Oh My Pi",
    dot: "bg-pink-500",
    badge: "bg-pink-50 text-pink-800 border-pink-200/90 dark:bg-pink-950/60 dark:text-pink-300 dark:border-pink-800/70",
    tint: "rgb(219, 107, 153)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "omp",
    resumeCli: (s) => `omp --resume "${s.id}"`,
  },
  {
    id: "openclaude",
    label: "OpenClaude",
    dot: "bg-orange-500",
    badge: "bg-orange-50 text-orange-800 border-orange-200/90 dark:bg-orange-950/60 dark:text-orange-300 dark:border-orange-800/70",
    tint: "rgb(234, 88, 12)",
    hasConfigExport: false,
    hasUsage: true,
    instructionTargetId: "openclaude",
    resumeCli: (s) => `openclaude --resume "${s.id}"`,
  },
  {
    id: "openclaw",
    label: "OpenClaw",
    dot: "bg-red-500",
    badge: "bg-red-50 text-red-800 border-red-200/90 dark:bg-red-950/60 dark:text-red-300 dark:border-red-800/70",
    tint: "rgb(220, 38, 38)",
    hasConfigExport: false,
    hasUsage: false,
    instructionTargetId: "openclaw",
    resumeCli: (s) => `openclaw session resume "${s.id}"`,
  },
  {
    id: "hermes",
    label: "Hermes",
    dot: "bg-orange-500",
    badge: "bg-orange-50 text-orange-800 border-orange-200/90 dark:bg-orange-950/60 dark:text-orange-300 dark:border-orange-800/70",
    tint: "rgb(249, 115, 22)",
    hasConfigExport: false,
    hasUsage: false,
    instructionTargetId: "hermes",
    resumeCli: (s) => `hermes session resume "${s.id}"`,
  },
  {
    id: "qwen",
    label: "Qwen",
    dot: "bg-blue-500",
    badge: "bg-blue-50 text-blue-800 border-blue-200/90 dark:bg-blue-950/60 dark:text-blue-300 dark:border-blue-800/70",
    tint: "rgb(37, 99, 235)",
    hasConfigExport: true,
    hasUsage: true,
    instructionTargetId: "qwen",
    resumeCli: (s) => `qwen --resume "${s.id}"`,
  },
  {
    id: "kimi",
    label: "Kimi Code",
    dot: "bg-teal-500",
    badge: "bg-teal-50 text-teal-800 border-teal-200/90 dark:bg-teal-950/60 dark:text-teal-300 dark:border-teal-800/70",
    tint: "rgb(20, 184, 166)",
    hasConfigExport: false,
    hasUsage: false,
    resumeCli: (s) => `kimi-code --resume "${s.id}"`,
  },
  {
    id: "deepseek",
    label: "DeepSeek CLI",
    dot: "bg-indigo-500",
    badge: "bg-indigo-50 text-indigo-800 border-indigo-200/90 dark:bg-indigo-950/60 dark:text-indigo-300 dark:border-indigo-800/70",
    tint: "rgb(99, 102, 241)",
    hasConfigExport: false,
    hasUsage: false,
    resumeCli: (s) => `deepseek --resume "${s.id}"`,
  },
  {
    id: "codewiz",
    label: "CodeWiz",
    dot: "bg-purple-500",
    badge: "bg-purple-50 text-purple-800 border-purple-200/90 dark:bg-purple-950/60 dark:text-purple-300 dark:border-purple-800/70",
    tint: "rgb(168, 85, 247)",
    hasConfigExport: false,
    hasUsage: true,
    resumeCli: (s) => `codewiz session resume "${s.id}"`,
  },
  {
    id: "cursor",
    label: "Cursor",
    dot: "bg-indigo-500",
    badge: "bg-indigo-50 text-indigo-800 border-indigo-200/90 dark:bg-indigo-950/60 dark:text-indigo-300 dark:border-indigo-800/70",
    tint: "rgb(79, 70, 229)",
    hasConfigExport: true,
    hasUsage: false,
    resumeCli: (s) => (s.cwd || s.dirname ? "cursor ." : "cursor"),
  },
  {
    id: "trae",
    label: "Trae",
    dot: "bg-violet-500",
    badge: "bg-violet-50 text-violet-800 border-violet-200/90 dark:bg-violet-950/60 dark:text-violet-300 dark:border-violet-800/70",
    tint: "rgb(139, 92, 246)",
    hasConfigExport: false,
    hasUsage: false,
    resumeCli: () => "trae",
  },
  {
    id: "qoder",
    label: "Qoder",
    dot: "bg-cyan-500",
    badge: "bg-cyan-50 text-cyan-800 border-cyan-200/90 dark:bg-cyan-950/60 dark:text-cyan-300 dark:border-cyan-800/70",
    tint: "rgb(6, 182, 212)",
    hasConfigExport: false,
    hasUsage: false,
    resumeCli: (s) => `qoder resume "${s.id}"`,
  },
  {
    id: "codebuddy",
    label: "CodeBuddy",
    dot: "bg-lime-500",
    badge: "bg-lime-50 text-lime-800 border-lime-200/90 dark:bg-lime-950/60 dark:text-lime-300 dark:border-lime-800/70",
    tint: "rgb(132, 204, 22)",
    hasConfigExport: false,
    hasUsage: true,
    resumeCli: (s) => `codebuddy --resume "${s.id}"`,
  },
  {
    id: "workbuddy",
    label: "WorkBuddy",
    dot: "bg-emerald-500",
    badge: "bg-emerald-50 text-emerald-800 border-emerald-200/90 dark:bg-emerald-950/60 dark:text-emerald-300 dark:border-emerald-800/70",
    tint: "rgb(5, 150, 105)",
    hasConfigExport: false,
    hasUsage: true,
    resumeCli: (s) => `workbuddy --session "${s.id}"`,
  },
];

const PLATFORM_MAP = new Map<string, PlatformMeta>(PLATFORMS.map((p) => [p.id, p]));

export function getPlatformMeta(id: string): PlatformMeta | undefined {
  return PLATFORM_MAP.get(id.toLowerCase());
}

export function getPlatformLabel(id: string): string {
  return PLATFORM_MAP.get(id.toLowerCase())?.label ?? id;
}

export function getPlatformBadgeClass(id: string): string {
  return (
    PLATFORM_MAP.get(id.toLowerCase())?.badge ??
    "bg-slate-100 text-slate-800 border-slate-200 dark:bg-slate-800 dark:text-slate-200 dark:border-slate-700"
  );
}

export function buildResumeCommand(session: SessionSummary): string {
  const plat = session.platform.toLowerCase();
  const meta = PLATFORM_MAP.get(plat);
  const cliCmd = meta ? meta.resumeCli(session) : `${plat} --resume "${session.id}"`;
  const dir = session.cwd || session.dirname;
  if (dir && dir.trim() && dir !== "/") {
    return `cd "${dir}" && ${cliCmd}`;
  }
  return cliCmd;
}

export type ResumeAction =
  | { kind: "terminal"; command: string }
  | { kind: "app"; appName: string; appLabel: string; targetPath?: string };

export function getResumeAction(session: SessionSummary): ResumeAction {
  const plat = session.platform.toLowerCase();
  const flavor = (session.flavor || "").toLowerCase();
  if (plat === "antigravity" && flavor !== "agy-cli") {
    const dir = session.cwd || session.dirname;
    const targetPath = dir && dir.trim() && dir !== "/" && dir.startsWith("/") ? dir.trim() : undefined;
    return {
      kind: "app",
      appName: "Antigravity",
      appLabel: "Antigravity",
      targetPath,
    };
  }
  return {
    kind: "terminal",
    command: buildResumeCommand(session),
  };
}
