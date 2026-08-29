<script>
  import { onMount } from 'svelte';

  let { apiBase = '' } = $props();

  let memory = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let lastUpdate = $state(null);

  async function fetchMemory() {
    try {
      loading = true;
      error = null;
      const res = await fetch(`${apiBase}/api/memory/current`);
      if (!res.ok) {
        if (res.status === 503) {
          error = 'No data available yet. Waiting for collector...';
          return;
        }
        throw new Error(`HTTP ${res.status}`);
      }
      memory = await res.json();
      lastUpdate = new Date();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function formatBytes(bytes) {
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

  function formatTimestamp(ts) {
    if (!ts) return 'N/A';
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString();
  }

  function getPercentage() {
    if (!memory || !memory.mem_available_bytes || !memory.mem_total_bytes || memory.mem_total_bytes === 0) return 0;
    return Math.round((memory.mem_available_bytes / memory.mem_total_bytes) * 100);
  }

  onMount(fetchMemory);

  const interval = setInterval(fetchMemory, 5000);
</script>

<div class="card">
  <div class="card-header">
    <h2>Memory</h2>
    {#if lastUpdate}
      <span class="updated">Updated {formatTimestamp(lastUpdate.getTime() / 1000)}</span>
    {/if}
  </div>

  {#if loading && !memory}
    <div class="loading">Loading...</div>
  {:else if error}
    <div class="error">{error}</div>
  {:else if memory}
    <div class="stats">
      <div class="stat">
        <span class="label">Total</span>
        <span class="value">{formatBytes(memory.mem_total_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Available</span>
        <span class="value">{formatBytes(memory.mem_available_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Free</span>
        <span class="value">{formatBytes(memory.mem_free_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Buffers</span>
        <span class="value">{formatBytes(memory.buffers_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Cached</span>
        <span class="value">{formatBytes(memory.cached_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Swap Total</span>
        <span class="value">{formatBytes(memory.swap_total_bytes)}</span>
      </div>
      <div class="stat">
        <span class="label">Swap Free</span>
        <span class="value">{formatBytes(memory.swap_free_bytes)}</span>
      </div>
    </div>

    <div class="bar-container">
      <div class="bar-label">
        <span>Available</span>
        <span>{getPercentage()}%</span>
      </div>
      <div class="bar">
        <div class="bar-fill" style="width: {getPercentage()}%"></div>
      </div>
    </div>
  {/if}
</div>

<style>
  .card {
    background: #111;
    border: 1px solid #222;
    border-radius: 8px;
    padding: 1.5rem;
  }
  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 1rem;
  }
  h2 {
    font-size: 1.125rem;
    font-weight: 600;
    color: #fff;
  }
  .updated {
    font-size: 0.75rem;
    color: #666;
  }
  .loading {
    color: #888;
    padding: 1rem 0;
  }
  .error {
    color: #f87171;
    padding: 1rem 0;
  }
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
  .bar-container {
    margin-top: 1rem;
  }
  .bar-label {
    display: flex;
    justify-content: space-between;
    font-size: 0.75rem;
    color: #888;
    margin-bottom: 0.5rem;
  }
  .bar {
    height: 8px;
    background: #222;
    border-radius: 4px;
    overflow: hidden;
  }
  .bar-fill {
    height: 100%;
    background: #22c55e;
    border-radius: 4px;
    transition: width 0.3s ease;
  }
</style>