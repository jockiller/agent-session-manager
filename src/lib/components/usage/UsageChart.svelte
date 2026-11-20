<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import * as echarts from "echarts/core";
  import { BarChart, LineChart } from "echarts/charts";
  import { GridComponent, LegendComponent, TooltipComponent } from "echarts/components";
  import { CanvasRenderer } from "echarts/renderers";
  import type { EChartsCoreOption } from "echarts/core";
  import { appStore } from "$lib/stores/app.svelte";

  echarts.use([BarChart, LineChart, GridComponent, TooltipComponent, LegendComponent, CanvasRenderer]);

  let {
    option,
    height = "260px",
  }: {
    option: EChartsCoreOption;
    height?: string;
  } = $props();

  let container = $state<HTMLDivElement | null>(null);
  let chart: echarts.ECharts | null = null;
  let observer: ResizeObserver | null = null;

  onMount(() => {
    if (!container) return;
    chart = echarts.init(container);
    chart.setOption(option);
    observer = new ResizeObserver(() => {
      chart?.resize();
    });
    observer.observe(container);
  });

  onDestroy(() => {
    observer?.disconnect();
    observer = null;
    chart?.dispose();
    chart = null;
  });

  $effect(() => {
    const opt = option;
    const _dark = appStore.isDarkEffective;
    if (chart && opt) {
      chart.setOption(opt, { notMerge: true });
    }
  });
</script>

<div bind:this={container} class="w-full" style="height: {height};"></div>
