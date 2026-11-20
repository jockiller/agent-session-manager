<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    Settings,
    Sun,
    Moon,
    Monitor,
    Terminal,
    FolderOpen,
    Globe,
    ExternalLink,
    Rows3,
  } from "@lucide/svelte";
  import { t, i18n, supportedLocales, type Locale } from "$lib/i18n";
  import {
    appStore,
    TERMINAL_OPTIONS,
    type DensityMode,
    type TerminalApp,
    type ThemeMode,
  } from "$lib/stores/app.svelte";
  import { configStore } from "$lib/config/store.svelte";
  import ConfigCard from "./config/ConfigCard.svelte";
  import { btnBorder, selectClass } from "./config/styles";

  let appVersion = $state("0.2.9");
  const repoUrl = "https://github.com/jockiller/agent-session-manager";

  onMount(() => {
    getVersion()
      .then((v) => {
        if (v) appVersion = v;
      })
      .catch(() => {});
  });

  async function revealBackups() {
    try {
      const dir = await invoke<string>("open_backups_dir");
      configStore.show({
        title: t("settings_backups_opened"),
        detail: dir,
        isError: false,
        revealedPath: "",
      });
    } catch (err) {
      configStore.show({
        title: t("cfg_save_failed"),
        detail: String(err),
        isError: true,
        revealedPath: "",
      });
    }
  }

  async function openRepo() {
    try {
      await openUrl(repoUrl);
    } catch {
      window.open(repoUrl, "_blank");
    }
  }
</script>

<div class="flex-1 min-h-0 overflow-y-auto bg-slate-50 dark:bg-slate-950">
  <div class="max-w-3xl mx-auto p-6 space-y-5">
    <div class="flex items-center gap-2">
      <Settings class="h-4 w-4 text-claude-600 dark:text-claude-400" />
      <h1 class="text-base font-bold text-slate-900 dark:text-slate-100">{t("nav_settings")}</h1>
    </div>

    <!-- Appearance -->
    <ConfigCard>
      <div class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("settings_appearance")}</div>

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-1">
        <div class="space-y-1.5">
          <div class="text-[11px] font-medium text-slate-500 dark:text-slate-400">{t("settings_theme")}</div>
          <div class="flex rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800/80 p-0.5">
            {#each [
              { id: "system", label: t("theme_system"), icon: Monitor },
              { id: "light", label: t("theme_light"), icon: Sun },
              { id: "dark", label: t("theme_dark"), icon: Moon },
            ] as opt (opt.id)}
              {@const Icon = opt.icon}
              <button
                type="button"
                onclick={() => appStore.setTheme(opt.id as ThemeMode)}
                class="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-md text-[11px] font-medium transition cursor-pointer {appStore.theme === opt.id
                  ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
                  : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
              >
                <Icon class="h-3.5 w-3.5" />
                <span>{opt.label}</span>
              </button>
            {/each}
          </div>
        </div>

        <div class="space-y-1.5">
          <div class="text-[11px] font-medium text-slate-500 dark:text-slate-400">{t("settings_density")}</div>
          <div class="flex rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800/80 p-0.5">
            {#each [
              { id: "comfortable", label: t("density_comfortable") },
              { id: "compact", label: t("density_compact") },
            ] as opt (opt.id)}
              <button
                type="button"
                onclick={() => appStore.setDensity(opt.id as DensityMode)}
                class="flex-1 flex items-center justify-center gap-1.5 px-2 py-1.5 rounded-md text-[11px] font-medium transition cursor-pointer {appStore.density === opt.id
                  ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
                  : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
              >
                <Rows3 class="h-3.5 w-3.5" />
                <span>{opt.label}</span>
              </button>
            {/each}
          </div>
        </div>
      </div>

      <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 pt-2 border-t border-slate-100 dark:border-slate-800">
        <div class="space-y-1.5">
          <div class="flex items-center gap-1.5 text-[11px] font-medium text-slate-500 dark:text-slate-400">
            <Globe class="h-3.5 w-3.5" />
            <span>{t("lang_switcher")}</span>
          </div>
          <select
            class={selectClass}
            value={i18n.locale}
            onchange={(e) => i18n.setLocale(e.currentTarget.value as Locale)}
          >
            {#each supportedLocales as loc}
              <option value={loc.id}>{loc.label}</option>
            {/each}
          </select>
        </div>

        <div class="space-y-1.5">
          <div class="flex items-center gap-1.5 text-[11px] font-medium text-slate-500 dark:text-slate-400">
            <Terminal class="h-3.5 w-3.5" />
            <span>{t("settings_terminal")}</span>
          </div>
          <select
            class={selectClass}
            value={appStore.preferredTerminal}
            onchange={(e) => appStore.setTerminal(e.currentTarget.value as TerminalApp)}
          >
            {#each TERMINAL_OPTIONS as term}
              <option value={term.id}>{term.label}</option>
            {/each}
          </select>
        </div>
      </div>
    </ConfigCard>

    <!-- Backups & Storage -->
    <ConfigCard>
      <div class="flex items-center justify-between gap-3">
        <div>
          <div class="text-xs font-bold text-slate-800 dark:text-slate-100">{t("settings_backups_title")}</div>
          <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">{t("settings_backups_desc")}</p>
        </div>
        <button type="button" class={btnBorder} onclick={revealBackups}>
          <FolderOpen class="h-3.5 w-3.5" />
          {t("settings_open_backups")}
        </button>
      </div>
    </ConfigCard>

    <!-- About -->
    <ConfigCard>
      <div class="flex items-center justify-between gap-4">
        <div class="flex items-center gap-3">
          <img
            src="/logo.png"
            alt="Agent Session Manager"
            class="h-11 w-11 rounded-xl border border-slate-200 dark:border-slate-700 object-contain"
          />
          <div>
            <div class="flex items-center gap-2">
              <span class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("app_title")}</span>
              <span class="px-2 py-0.5 rounded-full bg-claude-50 dark:bg-claude-950/70 border border-claude-200 dark:border-claude-800 text-[10px] font-mono font-semibold text-claude-700 dark:text-claude-300">
                v{appVersion}
              </span>
            </div>
            <p class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">{t("about_desc")}</p>
          </div>
        </div>
        <button type="button" class={btnBorder} onclick={openRepo}>
          <ExternalLink class="h-3.5 w-3.5" />
          GitHub
        </button>
      </div>
    </ConfigCard>
  </div>
</div>
