<script>
  import { onMount } from 'svelte';

  let thermals = null;
  let loading = true;
  let error = null;
  let lastUpdate = null;

  async function fetchThermal() {
    try {
      loading = true;
      error = null;
      const res = await fetch('/api/thermal/current');
      if (!res.ok) {
        if (res.status === 503) {
          error = 'No data available yet. Waiting for collector...';
          return;
        }
        throw new Error(`HTTP ${res.status}`);
      }
      thermals = await res.json();
      lastUpdate = new Date();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  function formatTemp(celsius) {
    if (celsius === null || celsius === undefined) return 'N/A';
    return `${celsius.toFixed(1)} °C`;
  }

  function formatTimestamp(ts) {
    if (!ts) return 'N/A';
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString();
  }

  function tempColor(temp) {
    if (temp === null || temp === undefined) return '#888';
    if (temp < 50) return '#22c55e';
    if (temp < 70) return '#eab308';
    if (temp < 85) return '#f97316';
    return '#ef4444';
  }

  function tempLabel(temp) {
    if (temp === null || temp === undefined) return 'Unknown';
    if (temp < 40) return 'Cold';
    if (temp < 60) return 'Normal';
    if (temp < 75) return 'Warm';
    if (temp < 85) return 'Hot';
    return 'Critical';
  }

  function displayLabel(thermal) {
    if (thermal.sensor_type === 'acpitz' && !thermal.zone.includes('_')) {
      return `Zone ${thermal.zone}`;
    }
    const device = thermal.sensor_type.charAt(0).toUpperCase() + thermal.sensor_type.slice(1);
    if (thermal.sensor_label) {
      return `${device} ${thermal.sensor_label}`;
    }
    return thermal.zone;
  }

  function tooltipText(thermal) {
    const parts = [];
    parts.push(`${thermal.sensor_type} / ${thermal.zone}`);
    if (thermal.trip_point_type && thermal.trip_point_temp_celsius) {
      parts.push(`${thermal.trip_point_type}: ${thermal.trip_point_temp_celsius.toFixed(1)} °C`);
    }
    return parts.join(' · ');
  }

  $: maxTemp = thermals
    ? Math.max(...thermals.map(t => t.temperature_celsius ?? 0))
    : 0;

  onMount(fetchThermal);

  const interval = setInterval(fetchThermal, 1000);
</script>

<div class="card">
  <div class="card-header">
    <h2>Thermal</h2>
    {#if lastUpdate}
      <span class="updated">Updated {formatTimestamp(lastUpdate.getTime() / 1000)}</span>
    {/if}
  </div>

  {#if loading && !thermals}
    <div class="loading">Loading...</div>
  {:else if error}
    <div class="error">{error}</div>
  {:else if thermals}
    <div class="stats">
      {#each thermals as thermal}
        <div class="stat" data-tooltip="{tooltipText(thermal)}" style="border-color: {tempColor(thermal.temperature_celsius)}">
          <span class="label">{displayLabel(thermal)}</span>
          <span class="value" style="color: {tempColor(thermal.temperature_celsius)}">
            {formatTemp(thermal.temperature_celsius)}
          </span>
          <span class="status" style="color: {tempColor(thermal.temperature_celsius)}">
            {tempLabel(thermal.temperature_celsius)}
          </span>
        </div>
      {/each}
    </div>

    <div class="bar-container">
      <div class="bar-label">
        <span>Peak Temperature</span>
        <span style="color: {tempColor(maxTemp)}">{formatTemp(maxTemp)}</span>
      </div>
      <div class="bar">
        <div class="bar-fill" style="width: {Math.min(maxTemp / 100 * 100, 100)}%; background: {tempColor(maxTemp)}"></div>
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
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
    gap: 0.75rem;
    margin-bottom: 1.5rem;
  }
  .stat {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    padding: 0.75rem;
    border-radius: 6px;
    border: 1px solid #333;
    background: #0a0a0a;
    position: relative;
    cursor: default;
  }
  .label {
    font-size: 0.65rem;
    color: #666;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .value {
    font-size: 1rem;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
  }
  .status {
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .stat[data-tooltip]:hover::after {
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
  .stat[data-tooltip]:hover::before {
    content: '';
    position: absolute;
    bottom: calc(100% + 2px);
    left: 50%;
    transform: translateX(-50%);
    border: 4px solid transparent;
    border-top-color: #444;
    z-index: 10;
    pointer-events: none;
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
    border-radius: 4px;
    transition: width 0.3s ease, background 0.3s ease;
  }
</style>
