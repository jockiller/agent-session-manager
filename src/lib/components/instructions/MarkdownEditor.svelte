<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView } from "@codemirror/view";
  import { basicSetup } from "codemirror";
  import { markdown } from "@codemirror/lang-markdown";
  import { oneDark } from "@codemirror/theme-one-dark";
  import { Code2, Columns2, Eye } from "@lucide/svelte";
  import { t } from "$lib/i18n";
  import { appStore } from "$lib/stores/app.svelte";
  import { renderMarkdown } from "$lib/utils/markdown";

  let {
    value = "",
    onchange,
  }: {
    value: string;
    onchange: (next: string) => void;
  } = $props();

  type ViewMode = "edit" | "split" | "preview";
  let mode = $state<ViewMode>("edit");
  let hostEl = $state<HTMLDivElement | null>(null);
  let view: EditorView | null = null;
  let themeComp = new Compartment();
  let internalUpdate = false;

  let stats = $derived.by(() => {
    const chars = value.length;
    const lines = value ? value.split(/\r?\n/).length : 0;
    const estTokens = Math.ceil(chars / 3.5);
    return { chars, lines, estTokens };
  });

  function themeExtension(dark: boolean) {
    return dark
      ? [
          oneDark,
          EditorView.theme({
            "&": { height: "100%", fontSize: "12px", backgroundColor: "#1A1917", color: "#F2F0E8" },
            ".cm-scroller": { overflow: "auto" },
            ".cm-gutters": { backgroundColor: "#161513", color: "#716D64", borderRight: "1px solid #292825" },
            ".cm-activeLine": { backgroundColor: "#242320" },
            ".cm-activeLineGutter": { backgroundColor: "#242320" },
            ".cm-cursor": { borderLeftColor: "#E08766" },
          }),
        ]
      : [
          EditorView.theme({
            "&": { height: "100%", fontSize: "12px", backgroundColor: "#ffffff", color: "#1F1E1C" },
            ".cm-scroller": { overflow: "auto" },
            ".cm-gutters": { backgroundColor: "#FAF9F5", color: "#9B968B", borderRight: "1px solid #E5E2D9" },
            ".cm-activeLine": { backgroundColor: "#FAF9F5" },
            ".cm-activeLineGutter": { backgroundColor: "#F2F0E8" },
            ".cm-cursor": { borderLeftColor: "#C15F3C" },
          }),
        ];
  }

  onMount(() => {
    if (!hostEl) return;
    const state = EditorState.create({
      doc: value,
      extensions: [
        basicSetup,
        markdown(),
        EditorView.lineWrapping,
        themeComp.of(themeExtension(appStore.isDarkEffective)),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !internalUpdate) {
            onchange(update.state.doc.toString());
          }
        }),
      ],
    });
    view = new EditorView({ state, parent: hostEl });
  });

  onDestroy(() => {
    view?.destroy();
    view = null;
  });

  $effect(() => {
    const dark = appStore.isDarkEffective;
    if (view) {
      view.dispatch({
        effects: themeComp.reconfigure(themeExtension(dark)),
      });
    }
  });

  $effect(() => {
    const incoming = value;
    if (!view) return;
    const current = view.state.doc.toString();
    if (incoming !== current) {
      internalUpdate = true;
      view.dispatch({
        changes: { from: 0, to: current.length, insert: incoming },
      });
      internalUpdate = false;
    }
  });
</script>

<div class="flex-1 min-h-0 flex flex-col border border-slate-200/90 dark:border-slate-700 rounded-xl overflow-hidden bg-white dark:bg-slate-900 shadow-2xs">
  <!-- Toolbar -->
  <div class="shrink-0 h-9 px-3 border-b border-slate-200/80 dark:border-slate-800 bg-slate-50/80 dark:bg-slate-900 flex items-center justify-between gap-3">
    <div class="flex items-center rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-100/80 dark:bg-slate-800 p-0.5">
      <button
        type="button"
        onclick={() => (mode = "edit")}
        class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium transition cursor-pointer {mode === 'edit'
          ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
          : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
      >
        <Code2 class="h-3 w-3" />
        <span>{t("inst_editor_edit")}</span>
      </button>
      <button
        type="button"
        onclick={() => (mode = "split")}
        class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium transition cursor-pointer {mode === 'split'
          ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
          : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
      >
        <Columns2 class="h-3 w-3" />
        <span>{t("inst_editor_split")}</span>
      </button>
      <button
        type="button"
        onclick={() => (mode = "preview")}
        class="flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium transition cursor-pointer {mode === 'preview'
          ? 'bg-white dark:bg-slate-700 text-slate-900 dark:text-slate-100 shadow-2xs'
          : 'text-slate-500 dark:text-slate-400 hover:text-slate-800 dark:hover:text-slate-200'}"
      >
        <Eye class="h-3 w-3" />
        <span>{t("inst_editor_preview")}</span>
      </button>
    </div>

    <div class="flex items-center gap-2.5 text-[10px] font-mono text-slate-400 dark:text-slate-500">
      <span>{t("inst_lines", { n: stats.lines })}</span>
      <span>·</span>
      <span>{t("inst_chars", { n: stats.chars })}</span>
      <span>·</span>
      <span class="px-1.5 py-0.5 rounded bg-claude-50 dark:bg-claude-950/70 text-claude-700 dark:text-claude-300 font-semibold">
        {t("inst_est_tokens", { n: stats.estTokens })}
      </span>
    </div>
  </div>

  <!-- Editor + Preview Panes -->
  <div class="flex-1 min-h-0 flex overflow-hidden">
    <div
      class="min-h-0 min-w-0 select-text {mode === 'preview' ? 'hidden' : mode === 'split' ? 'w-1/2 border-r border-slate-200 dark:border-slate-800' : 'flex-1'}"
      bind:this={hostEl}
    ></div>
    {#if mode !== "edit"}
      <div class="min-h-0 min-w-0 overflow-y-auto p-4 bg-slate-50/40 dark:bg-slate-950/40 text-slate-800 dark:text-slate-100 select-text asm-prose {mode === 'split' ? 'w-1/2' : 'flex-1'}">
        {@html renderMarkdown(value)}
      </div>
    {/if}
  </div>
</div>
