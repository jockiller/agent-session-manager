<script lang="ts">
  import "../app.css";
  let { children } = $props();
</script>

<div class="h-screen w-screen overflow-hidden bg-slate-50 dark:bg-slate-950 text-slate-900 dark:text-slate-100 flex flex-col font-sans antialiased">
  {@render children()}
</div>
