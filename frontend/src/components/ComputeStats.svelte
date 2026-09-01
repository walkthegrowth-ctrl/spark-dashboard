<script>
  import { onMount } from 'svelte';
  import CollapsibleCard from './CollapsibleCard.svelte';
  import StatusBar from './StatusBar.svelte';
  import Sparkline from './Sparkline.svelte';
  import { touch } from '../lib/lastUpdated.js';

  let compute = null;
  let loading = true;
  let error = null;
  let collapsed = false; // expanded by default

  // In-memory session history for the two utilization series, one sample per
  // refresh (1 s), capped. Both are 0–100 %, so the chart ceiling is 100.
  const MAX_SAMPLES = 3600;
  let cpuUtilArr = [];
  let gpuUtilArr = [];
  let computeTimes = [];

  function append(arr, v) {
    const next = arr.concat(v);
    return next.length > MAX_SAMPLES ? next.slice(next.length - MAX_SAMPLES) : next;
  }

  // The CPU bar plots the mean of per-core utilization; mirror that here.
  function pushComputeSample(c) {
    const coresArr = (c && c.cores) || [];
    let cpu = null;
    if (coresArr.length) {
      cpu = coresArr.reduce((s, x) => s + (x.utilization_pct ?? 0), 0) / coresArr.length;
    } else if (c.cpu_utilization_pct !== null && c.cpu_utilization_pct !== undefined) {
      cpu = c.cpu_utilization_pct;
    }
    if (cpu !== null) cpuUtilArr = append(cpuUtilArr, cpu);
    const gpu = c.gpu_utilization_pct;
    if (gpu !== null && gpu !== undefined) gpuUtilArr = append(gpuUtilArr, gpu);
    if (cpu !== null || (gpu !== null && gpu !== undefined)) {
      computeTimes = append(computeTimes, Date.now());
    }
  }

  function fmtPct(v) {
    return v === null || v === undefined ? 'N/A' : `${v.toFixed(0)}%`;
  }

  async function fetchCompute() {
    try {
      loading = true;
      error = null;
      const res = await fetch('/api/compute/current');
      if (!res.ok) {
        if (res.status === 503) {
          error = 'No data available yet. Waiting for collector...';
          return;
        }
        throw new Error(`HTTP ${res.status}`);
      }
      compute = await res.json();
      pushComputeSample(compute);
      touch();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function formatPct(v) {
    if (v === null || v === undefined) return 'N/A';
    return `${Math.round(v)}%`;
  }

  // Load figures arrive as percentages already (CPU: load/cores*100; GPU:
  // utilization averages). Render with one decimal so small loads stay legible.
  function loadPct(v) {
    if (v === null || v === undefined) return 'N/A';
    return `${v.toFixed(1)}%`;
  }

  function utilizationColor(pct) {
    if (pct === null || pct === undefined) return '#555';
    if (pct < 50) return '#22c55e';
    if (pct < 80) return '#eab308';
    if (pct < 95) return '#f97316';
    return '#ef4444';
  }

  function utilizationLabel(pct) {
    if (pct === null || pct === undefined) return 'N/A';
    if (pct < 15) return 'Idle';
    if (pct < 60) return 'Active';
    if (pct < 90) return 'Busy';
    return 'Saturated';
  }

  // --- radar geometry ------------------------------------------------------
  const CX = 200;
  const CY = 200;
  const R = 152;
  const TIP_R = 174;
  const TAU = Math.PI * 2;
  const RING_FRACS = [0.25, 0.5, 0.75, 1.0];

  function pointOnSpoke(n, i, radius) {
    const angle = -Math.PI / 2 + (TAU * i) / Math.max(n, 1);
    return [CX + radius * Math.cos(angle), CY + radius * Math.sin(angle)];
  }

  function ringOf(n, frac) {
    if (!n) return '';
    const pts = Array.from({ length: n }, (_, i) => {
      const [x, y] = pointOnSpoke(n, i, R * frac);
      return `${i ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`;
    });
    return pts.join(' ') + ' Z';
  }

  function spokeEnds(n) {
    if (!n) return [];
    return Array.from({ length: n }, (_, i) => {
      const [x, y] = pointOnSpoke(n, i, R);
      return { x: +x.toFixed(1), y: +y.toFixed(1) };
    });
  }

  function dotsFor(values, n) {
    return values.map((v, i) => {
      const radius = (R * Math.max(0, Math.min(v ?? 0, 100))) / 100;
      const [x, y] = pointOnSpoke(n, i, radius);
      return {
        i, x: +x.toFixed(1), y: +y.toFixed(1),
        pct: v ?? 0,
        color: utilizationColor(v),
      };
    });
  }

  function fillPath(dots) {
    return dots
      .map((d, i) => `${i ? 'L' : 'M'}${d.x} ${d.y}`)
      .join(' ') + ' Z';
  }

  function tipLabels(n, labels) {
    return Array.from({ length: n }, (_, i) => {
      const [x, y] = pointOnSpoke(n, i, TIP_R);
      return { x: +x.toFixed(1), y: +y.toFixed(1), text: labels[i] };
    });
  }

  // Spokes are always ordered by load, highest first, so the radar shows load
  // surfaces (a smooth blob) rather than isolated spikes amid idle cores.
  $: cores = (compute && compute.cores || []).slice().sort(
    (a, b) => (b.utilization_pct ?? 0) - (a.utilization_pct ?? 0) || a.index - b.index
  );

  $: cpuTotal = (compute && compute.cpu_utilization_pct) ?? null;

  $: cpuAvg = cores.length
    ? cores.reduce((sum, c) => sum + c.utilization_pct, 0) / cores.length
    : cpuTotal;

  $: cpuN = cores.length || 1;
  $: cpuValues = cores.map((c) => c.utilization_pct);
  $: cpuDots = dotsFor(cpuValues, cpuN);
  $: cpuRings = RING_FRACS.map((f) => ringOf(cpuN, f));
  $: cpuSpokes = spokeEnds(cpuN);
  $: cpuPath = fillPath(cpuDots);
  $: cpuTips = cores.length ? tipLabels(cores.length, cores.map((c) => c.index)) : [];

  // GPU radar: 48 spokes all reporting the same (global) GPU load, labelled as SM slots.
  const GPU_SPOKES = 48;
  $: gpuPct = (compute && compute.gpu_utilization_pct) ?? 0;
  $: gpuColor = utilizationColor(compute ? compute.gpu_utilization_pct : null);
  $: gpuValues = Array.from({ length: GPU_SPOKES }, () => gpuPct);
  $: gpuDots = dotsFor(gpuValues, GPU_SPOKES);
  $: gpuRings = RING_FRACS.map((f) => ringOf(GPU_SPOKES, f));
  $: gpuSpokes = spokeEnds(GPU_SPOKES);
  $: gpuPath = fillPath(gpuDots);
  $: gpuTips = tipLabels(GPU_SPOKES, Array.from({ length: GPU_SPOKES }, (_, i) => i + 1));

  $: cpuBarColor = utilizationColor(cpuAvg);
  $: gpuBarColor = utilizationColor(compute ? compute.gpu_utilization_pct : null);

  onMount(fetchCompute);
  const interval = setInterval(fetchCompute, 1000);
</script>

<CollapsibleCard
  title="Compute Load"
  {collapsed}
  on:toggle={() => (collapsed = !collapsed)}
>
  <svelte:fragment slot="summary">
    {#if compute}
      <StatusBar
        label="CPU utilization"
        value={Math.min(cpuAvg ?? 0, 100)}
        color={cpuBarColor}
        display={formatPct(cpuAvg)}
        tooltip="Mean of all CPU core utilization."
      />
      <StatusBar
        label="GPU utilization"
        value={Math.min(compute.gpu_utilization_pct ?? 0, 100)}
        color={gpuBarColor}
        display={formatPct(compute.gpu_utilization_pct)}
        tooltip="GPU utilization reported by nvidia-smi."
      />
    {/if}
  </svelte:fragment>

  {#if loading && !compute}
    <div class="loading">Loading...</div>
  {:else if error}
    <div class="error">{error}</div>
  {:else if compute}
    <div class="metrics">
      <div class="row-label cpu">CPU</div>
      <div class="metric current" data-tooltip="CPU utilization right now (share of total core capacity in use).">
        <span class="label">Current</span>
        <span class="value" style="color: {utilizationColor(cpuTotal)}">{formatPct(cpuTotal)}</span>
        <span class="status">{utilizationLabel(cpuTotal)}</span>
      </div>
      <div class="metric" data-tooltip="CPU load, 1-min run-queue average normalized to % of the maximum total load (100% = every core fully busy).">
        <span class="label">1m</span>
        <span class="value">{loadPct(compute.load_1_pct)}</span>
        <span class="status">% of max</span>
      </div>
      <div class="metric" data-tooltip="CPU load, 5-min run-queue average normalized to % of the maximum total load (100% = every core fully busy).">
        <span class="label">5m</span>
        <span class="value">{loadPct(compute.load_5_pct)}</span>
        <span class="status">% of max</span>
      </div>
      <div class="metric" data-tooltip="CPU load, 15-min run-queue average normalized to % of the maximum total load (100% = every core fully busy).">
        <span class="label">15m</span>
        <span class="value">{loadPct(compute.load_15_pct)}</span>
        <span class="status">% of max</span>
      </div>
      <div class="metric" data-tooltip="CPU load over the last hour, derived from the history of 1-min run-queue averages (the kernel has no 60-min average), normalized to % of max.">
        <span class="label">60m</span>
        <span class="value">{loadPct(compute.load_60_pct)}</span>
        <span class="status">% of max</span>
      </div>

      <div class="row-label gpu">GPU</div>
      <div class="metric current" data-tooltip="GPU utilization right now (nvidia-smi).">
        <span class="label">Current</span>
        <span class="value" style="color: {utilizationColor(compute.gpu_utilization_pct)}">
          {formatPct(compute.gpu_utilization_pct)}
        </span>
        <span class="status">{utilizationLabel(compute.gpu_utilization_pct)}</span>
      </div>
      <div class="metric" data-tooltip="GPU load, 1-min rolling average of GPU utilization (nvidia-smi). The platform exposes no GPU run-queue, so this is the utilization-based analogue of load.">
        <span class="label">1m</span>
        <span class="value">{loadPct(compute.gpu_load_1)}</span>
        <span class="status">% of full</span>
      </div>
      <div class="metric" data-tooltip="GPU load, 5-min rolling average of GPU utilization (nvidia-smi). The platform exposes no GPU run-queue, so this is the utilization-based analogue of load.">
        <span class="label">5m</span>
        <span class="value">{loadPct(compute.gpu_load_5)}</span>
        <span class="status">% of full</span>
      </div>
      <div class="metric" data-tooltip="GPU load, 15-min rolling average of GPU utilization (nvidia-smi). The platform exposes no GPU run-queue, so this is the utilization-based analogue of load.">
        <span class="label">15m</span>
        <span class="value">{loadPct(compute.gpu_load_15)}</span>
        <span class="status">% of full</span>
      </div>
      <div class="metric" data-tooltip="GPU load over the last hour, a rolling average of GPU utilization samples (nvidia-smi). The platform exposes no GPU run-queue, so this is the utilization-based analogue of load.">
        <span class="label">60m</span>
        <span class="value">{loadPct(compute.gpu_load_60)}</span>
        <span class="status">% of full</span>
      </div>
    </div>

    <StatusBar label="CPU utilization" value={Math.min(cpuAvg ?? 0, 100)} color={cpuBarColor} display={formatPct(cpuAvg)} tooltip="Mean of all CPU core utilization." />
    <StatusBar label="GPU utilization" value={Math.min(compute.gpu_utilization_pct ?? 0, 100)} color={gpuBarColor} display={formatPct(compute.gpu_utilization_pct)} tooltip="GPU utilization reported by nvidia-smi." />

    <div class="history">
      <div class="history-label">
        Time dynamics
        <span class="hint">since this page opened · fixed scale 0 → 100 %</span>
      </div>
      <div class="charts">
        <Sparkline label="CPU utilization" color="#38bdf8" values={cpuUtilArr} timestamps={computeTimes} max={100} format={fmtPct} />
        <Sparkline label="GPU utilization" color="#a78bfa" values={gpuUtilArr} timestamps={computeTimes} max={100} format={fmtPct} />
      </div>
    </div>

    <div class="radars">
      {#if cpuDots.length}
        <div class="radar-block">
          <div class="group-label" data-tooltip="Spokes ordered by load, highest first (not by core number), to make load surfaces easier to read.">
            CPU cores*
          </div>
          <svg viewBox="0 0 400 400" class="radar" role="img" aria-label="Per-core CPU utilization radar chart">
            {#each cpuRings as path}
              <path d="{path}" class="radar-ring"></path>
            {/each}
            {#each cpuSpokes as sp}
              <line x1={CX} y1={CY} x2={sp.x} y2={sp.y} class="radar-spoke"></line>
            {/each}

            {#each cpuDots as dot}
              <line x1={CX} y1={CY} x2={dot.x} y2={dot.y} class="radar-spoke-val" stroke={dot.color} opacity="0.3"></line>
            {/each}

            <path d="{cpuPath}" fill="{utilizationColor(cpuAvg)}" fill-opacity="0.12" stroke="{utilizationColor(cpuAvg)}" stroke-opacity="0.4" stroke-width="1"></path>

            {#each cpuDots as dot}
              <circle cx={dot.x} cy={dot.y} r="4.5" class="radar-dot" style="fill: {dot.color}">
                <title>Core {cores[dot.i].index}: {Math.round(dot.pct)}%</title>
              </circle>
            {/each}

            {#each cpuTips as tip}
              <text x={tip.x} y={tip.y + 3} class="radar-tip" text-anchor="middle">{tip.text}</text>
            {/each}
          </svg>
        </div>
      {/if}

      <div class="radar-block">
        <div class="group-label" data-tooltip="All 48 spokes reflect the global GPU core utilization reported by nvidia-smi. NVIDIA GB10 exposes a single aggregate GPU metric.">
          GPU SMs*
        </div>
        <svg viewBox="0 0 400 400" class="radar radar-gpu" role="img" aria-label="GPU utilization radar (48 spokes, all at the global GPU load level)">
          {#each gpuRings as path}
            <path d="{path}" class="radar-ring"></path>
          {/each}
          {#each gpuSpokes as sp}
            <line x1={CX} y1={CY} x2={sp.x} y2={sp.y} class="radar-spoke"></line>
          {/each}

          {#each gpuDots as dot}
            <line x1={CX} y1={CY} x2={dot.x} y2={dot.y} class="radar-spoke-val" stroke={gpuColor} opacity="0.3"></line>
          {/each}

          <path d="{gpuPath}" fill="{gpuColor}" fill-opacity="0.12" stroke="{gpuColor}" stroke-opacity="0.4" stroke-width="1"></path>

          {#each gpuDots as dot}
            <circle cx={dot.x} cy={dot.y} r="3" class="radar-dot radar-dot-sm" style="fill: {gpuColor}">
              <title>GPU core slot {dot.i + 1}: {Math.round(dot.pct)}%</title>
            </circle>
          {/each}

          {#each gpuTips as tip}
            <text x={tip.x} y={tip.y + 3} class="radar-tip radar-tip-sm" text-anchor="middle">{tip.text}</text>
          {/each}
        </svg>
      </div>
    </div>
  {/if}
</CollapsibleCard>

<style>
  .loading { color: #888; padding: 1rem 0; }
  .error { color: #f87171; padding: 1rem 0; }
  .metrics {
    display: grid;
    grid-template-columns: 64px repeat(5, minmax(90px, 1fr));
    gap: 0.6rem;
    margin-bottom: 1.5rem;
    align-items: stretch;
  }
  .row-label {
    align-self: center;
    font-size: 0.85rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #bbb;
    text-transform: uppercase;
  }
  .row-label.cpu { color: #38bdf8; }
  .row-label.gpu { color: #a78bfa; }
  .metric {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.75rem;
    border-radius: 6px;
    border: 1px solid #333;
    background: #0a0a0a;
    cursor: default;
    position: relative;
  }
  .metric[data-tooltip]:hover::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    background: #1a1a1a;
    color: #ccc;
    font-size: 0.7rem;
    font-family: monospace;
    padding: 4px 8px;
    border-radius: 4px;
    border: 1px solid #444;
    white-space: nowrap;
    z-index: 10;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
  }
  .label { font-size: 0.75rem; color: #888; text-transform: uppercase; letter-spacing: 0.05em; }
  .value { font-size: 1.125rem; font-weight: 500; font-variant-numeric: tabular-nums; }
  .status { font-size: 0.65rem; text-transform: uppercase; letter-spacing: 0.05em; color: #666 !important; }
  .group-label {
    font-size: 0.85rem; font-weight: 600; color: #bbb;
    text-transform: none; letter-spacing: 0.02em;
    margin: 1.5rem 0 0.25rem;
    cursor: default;
    position: relative;
  }
  .group-label[data-tooltip]:hover::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: calc(100% + 6px);
    left: 0;
    background: #1a1a1a;
    color: #ccc;
    font-size: 0.7rem;
    font-family: monospace;
    padding: 4px 8px;
    border-radius: 4px;
    border: 1px solid #444;
    white-space: nowrap;
    z-index: 10;
    pointer-events: none;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5);
  }
  .radars {
    display: flex;
    flex-wrap: wrap;
    gap: 2rem;
    align-items: flex-start;
    justify-content: center;
    margin-top: 1rem;
  }
  .radar-block {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
  }
  .radar {
    width: 100%;
    max-width: 380px;
    display: block;
  }
  .radar-ring { fill: none; stroke: #333; stroke-width: 1; }
  .radar-spoke { stroke: #2a2a2a; stroke-width: 1; }
  .radar-spoke-val { stroke-width: 1; }
  .radar-dot {
    cursor: crosshair;
    stroke: #111;
    stroke-width: 1;
    transition: r 0.15s ease;
  }
  .radar-dot:hover { r: 7; }
  .radar-dot-sm:hover { r: 6; }
  /* GPU SM radar: smooth, quick transitions when values change (< 500 ms). */
  .radar-gpu .radar-spoke-val,
  .radar-gpu path {
    transition: 0.25s ease-out;
  }
  .radar-gpu .radar-dot {
    transition: cx 0.25s ease-out, cy 0.25s ease-out, r 0.15s ease;
  }
  .radar-tip {
    fill: #888;
    font-size: 10px;
    font-family: ui-monospace, SFMono-Regular, monospace;
    pointer-events: none;
  }
  .radar-tip-sm {
    font-size: 8px;
    fill: #666;
  }
  .history {
    margin-top: 1.75rem;
    border-top: 1px solid #222;
    padding-top: 1rem;
  }
  .history-label {
    display: flex;
    gap: 0.75rem;
    align-items: baseline;
    font-size: 0.85rem;
    font-weight: 600;
    color: #bbb;
    letter-spacing: 0.02em;
    margin-bottom: 0.75rem;
  }
  .history-label .hint {
    font-size: 0.68rem;
    font-weight: 400;
    color: #666;
    letter-spacing: 0;
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(210px, 1fr));
    gap: 0.75rem;
  }
</style>
