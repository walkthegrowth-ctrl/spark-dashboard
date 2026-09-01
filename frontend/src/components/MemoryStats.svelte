<script>
  import { onMount } from 'svelte';
  import CollapsibleCard from './CollapsibleCard.svelte';
  import StatusBar from './StatusBar.svelte';
  import Sparkline from './Sparkline.svelte';
  import { touch } from '../lib/lastUpdated.js';

  let memory = null;
  let loading = true;
  let error = null;
  let collapsed = false; // expanded by default

  // In-memory history for the current session: one sample per refresh (1 s),
  // oldest → newest, capped so a long-lived tab stays bounded. Each is a top-level
  // reactive `let` (reassigned with a fresh array each tick) so Svelte reliably
  // notifies the child <Sparkline/> props.
  const MAX_SAMPLES = 3600;
  let inUseArr = [];
  let availableArr = [];
  let freeArr = [];
  let buffersArr = [];
  let cachedArr = [];
  let swapFreeArr = [];
  let timesArr = [];

  function append(arr, v) {
    const next = arr.concat(v);
    return next.length > MAX_SAMPLES ? next.slice(next.length - MAX_SAMPLES) : next;
  }

  async function fetchMemory() {
    try {
      loading = true;
      error = null;
      const res = await fetch('/api/memory/current');
      if (!res.ok) {
        if (res.status === 503) {
          error = 'No data available yet. Waiting for collector...';
          return;
        }
        throw new Error(`HTTP ${res.status}`);
      }
      memory = await res.json();
      pushSample(memory);
      touch();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function pushSample(m) {
    const total = m.mem_total_bytes ?? 0;
    inUseArr = append(inUseArr, total ? total - (m.mem_available_bytes ?? 0) : 0);
    availableArr = append(availableArr, m.mem_available_bytes ?? 0);
    freeArr = append(freeArr, m.mem_free_bytes ?? 0);
    buffersArr = append(buffersArr, m.buffers_bytes ?? 0);
    cachedArr = append(cachedArr, m.cached_bytes ?? 0);
    swapFreeArr = append(swapFreeArr, m.swap_free_bytes ?? 0);
    timesArr = append(timesArr, Date.now());
  }

  $: usagePct = memory && memory.mem_total_bytes
    ? Math.round(((memory.mem_total_bytes - (memory.mem_available_bytes ?? 0)) / memory.mem_total_bytes) * 100)
    : 0;

  $: usedBytes = memory && memory.mem_total_bytes !== null && memory.mem_available_bytes !== null
    ? memory.mem_total_bytes - memory.mem_available_bytes
    : null;

  function usageColor(pct) {
    if (pct >= 90) return '#ef4444';
    if (pct >= 75) return '#f97316';
    if (pct >= 60) return '#eab308';
    return '#22c55e';
  }

  $: inUseDisplay = usagePct === 0 && memory ? '0%' : `${usagePct}%`;

  function fmt(bytes) {
    if (bytes === null || bytes === undefined) return 'N/A';
    const units = ['B', 'KB', 'MB', 'GB', 'TB'];
    let i = 0;
    let val = bytes;
    while (val >= 1024 && i < units.length - 1) {
      val /= 1024;
      i++;
    }
    return `${val.toFixed(1)} ${units[i]}`;
  }

  onMount(fetchMemory);
  const interval = setInterval(fetchMemory, 1000);
</script>

<CollapsibleCard
  title="Memory"
  {collapsed}
  on:toggle={() => (collapsed = !collapsed)}
>
  <svelte:fragment slot="summary">
    {#if memory}
      <StatusBar
        label="In Use"
        value={usagePct}
        color={usageColor(usagePct)}
        display={inUseDisplay}
        tooltip="Share of total RAM in use (total − available)."
      />
    {/if}
  </svelte:fragment>

  {#if loading && !memory}
    <div class="loading">Loading...</div>
  {:else if error}
    <div class="error">{error}</div>
  {:else if memory}
    <div class="stats">
      <div class="stat">
        <span class="label">Total</span>
        <span class="value">{fmt(memory.mem_total_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">In Use</span>
        <span class="value">{usedBytes === null ? 'N/A' : fmt(usedBytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Available</span>
        <span class="value">{memory.mem_available_bytes === null ? 'N/A' : fmt(memory.mem_available_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Free</span>
        <span class="value">{memory.mem_free_bytes === null ? 'N/A' : fmt(memory.mem_free_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Buffers</span>
        <span class="value">{memory.buffers_bytes === null ? 'N/A' : fmt(memory.buffers_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Cached</span>
        <span class="value">{memory.cached_bytes === null ? 'N/A' : fmt(memory.cached_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Swap Total</span>
        <span class="value">{memory.swap_total_bytes === null ? 'N/A' : fmt(memory.swap_total_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Swap Free</span>
        <span class="value">{memory.swap_free_bytes === null ? 'N/A' : fmt(memory.swap_free_bytes)}</span>
      </div>
    </div>

    <StatusBar
      label="In Use"
      value={usagePct}
      color={usageColor(usagePct)}
      display={inUseDisplay}
      tooltip="Share of total RAM in use (total − available)."
    />

    <div class="history">
      <div class="group-label">Time dynamics <span class="hint">since this page opened · fixed scale 0 → total (swap-free: 0 → total swap)</span></div>
      <div class="charts">
        <Sparkline label="In Use" color="#ef4444" values={inUseArr} timestamps={timesArr} max={memory.mem_total_bytes} format={fmt} />
        <Sparkline label="Available" color="#22c55e" values={availableArr} timestamps={timesArr} max={memory.mem_total_bytes} format={fmt} />
        <Sparkline label="Free" color="#38bdf8" values={freeArr} timestamps={timesArr} max={memory.mem_total_bytes} format={fmt} />
        <Sparkline label="Buffers" color="#a78bfa" values={buffersArr} timestamps={timesArr} max={memory.mem_total_bytes} format={fmt} />
        <Sparkline label="Cached" color="#eab308" values={cachedArr} timestamps={timesArr} max={memory.mem_total_bytes} format={fmt} />
        <Sparkline label="Swap Free" color="#94a3b8" values={swapFreeArr} timestamps={timesArr} max={memory.swap_total_bytes} format={fmt} />
      </div>
    </div>
  {/if}
</CollapsibleCard>

<style>
  .loading { color: #888; padding: 1rem 0; }
  .error { color: #f87171; padding: 1rem 0; }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 1rem;
    margin-bottom: 1.5rem;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }
  .label {
    font-size: 0.75rem;
    color: #888;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .value {
    font-size: 1.125rem;
    font-weight: 500;
    color: #e0e0e0;
    font-variant-numeric: tabular-nums;
  }
  .history { margin-top: 1.75rem; }
  .group-label {
    display: flex;
    gap: 0.75rem;
    align-items: baseline;
    font-size: 0.85rem;
    font-weight: 600;
    color: #bbb;
    letter-spacing: 0.02em;
    margin-bottom: 0.75rem;
  }
  .group-label .hint {
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
