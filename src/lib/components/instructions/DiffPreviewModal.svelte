<script lang="ts">
  import { FileDiff, X } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { btnGhost, btnPrimary } from "../config/styles";

  export interface DiffEntry {
    targetId: string;
    name: string;
    path: string;
    before: string;
    after: string;
  }

  let {
    isOpen = false,
    entries = [],
    onClose,
    onConfirm,
  }: {
    isOpen: boolean;
    entries: DiffEntry[];
    onClose: () => void;
    onConfirm: () => Promise<void>;
  } = $props();

  let selectedId = $state("");
  let writing = $state(false);

  $effect(() => {
    if (isOpen && entries.length > 0 && !entries.some((e) => e.targetId === selectedId)) {
      selectedId = entries[0].targetId;
    }
  });

  let activeEntry = $derived(entries.find((e) => e.targetId === selectedId) ?? entries[0] ?? null);

  interface DiffLine {
    kind: "same" | "add" | "del";
    text: string;
  }

  function computeLineDiff(beforeText: string, afterText: string): DiffLine[] {
    const a = beforeText ? beforeText.replace(/\r\n/g, "\n").split("\n") : [];
    const b = afterText ? afterText.replace(/\r\n/g, "\n").split("\n") : [];
    const m = a.length;
    const n = b.length;
    if (m * n > 250_000) {
      return [
        ...a.map((text): DiffLine => ({ kind: "del", text })),
        ...b.map((text): DiffLine => ({ kind: "add", text })),
      ];
    }
    const dp: number[][] = Array.from({ length: m + 1 }, () => new Array<number>(n + 1).fill(0));
    for (let i = m - 1; i >= 0; i--) {
      for (let j = n - 1; j >= 0; j--) {
        dp[i][j] = a[i] === b[j] ? 1 + dp[i + 1][j + 1] : Math.max(dp[i + 1][j], dp[i][j + 1]);
      }
    }
    const out: DiffLine[] = [];
    let i = 0;
    let j = 0;
    while (i < m && j < n) {
      if (a[i] === b[j]) {
        out.push({ kind: "same", text: a[i] });
        i += 1;
        j += 1;
      } else if (dp[i + 1][j] >= dp[i][j + 1]) {
        out.push({ kind: "del", text: a[i] });
        i += 1;
      } else {
        out.push({ kind: "add", text: b[j] });
        j += 1;
      }
    }
    while (i < m) out.push({ kind: "del", text: a[i++] });
    while (j < n) out.push({ kind: "add", text: b[j++] });
    return out;
  }

  let diffLines = $derived(
    activeEntry ? computeLineDiff(activeEntry.before, activeEntry.after) : []
  );

  async function confirmWrite() {
    if (writing) return;
    writing = true;
    try {
      await onConfirm();
    } finally {
      writing = false;
    }
  }
</script>

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="fixed inset-0 z-50 bg-slate-900/50 backdrop-blur-xs flex items-center justify-center p-4"
    onclick={(e) => {
      if (e.target === e.currentTarget && !writing) onClose();
    }}
    role="dialog"
    aria-modal="true"
    tabindex="-1"
  >
    <div class="bg-white dark:bg-slate-900 border border-slate-200 dark:border-slate-700 rounded-2xl shadow-2xl w-full max-w-4xl h-[80vh] flex flex-col overflow-hidden">
      <div class="px-5 py-3.5 border-b border-slate-200 dark:border-slate-800 flex items-center justify-between shrink-0">
        <div class="flex items-center gap-2">
          <FileDiff class="h-4 w-4 text-claude-600 dark:text-claude-400" />
          <h2 class="text-sm font-bold text-slate-900 dark:text-slate-100">{t("inst_diff_title")}</h2>
        </div>
        <button
          type="button"
          onclick={onClose}
          disabled={writing}
          class="p-1 rounded-lg text-slate-400 hover:text-slate-700 dark:hover:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-800 cursor-pointer"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <div class="flex-1 min-h-0 flex overflow-hidden">
        {#if entries.length > 1}
          <div class="w-56 shrink-0 border-r border-slate-200 dark:border-slate-800 bg-slate-50 dark:bg-slate-950/50 overflow-y-auto p-2 space-y-1">
            {#each entries as entry (entry.targetId)}
              <button
                type="button"
                onclick={() => (selectedId = entry.targetId)}
                class="w-full text-left px-2.5 py-2 rounded-lg text-xs transition cursor-pointer {activeEntry?.targetId === entry.targetId
                  ? 'bg-white dark:bg-slate-800 text-slate-900 dark:text-slate-100 font-semibold shadow-2xs border border-slate-200 dark:border-slate-700'
                  : 'text-slate-600 dark:text-slate-400 hover:bg-slate-100 dark:hover:bg-slate-800/60'}"
              >
                <div class="truncate">{entry.name}</div>
                <div class="text-[10px] font-mono text-slate-400 truncate">{entry.path}</div>
              </button>
            {/each}
          </div>
        {/if}

        <div class="flex-1 min-w-0 flex flex-col min-h-0">
          {#if activeEntry}
            <div class="px-4 py-2 border-b border-slate-100 dark:border-slate-800 text-[11px] font-mono text-slate-500 dark:text-slate-400 bg-slate-50/70 dark:bg-slate-900/60 truncate">
              {activeEntry.path}
            </div>
            <div class="flex-1 overflow-y-auto font-mono text-[11px] leading-5 p-3 space-y-0.5 bg-slate-950 text-slate-200 select-text">
              {#each diffLines as line, i (`diff-${i}`)}
                <div
                  class="px-2 rounded-xs whitespace-pre-wrap break-all {line.kind === 'add'
                    ? 'bg-emerald-950/70 text-emerald-300'
                    : line.kind === 'del'
                      ? 'bg-rose-950/70 text-rose-300'
                      : 'text-slate-400'}"
                >
                  <span class="inline-block w-4 select-none opacity-70">{line.kind === "add" ? "+" : line.kind === "del" ? "-" : " "}</span>{line.text}
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <div class="px-5 py-3 border-t border-slate-200 dark:border-slate-800 bg-slate-50/80 dark:bg-slate-900 flex items-center justify-end gap-2 shrink-0">
        <button type="button" class={btnGhost} disabled={writing} onclick={onClose}>{t("cancel")}</button>
        <button type="button" class={btnPrimary} disabled={writing || entries.length === 0} onclick={confirmWrite}>
          {t("inst_diff_confirm")}
        </button>
      </div>
    </div>
  </div>
{/if}
