export type NavTab =
  | "sessions"
  | "usage"
  | "logs"
  | "instructions"
  | "agents"
  | "providers"
  | "mcp"
  | "skills"
  | "platforms"
  | "settings";

export type ThemeMode = "system" | "light" | "dark";
export type DensityMode = "comfortable" | "compact";
export type TerminalApp =
  | "Terminal"
  | "iTerm"
  | "Ghostty"
  | "Warp"
  | "Alacritty"
  | "Kitty";

export const TERMINAL_OPTIONS: { id: TerminalApp; label: string }[] = [
  { id: "Terminal", label: "macOS Terminal" },
  { id: "iTerm", label: "iTerm2" },
  { id: "Ghostty", label: "Ghostty" },
  { id: "Warp", label: "Warp" },
  { id: "Alacritty", label: "Alacritty" },
  { id: "Kitty", label: "Kitty" },
];

const STORAGE_KEYS = {
  theme: "asm_theme_mode",
  density: "asm_density_mode",
  terminal: "asm_preferred_terminal",
  hiddenPlatforms: "asm_hidden_platforms",
  sidebarCollapsed: "asm_sidebar_collapsed",
  activeNav: "asm_active_nav",
} as const;

const VALID_NAVS = new Set<NavTab>([
  "sessions",
  "usage",
  "logs",
  "instructions",
  "agents",
  "providers",
  "mcp",
  "skills",
  "platforms",
  "settings",
]);

function readStorage<T>(key: string, fallback: T): T {
  if (typeof window === "undefined") return fallback;
  try {
    const raw = localStorage.getItem(key);
    if (raw === null) return fallback;
    return JSON.parse(raw) as T;
  } catch {
    return fallback;
  }
}

function readString(key: string, fallback: string): string {
  if (typeof window === "undefined") return fallback;
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function writeStorage(key: string, value: string): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(key, value);
  } catch {
    // ignore storage quota errors
  }
}

class AppStore {
  activeNav = $state<NavTab>("sessions");
  sidebarCollapsed = $state<boolean>(false);
  theme = $state<ThemeMode>("system");
  density = $state<DensityMode>("comfortable");
  preferredTerminal = $state<TerminalApp>("Terminal");
  hiddenPlatforms = $state<string[]>([]);
  todaySpendLabel = $state<string>("");
  pendingJumpSessionId = $state<string | null>(null);
  isDarkEffective = $state<boolean>(false);
  private mediaQuery: MediaQueryList | null = null;

  constructor() {
    if (typeof window !== "undefined") {
      const savedNav = readString(STORAGE_KEYS.activeNav, "sessions") as NavTab;
      if (VALID_NAVS.has(savedNav)) this.activeNav = savedNav;

      const savedTheme = readString(STORAGE_KEYS.theme, "system") as ThemeMode;
      if (savedTheme === "system" || savedTheme === "light" || savedTheme === "dark") {
        this.theme = savedTheme;
      }

      const savedDensity = readString(STORAGE_KEYS.density, "comfortable") as DensityMode;
      if (savedDensity === "comfortable" || savedDensity === "compact") {
        this.density = savedDensity;
      }

      const savedTerm = readString(STORAGE_KEYS.terminal, "Terminal") as TerminalApp;
      if (TERMINAL_OPTIONS.some((t) => t.id === savedTerm)) {
        this.preferredTerminal = savedTerm;
      }

      const hidden = readStorage<unknown>(STORAGE_KEYS.hiddenPlatforms, []);
      if (Array.isArray(hidden)) {
        this.hiddenPlatforms = hidden.filter((item): item is string => typeof item === "string");
      }

      this.sidebarCollapsed = readStorage<boolean>(STORAGE_KEYS.sidebarCollapsed, false);

      this.mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
      this.mediaQuery.addEventListener("change", () => this.applyDomPreferences());
      this.applyDomPreferences();
    }
  }

  applyDomPreferences(): void {
    if (typeof document === "undefined") return;
    const systemDark = this.mediaQuery?.matches ?? false;
    const dark = this.theme === "dark" || (this.theme === "system" && systemDark);
    this.isDarkEffective = dark;
    const root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.dataset.density = this.density;
  }

  setNav(tab: NavTab): void {
    this.activeNav = tab;
    writeStorage(STORAGE_KEYS.activeNav, tab);
  }

  toggleSidebar(): void {
    this.sidebarCollapsed = !this.sidebarCollapsed;
    writeStorage(STORAGE_KEYS.sidebarCollapsed, JSON.stringify(this.sidebarCollapsed));
  }

  setSidebarCollapsed(collapsed: boolean): void {
    this.sidebarCollapsed = collapsed;
    writeStorage(STORAGE_KEYS.sidebarCollapsed, JSON.stringify(collapsed));
  }

  setTheme(mode: ThemeMode): void {
    this.theme = mode;
    writeStorage(STORAGE_KEYS.theme, mode);
    this.applyDomPreferences();
  }

  setDensity(mode: DensityMode): void {
    this.density = mode;
    writeStorage(STORAGE_KEYS.density, mode);
    this.applyDomPreferences();
  }

  setTerminal(term: TerminalApp): void {
    this.preferredTerminal = term;
    writeStorage(STORAGE_KEYS.terminal, term);
  }

  isPlatformHidden(id: string): boolean {
    return this.hiddenPlatforms.includes(id);
  }

  togglePlatformVisibility(id: string): void {
    const next = new Set(this.hiddenPlatforms);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    this.hiddenPlatforms = Array.from(next);
    writeStorage(STORAGE_KEYS.hiddenPlatforms, JSON.stringify(this.hiddenPlatforms));
  }

  showAllPlatforms(): void {
    this.hiddenPlatforms = [];
    writeStorage(STORAGE_KEYS.hiddenPlatforms, "[]");
  }

  jumpToSession(sessionId: string): void {
    this.pendingJumpSessionId = sessionId;
    this.setNav("sessions");
  }
}

export const appStore = new AppStore();
