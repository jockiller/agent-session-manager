<script lang="ts">
  import {
    MessageSquareText,
    BarChart3,
    ScrollText,
    FileCode2,
    Bot,
    Server,
    Puzzle,
    Sparkles,
    Layers,
    Settings,
    PanelLeftClose,
    PanelLeftOpen,
  } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { appStore, type NavTab } from "$lib/stores/app.svelte";
  import { configStore } from "$lib/config/store.svelte";
  import { skillsStore } from "$lib/stores/skills.svelte";

  let {
    runningCount = 0,
    totalSessions = 0,
  }: {
    runningCount?: number;
    totalSessions?: number;
  } = $props();

  let collapsed = $derived(appStore.sidebarCollapsed);

  interface NavItem {
    id: NavTab;
    labelKey: string;
    shortcut: string;
    icon: typeof MessageSquareText;
    group: "primary" | "config" | "footer";
  }

  const ITEMS: NavItem[] = [
    { id: "sessions", labelKey: "nav_sessions", shortcut: "⌘1", icon: MessageSquareText, group: "primary" },
    { id: "usage", labelKey: "nav_usage", shortcut: "⌘2", icon: BarChart3, group: "primary" },
    { id: "logs", labelKey: "nav_logs", shortcut: "⌘3", icon: ScrollText, group: "primary" },
    { id: "instructions", labelKey: "nav_instructions", shortcut: "⌘4", icon: FileCode2, group: "config" },
    { id: "agents", labelKey: "nav_agents", shortcut: "⌘5", icon: Bot, group: "config" },
    { id: "providers", labelKey: "nav_providers", shortcut: "⌘6", icon: Server, group: "config" },
    { id: "mcp", labelKey: "nav_mcp", shortcut: "⌘7", icon: Puzzle, group: "config" },
    { id: "skills", labelKey: "nav_skills", shortcut: "⌘8", icon: Sparkles, group: "config" },
    { id: "platforms", labelKey: "nav_platforms", shortcut: "", icon: Layers, group: "footer" },
    { id: "settings", labelKey: "nav_settings", shortcut: "", icon: Settings, group: "footer" },
  ];

  const primaryItems = ITEMS.filter((i) => i.group === "primary");
  const configItems = ITEMS.filter((i) => i.group === "config");
  const footerItems = ITEMS.filter((i) => i.group === "footer");

  function handleWindowKeydown(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.shiftKey || e.altKey) return;
    const map: Record<string, NavTab> = {
      "1": "sessions",
      "2": "usage",
      "3": "logs",
      "4": "instructions",
      "5": "agents",
      "6": "providers",
      "7": "mcp",
      "8": "skills",
    };
    const target = map[e.key];
    if (target) {
      e.preventDefault();
      appStore.setNav(target);
    }
  }
</script>

<svelte:window onkeydown={handleWindowKeydown} />

<aside
  class="shrink-0 h-full flex flex-col border-r border-slate-200/90 dark:border-slate-800 bg-slate-100/80 dark:bg-slate-900 select-none transition-[width] duration-150 {collapsed
    ? 'w-14'
    : 'w-[210px]'}"
>
  <!-- Collapse toggle header -->
  <div class="px-2.5 pt-2.5 pb-1.5 flex items-center {collapsed ? 'justify-center' : 'justify-between'}">
    {#if !collapsed}
      <span class="text-[10px] font-bold uppercase tracking-wider text-slate-400 dark:text-slate-500 px-1">
        {t("nav_workspace")}
      </span>
    {/if}
    <button
      type="button"
      onclick={() => appStore.toggleSidebar()}
      class="p-1.5 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-200/60 dark:hover:bg-slate-800 transition cursor-pointer"
      title={collapsed ? t("nav_expand_sidebar") : t("nav_collapse_sidebar")}
    >
      {#if collapsed}
        <PanelLeftOpen class="h-3.5 w-3.5" />
      {:else}
        <PanelLeftClose class="h-3.5 w-3.5" />
      {/if}
    </button>
  </div>

  <!-- Primary section -->
  <div class="px-2 space-y-0.5">
    {#each primaryItems as item (item.id)}
      {@const Icon = item.icon}
      {@const active = appStore.activeNav === item.id}
      <button
        type="button"
        onclick={() => appStore.setNav(item.id)}
        title={collapsed ? `${t(item.labelKey)} (${item.shortcut})` : item.shortcut}
        class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-xs font-medium transition cursor-pointer {collapsed
          ? 'justify-center px-0'
          : ''} {active
          ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-semibold shadow-2xs border border-slate-200/90 dark:border-slate-700'
          : 'text-slate-600 dark:text-slate-400 hover:bg-slate-200/50 dark:hover:bg-slate-800/60 hover:text-slate-900 dark:hover:text-slate-100 border border-transparent'}"
      >
        <Icon
          class="h-4 w-4 shrink-0 {active
            ? 'text-claude-600 dark:text-claude-400'
            : 'text-slate-500 dark:text-slate-400'}"
        />
        {#if !collapsed}
          <span class="flex-1 text-left truncate">{t(item.labelKey)}</span>
          {#if item.id === "sessions"}
            {#if runningCount > 0}
              <span class="inline-flex items-center gap-1 px-1.5 py-0.2 rounded-full text-[10px] font-mono font-semibold bg-emerald-50 dark:bg-emerald-950/70 text-emerald-700 dark:text-emerald-300 border border-emerald-200 dark:border-emerald-800">
                <span class="h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse"></span>
                {runningCount}
              </span>
            {:else if totalSessions > 0}
              <span class="text-[10px] font-mono text-slate-400 dark:text-slate-500">{totalSessions}</span>
            {/if}
          {:else if item.id === "usage" && appStore.todaySpendLabel}
            <span class="px-1.5 py-0.2 rounded-md text-[10px] font-mono font-semibold bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 border border-claude-200/80 dark:border-claude-800">
              {appStore.todaySpendLabel}
            </span>
          {/if}
        {/if}
      </button>
    {/each}
  </div>

  <!-- Config & Assets section -->
  <div class="mt-3 pt-2.5 px-2 border-t border-slate-200/70 dark:border-slate-800 space-y-0.5 flex-1 min-h-0 overflow-y-auto">
    {#if !collapsed}
      <div class="px-2 pb-1 text-[10px] font-bold uppercase tracking-wider text-slate-400 dark:text-slate-500">
        {t("nav_config_assets")}
      </div>
    {/if}
    {#each configItems as item (item.id)}
      {@const Icon = item.icon}
      {@const active = appStore.activeNav === item.id}
      {@const showDirtyDot =
        (((item.id === "agents" || item.id === "providers" || item.id === "mcp" || item.id === "instructions") &&
          configStore.isDirty) ||
          (item.id === "skills" && skillsStore.isDirty)) &&
        active}
      <button
        type="button"
        onclick={() => appStore.setNav(item.id)}
        title={collapsed ? `${t(item.labelKey)} (${item.shortcut})` : item.shortcut}
        class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-xs font-medium transition cursor-pointer {collapsed
          ? 'justify-center px-0'
          : ''} {active
          ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-semibold shadow-2xs border border-slate-200/90 dark:border-slate-700'
          : 'text-slate-600 dark:text-slate-400 hover:bg-slate-200/50 dark:hover:bg-slate-800/60 hover:text-slate-900 dark:hover:text-slate-100 border border-transparent'}"
      >
        <Icon
          class="h-4 w-4 shrink-0 {active
            ? 'text-claude-600 dark:text-claude-400'
            : 'text-slate-500 dark:text-slate-400'}"
        />
        {#if !collapsed}
          <span class="flex-1 text-left truncate">{t(item.labelKey)}</span>
          {#if showDirtyDot}
            <span class="h-1.5 w-1.5 rounded-full bg-claude-500 shrink-0"></span>
          {:else if item.id === "mcp" && configStore.document.library.mcp.length > 0}
            <span class="text-[10px] font-mono text-slate-400 dark:text-slate-500">
              {configStore.document.library.mcp.length}
            </span>
          {:else if item.id === "providers" && configStore.document.providers.length > 0}
            <span class="text-[10px] font-mono text-slate-400 dark:text-slate-500">
              {configStore.document.providers.length}
            </span>
          {:else if item.id === "skills" && skillsStore.skills.length > 0}
            <span class="text-[10px] font-mono text-slate-400 dark:text-slate-500">
              {skillsStore.skills.length}
            </span>
          {/if}
        {/if}
      </button>
    {/each}
  </div>

  <!-- Footer tools -->
  <div class="p-2 border-t border-slate-200/70 dark:border-slate-800 space-y-0.5 shrink-0">
    {#each footerItems as item (item.id)}
      {@const Icon = item.icon}
      {@const active = appStore.activeNav === item.id}
      <button
        type="button"
        onclick={() => appStore.setNav(item.id)}
        title={t(item.labelKey)}
        class="w-full flex items-center gap-2.5 px-2.5 py-2 rounded-xl text-xs font-medium transition cursor-pointer {collapsed
          ? 'justify-center px-0'
          : ''} {active
          ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-semibold shadow-2xs border border-slate-200/90 dark:border-slate-700'
          : 'text-slate-600 dark:text-slate-400 hover:bg-slate-200/50 dark:hover:bg-slate-800/60 hover:text-slate-900 dark:hover:text-slate-100 border border-transparent'}"
      >
        <Icon
          class="h-4 w-4 shrink-0 {active
            ? 'text-claude-600 dark:text-claude-400'
            : 'text-slate-500 dark:text-slate-400'}"
        />
        {#if !collapsed}
          <span class="flex-1 text-left truncate">{t(item.labelKey)}</span>
        {/if}
      </button>
    {/each}
  </div>
</aside>
