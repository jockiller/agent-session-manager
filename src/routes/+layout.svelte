<script lang="ts">
  import "../app.css";
  let { children } = $props();
</script>

<div class="h-screen w-screen overflow-hidden bg-slate-50 text-slate-900 flex flex-col font-sans antialiased">
  {@render children()}
</div>
