<script lang="ts">
  import { X, ExternalLink, Copy, Check } from "@lucide/svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { t } from "../i18n";

  let {
    isOpen = false,
    onClose,
  }: {
    isOpen: boolean;
    onClose: () => void;
  } = $props();

  const repoUrl = "https://github.com/jockiller/agent-session-manager";
  const appVersion = "0.1.0";

  let copied = $state(false);

  async function openGithub() {
    try {
      await openUrl(repoUrl);
    } catch {
      window.open(repoUrl, "_blank");
    }
  }

  function copyRepoUrl() {
    navigator.clipboard.writeText(repoUrl);
    copied = true;
    setTimeout(() => (copied = false), 2000);
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <!-- Backdrop -->
  <div
    class="fixed inset-0 z-50 bg-slate-900/40 backdrop-blur-xs flex items-center justify-center p-4 animate-in fade-in duration-150 select-none"
    onclick={onClose}
    role="dialog"
    aria-modal="true"
    aria-labelledby="about-title"
    tabindex="-1"
  >
    <!-- Modal Card -->
    <div
      class="bg-white rounded-2xl shadow-xl border border-slate-200/90 w-full max-w-sm overflow-hidden flex flex-col animate-in zoom-in-95 duration-150"
      onclick={(e) => e.stopPropagation()}
      role="document"
    >
      <!-- Top Bar with Close button -->
      <div class="px-4 pt-3 flex justify-end">
        <button
          onclick={onClose}
          class="p-1 rounded-lg text-slate-400 hover:text-slate-600 hover:bg-slate-100 transition cursor-pointer"
          title={t("close")}
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- App Info Content -->
      <div class="px-6 pb-6 pt-1 flex flex-col items-center text-center space-y-3">
        <!-- Logo -->
        <div class="relative">
          <img
            src="/logo.png"
            alt="Agent Session Manager"
            class="h-16 w-16 rounded-2xl shadow-md border border-slate-200/60 object-contain select-none"
          />
        </div>

        <!-- Title & Version -->
        <div class="space-y-1">
          <h2 id="about-title" class="text-base font-bold text-slate-900 tracking-tight">
            {t("app_title")}
          </h2>
          <div class="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full bg-sky-50 border border-sky-200/80 text-[11px] font-mono font-semibold text-sky-700">
            <span>{t("about_version")}</span>
            <span>v{appVersion}</span>
          </div>
        </div>

        <!-- Description -->
        <p class="text-xs text-slate-600 leading-relaxed max-w-[280px]">
          {t("about_desc")}
        </p>

        <!-- Ecosystem tag -->
        <div class="text-[11px] text-slate-500 bg-slate-50 border border-slate-200/70 rounded-lg px-3 py-1 font-medium">
          {t("supported_ecosystem")}
        </div>

        <!-- GitHub Primary Action Link Button -->
        <div class="w-full pt-1 space-y-1.5">
          <button
            onclick={openGithub}
            class="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-xl bg-slate-900 hover:bg-slate-800 active:bg-black text-white text-xs font-semibold shadow-xs transition cursor-pointer group"
          >
            <!-- GitHub SVG mark -->
            <svg class="h-4 w-4 fill-current shrink-0" viewBox="0 0 24 24" aria-hidden="true">
              <path fill-rule="evenodd" clip-rule="evenodd" d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z" />
            </svg>
            <span>{t("open_github")}</span>
            <ExternalLink class="h-3.5 w-3.5 text-slate-400 group-hover:text-white transition shrink-0" />
          </button>

          <!-- Copy URL button -->
          <button
            onclick={copyRepoUrl}
            class="text-[10px] text-slate-400 hover:text-slate-600 transition cursor-pointer flex items-center justify-center gap-1 mx-auto"
          >
            {#if copied}
              <Check class="h-3 w-3 text-emerald-600" />
              <span class="text-emerald-700 font-medium">Copied URL</span>
            {:else}
              <Copy class="h-3 w-3" />
              <span>github.com/jockiller/agent-session-manager</span>
            {/if}
          </button>
        </div>

        <!-- Tech Footer -->
        <div class="pt-2 border-t border-slate-100 w-full text-[10px] text-slate-400 font-mono flex items-center justify-between">
          <span>Tauri 2 · Svelte 5 · Rust</span>
          <span>MIT License</span>
        </div>
      </div>
    </div>
  </div>
{/if}
