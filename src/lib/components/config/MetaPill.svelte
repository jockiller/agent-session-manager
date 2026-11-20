<script lang="ts">
  let { text, emphasized = false, title }: { text: string; emphasized?: boolean; title?: string } = $props();
</script>

<span
  class="inline-flex items-center px-1.5 py-0.5 rounded-full text-[10px] font-medium shrink-0 {emphasized
    ? 'bg-claude-600 text-white'
    : 'bg-slate-100 dark:bg-slate-800 text-slate-500 dark:text-slate-400 border border-slate-200/80 dark:border-slate-700'}"
  {title}
>
  {text}
</span>
