<script lang="ts">
  import { onMount } from "svelte";
  import { LoaderCircle } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { configStore as store } from "$lib/config/store.svelte";
  import { AGENT_TITLE } from "$lib/config/document";
  import { SIDEBAR_ITEMS, type SidebarItem } from "$lib/config/types";
  import AgentsView from "./AgentsView.svelte";
  import ProvidersView from "./ProvidersView.svelte";
  import LibraryView from "./LibraryView.svelte";
  import DirtyBadge from "./DirtyBadge.svelte";
  import ToastOverlay from "./ToastOverlay.svelte";

  let statusText = $derived.by(() => {
    if (store.errorMessage) return store.errorMessage;
    if (store.status) return store.status;
    switch (store.selectedSidebar) {
      case "agents":
        return t("cfg_hint_agents", { title: AGENT_TITLE[store.selectedAgent] });
      case "providers":
        return t("cfg_hint_providers");
      case "library":
        return t("cfg_hint_library");
    }
  });

  function tabLabel(item: SidebarItem): string {
    switch (item) {
      case "agents":
        return t("cfg_tab_agents");
      case "providers":
        return t("cfg_tab_providers");
      case "library":
        return t("cfg_tab_library");
    }
  }

  function onKeydown(event: KeyboardEvent) {
    const meta = event.metaKey || event.ctrlKey;
    if (meta && !event.shiftKey && event.key.toLowerCase() === "s") {
      event.preventDefault();
      if (store.selectedSidebar !== "agents" && !store.isDirty) return;
      void store.saveForPage();
      return;
    }
    if (meta && event.shiftKey && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (store.isDirty) store.revert();
    }
  }

  onMount(() => {
    void store.ensureLoaded();
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  });
</script>

<div class="flex-1 min-h-0 flex flex-col relative bg-slate-50">
  <div class="shrink-0 h-9 flex items-center justify-center border-b border-slate-200/80 bg-slate-50/90">
    <div class="flex items-center rounded-lg border border-slate-200 bg-slate-100/80 p-0.5">
      {#each SIDEBAR_ITEMS as item}
        <button
          type="button"
          class="px-3 py-1 rounded-md text-xs font-medium transition cursor-pointer {store.selectedSidebar === item
            ? 'bg-white text-slate-900 shadow-2xs'
            : 'text-slate-500 hover:text-slate-800'}"
          onclick={() => (store.selectedSidebar = item)}
        >
          {tabLabel(item)}
        </button>
      {/each}
    </div>
  </div>

  <div class="flex-1 min-h-0 relative">
    <div class="absolute inset-0 {store.selectedSidebar === 'agents' ? '' : 'invisible pointer-events-none'}">
      <AgentsView />
    </div>
    <div class="absolute inset-0 {store.selectedSidebar === 'providers' ? '' : 'invisible pointer-events-none'}">
      <ProvidersView />
    </div>
    <div class="absolute inset-0 {store.selectedSidebar === 'library' ? '' : 'invisible pointer-events-none'}">
      <LibraryView />
    </div>
  </div>

  <div class="shrink-0 h-7 flex items-center gap-2 px-4 border-t border-slate-200/80 bg-slate-100/80 text-[11px]">
    <span class="h-1.5 w-1.5 rounded-full {store.errorMessage ? 'bg-rose-500' : 'bg-slate-300'}"></span>
    <span class="flex-1 min-w-0 truncate {store.errorMessage ? 'text-rose-600' : 'text-slate-500'}">{statusText}</span>
    {#if store.isDirty}
      <DirtyBadge />
    {/if}
    {#if store.isExporting}
      <LoaderCircle class="h-3 w-3 animate-spin text-slate-400" />
    {/if}
  </div>

  {#if store.toast}
    <div class="pointer-events-none absolute top-12 right-5 z-20">
      <ToastOverlay
        toast={store.toast}
        onDismiss={() => store.dismissToast()}
        onReveal={(path) => { void store.revealPath(path); }}
      />
    </div>
  {/if}
</div>
