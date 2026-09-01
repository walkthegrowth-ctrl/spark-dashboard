<script>
  import { onMount } from 'svelte';
  import CollapsibleCard from './CollapsibleCard.svelte';
  import StatusBar from './StatusBar.svelte';
  import Sparkline from './Sparkline.svelte';
  import { touch } from '../lib/lastUpdated.js';

  let thermals = null;
  let power = null;
  let loading = true;
  let error = null;
  let collapsed = false; // expanded by default

  // In-memory session history for the two "status" metrics shown in the bars:
  // the hottest sensor temperature, and power draw. One sample per refresh (1 s),
  // capped, each a top-level reactive `let` (fresh array per tick) so the chart
  // props re-fire reliably.
  const MAX_SAMPLES = 3600;
  let hottestArr = [];
  let powerArr = [];
  let thermalTimes = [];
  let powerTimes = [];

  function append(arr, v) {
    const next = arr.concat(v);
    return next.length > MAX_SAMPLES ? next.slice(next.length - MAX_SAMPLES) : next;
  }

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
      pushThermalSample(thermals);
      touch();
    } catch (e) {
      error = e.message;
    } finally {
      loading = false;
    }
  }

  // Power is fetched independently so a missing GPU-power reading never masks a
  // thermal failure (and vice-versa). A 503 is treated as "no data yet".
  async function fetchPower() {
    try {
      const res = await fetch('/api/power/current');
      if (res.status === 503) {
        power = null;
        return;
      }
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      power = await res.json();
      pushPowerSample(power);
      touch();
    } catch (e) {
      power = null;
    }
  }

  function formatTemp(celsius) {
    if (celsius === null || celsius === undefined) return 'N/A';
    return `${celsius.toFixed(1)} °C`;
  }

  function formatPower(data) {
    if (!data || data.power_w === null || data.power_w === undefined) return 'N/A';
    return `${(data.power_w).toFixed(2)} W`;
  }

  function powerColor(data) {
    const w = data && data.power_w;
    if (w === null || w === undefined) return '#888';
    if (w < 80) return '#22c55e';
    if (w < 110) return '#eab308';
    if (w < 130) return '#f97316';
    return '#ef4444';
  }

  function powerStatus(data) {
    const w = data && data.power_w;
    if (w === null || w === undefined) return 'Unknown';
    if (w < 80) return 'Normal';
    if (w < 110) return 'Elevated';
    if (w < 130) return 'High';
    return 'Peak';
  }

  function isCore(thermal) {
    return thermal.sensor_type === 'nvidia' || thermal.sensor_type === 'acpitz';
  }

  // SoC/GPU/board: GB10 throttles around 95 C and incidents occur at 94-97 C.
  function coreColor(temp) {
    if (temp === null || temp === undefined) return '#888';
    if (temp < 80) return '#22c55e';
    if (temp < 90) return '#eab308';
    if (temp < 95) return '#f97316';
    return '#ef4444';
  }

  // Peripherals (NVMe, WiFi PHY): key relative to their own critical trip point.
  function peripheralColor(temp, trip) {
    if (temp === null || temp === undefined) return '#888';
    if (trip && trip > temp) {
      const m = trip - temp;
      if (m < 10) return '#f97316';
      if (m < 25) return '#eab308';
    }
    if (temp >= 85) return '#ef4444';
    if (temp >= 70) return '#eab308';
    return '#22c55e';
  }

  function sensorColor(thermal) {
    const temp = thermal.temperature_celsius;
    if (isCore(thermal)) return coreColor(temp);
    return peripheralColor(temp, thermal.trip_point_temp_celsius);
  }

  function sensorStatus(thermal) {
    const temp = thermal.temperature_celsius;
    if (temp === null || temp === undefined) return 'Unknown';
    if (isCore(thermal)) {
      if (temp < 80) return 'Normal';
      if (temp < 90) return 'Warm';
      if (temp < 95) return 'Hot';
      return 'Critical';
    }
    const trip = thermal.trip_point_temp_celsius;
    if (trip && temp >= trip - 10) return 'Critical';
    if (temp >= 85) return 'Hot';
    if (temp >= 70) return 'Warm';
    return 'Normal';
  }

  function displayLabel(thermal) {
    if (thermal.sensor_type === 'acpitz') {
      return thermal.sensor_label || `Zone ${thermal.zone}`;
    }
    if (thermal.sensor_type === 'nvidia') {
      return 'GPU';
    }
    if (thermal.sensor_type === 'nvme') {
      const n = thermal.zone.match(/(\d+)$/);
      return `NVMe${n ? n[1] : 1}`;
    }
    if (/wifi|phy|wl|mt\d/.test(thermal.sensor_type)) {
      return 'WiFi';
    }
    if (thermal.sensor_label && thermal.sensor_label !== thermal.zone) {
      return thermal.sensor_label;
    }
    return thermal.sensor_type;
  }

  function tooltipText(thermal) {
    const parts = [];
    parts.push(`${thermal.sensor_type} / ${thermal.zone}`);
    if (thermal.trip_point_type && thermal.trip_point_temp_celsius) {
      parts.push(`${thermal.trip_point_type}: ${thermal.trip_point_temp_celsius.toFixed(1)} °C`);
    }
    if (thermal.sensor_label) {
      parts.push(thermal.sensor_label);
    }
    return parts.join(' · ');
  }

  // Stable, identity-based ordering. Each sensor keeps its slot by (sensor_type,
  // zone) identity — never by live temperature — so there's no shuffling.
  function zoneNum(thermal) {
    const m = String(thermal.zone).match(/(\d+)/);
    return m ? parseInt(m[1], 10) : Number.MAX_SAFE_INTEGER;
  }

  // SoC layout is two fixed rows (a dashboard for fixed hardware, not a feed):
  //   Row 1: Board package, GPU die (SoC), GPU, [gap], Power — right-aligned
  //   Row 2: the remaining acpitz SoC zones, strict numerical order
  function socRow(thermal) {
    if (thermal.sensor_type === 'nvidia') return 1;
    if (thermal.sensor_type === 'acpitz') {
      const lab = thermal.sensor_label || '';
      if (lab.startsWith('Board') || lab.startsWith('GPU die')) return 1;
      return 2;
    }
    return 2;
  }

  function socRank(thermal) {
    if (thermal.sensor_type === 'nvidia') return [1, 2];
    const lab = thermal.sensor_label || '';
    if (lab.startsWith('Board')) return [1, 0];
    if (lab.startsWith('GPU die')) return [1, 1];
    return [2, zoneNum(thermal)];
  }

  function peripheralRank(thermal) {
    if (thermal.sensor_type === 'nvme') return [0, zoneNum(thermal)];
    if (/wifi|phy|wl|mt\d/.test(thermal.sensor_type)) return [1, zoneNum(thermal)];
    return [2, zoneNum(thermal)];
  }

  function compareRank(a, b) {
    if (a[0] !== b[0]) return a[0] - b[0];
    if (a[1] !== b[1]) return a[1] - b[1];
    return 0;
  }

  $: allSensors = thermals || [];

  $: coreSensors = allSensors.filter(isCore).sort((a, b) => compareRank(socRank(a), socRank(b)));
  $: socPrimary = coreSensors.filter((t) => socRow(t) === 1);
  $: socZones = coreSensors.filter((t) => socRow(t) === 2);

  $: powerColorValue = powerColor(power);

  $: peripheralSensors = allSensors.filter((t) => !isCore(t)).sort((a, b) => compareRank(peripheralRank(a), peripheralRank(b)));

  // Highest temperature across ALL displayed thermal boxes (core + peripherals).
  // Used for the collapsed "Hottest" bar; colour comes from that sensor's own
  // core/peripheral scale so it stays meaningful.
  $: hottest = allSensors.reduce((acc, s) => {
    const t = s.temperature_celsius;
    if (t === null || t === undefined) return acc;
    if (acc.temp === null || t > acc.temp) return { temp: t, sensor: s };
    return acc;
  }, { temp: null, sensor: null });

  $: hottestColor = hottest.sensor ? sensorColor(hottest.sensor) : null;
  $: hottestDisplay = hottest.temp !== null ? `${hottest.temp.toFixed(1)} °C` : 'N/A';

  // Power fill scales against a 150 W ceiling (GB10 peak headroom); the value
  // itself is what matters, colour is a convenience.
  $: powerFill = power && power.power_w !== null ? (power.power_w / 150) * 100 : 0;
  $: powerTooltip = power && power.source ? `nvidia-smi power.draw — unified GB10 package (GPU + ARM + memory)` : 'nvidia-smi power.draw';

  // Fixed chart ceilings — the natural top of each axis, so history reads
  // against a stable reference (not auto-zoomed).
  const TEMP_MAX = 100; // °C — covers GB10 throttling (~95 °C) with headroom
  const POWER_MAX = 150; // W — GB10 peak headroom

  function hottestOf(list) {
    let best = null;
    for (const s of list || []) {
      const t = s.temperature_celsius;
      if (t !== null && t !== undefined && (best === null || t > best)) best = t;
    }
    return best;
  }

  function pushThermalSample(list) {
    const h = hottestOf(list);
    if (h === null) return;
    hottestArr = append(hottestArr, h);
    thermalTimes = append(thermalTimes, Date.now());
  }

  function pushPowerSample(d) {
    const w = d && d.power_w;
    if (w === null || w === undefined) return;
    powerArr = append(powerArr, w);
    powerTimes = append(powerTimes, Date.now());
  }

  function fmtTemp(v) {
    if (v === null || v === undefined) return 'N/A';
    const isCeiling = Number.isInteger(v); // fixed 0 and 100 → round
    return `${isCeiling ? v : v.toFixed(1)} °C`;
  }
  function fmtW(v) {
    if (v === null || v === undefined) return 'N/A';
    const isCeiling = Number.isInteger(v); // fixed 0 and 150 → round
    return `${isCeiling ? v : v.toFixed(1)} W`;
  }

  onMount(() => {
    fetchThermal();
    fetchPower();
  });

  const interval = setInterval(() => {
    fetchThermal();
    fetchPower();
  }, 1000);
</script>

<CollapsibleCard
  title="Thermal and Power"
  {collapsed}
  on:toggle={() => (collapsed = !collapsed)}
>
  <svelte:fragment slot="summary">
    <StatusBar
      label="Hottest"
      value={hottest.temp != null ? hottest.temp : 0}
      color={hottestColor}
      display={hottestDisplay}
      tooltip={hottest.sensor ? `${displayLabel(hottest.sensor)} · ${tooltipText(hottest.sensor)}` : 'Highest temperature across all sensors.'}
    />
    <StatusBar
      label="Power"
      value={powerFill}
      color={powerColorValue}
      display={power && power.power_w != null ? formatPower(power) : 'N/A'}
      tooltip={powerTooltip}
    />
  </svelte:fragment>

  {#if loading && !thermals}
    <div class="loading">Loading...</div>
  {:else if error}
    <div class="error">{error}</div>
  {:else if thermals}
    {#if coreSensors.length}
      <div class="group-label">SoC (GB10)</div>
      {#if socPrimary.length}
        <div class="stats stats-primary">
          {#each socPrimary as thermal}
            <div class="stat" data-tooltip="{tooltipText(thermal)}" style="border-color: {sensorColor(thermal)}">
              <span class="label">{displayLabel(thermal)}</span>
              <span class="value" style="color: {sensorColor(thermal)}">
                {formatTemp(thermal.temperature_celsius)}
              </span>
              <span class="status" style="color: {sensorColor(thermal)}">
                {sensorStatus(thermal)}
              </span>
            </div>
          {/each}
          <div class="stat-spacer" aria-hidden="true"></div>
          <div class="stat stat-power" data-tooltip="nvidia-smi power.draw — unified GB10 package (GPU + ARM + memory)" style="border-color: {powerColorValue}">
            <span class="label">Power</span>
            <span class="value" style="color: {powerColorValue}">{formatPower(power)}</span>
            <span class="status" style="color: {powerColorValue}">{powerStatus(power)}</span>
          </div>
        </div>
      {/if}
      {#if socZones.length}
        <div class="stats" style="margin-top: 0">
          {#each socZones as thermal}
            <div class="stat" data-tooltip="{tooltipText(thermal)}" style="border-color: {sensorColor(thermal)}">
              <span class="label">{displayLabel(thermal)}</span>
              <span class="value" style="color: {sensorColor(thermal)}">
                {formatTemp(thermal.temperature_celsius)}
              </span>
              <span class="status" style="color: {sensorColor(thermal)}">
                {sensorStatus(thermal)}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    {/if}

    {#if peripheralSensors.length}
      <div class="group-label">Peripherals</div>
      <div class="stats">
        {#each peripheralSensors as thermal}
          <div class="stat" data-tooltip="{tooltipText(thermal)}" style="border-color: {sensorColor(thermal)}">
            <span class="label">{displayLabel(thermal)}</span>
            <span class="value" style="color: {sensorColor(thermal)}">
              {formatTemp(thermal.temperature_celsius)}
            </span>
            <span class="status" style="color: {sensorColor(thermal)}">
              {sensorStatus(thermal)}
            </span>
          </div>
        {/each}
      </div>
    {/if}

    <div class="bars">
      <StatusBar
        label="Hottest"
        value={hottest.temp != null ? hottest.temp : 0}
        color={hottestColor}
        display={hottestDisplay}
        tooltip={hottest.sensor ? `${displayLabel(hottest.sensor)} · ${tooltipText(hottest.sensor)}` : 'Highest temperature across all sensors.'}
      />
      <StatusBar
        label="Power"
        value={powerFill}
        color={powerColorValue}
        display={power && power.power_w != null ? formatPower(power) : 'N/A'}
        tooltip={powerTooltip}
      />
    </div>

    <div class="history">
      <div class="group-label">
        Time dynamics
        <span class="hint">since this page opened · fixed scale (0 → 100 °C, 0 → 150 W)</span>
      </div>
      <div class="charts">
        <Sparkline label="Hottest" color="#f97316" values={hottestArr} timestamps={thermalTimes} max={TEMP_MAX} format={fmtTemp} />
        <Sparkline label="Power" color="#38bdf8" values={powerArr} timestamps={powerTimes} max={POWER_MAX} format={fmtW} />
      </div>
    </div>
  {/if}
</CollapsibleCard>

<style>
  .loading { color: #888; padding: 1rem 0; }
  .error { color: #f87171; padding: 1rem 0; }
  .group-label {
    font-size: 0.75rem;
    font-weight: 600;
    color: #666;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 0.5rem;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 0.75rem;
    margin-bottom: 1.5rem;
  }
  /* Row 1: fixed four columns — three thermal boxes, a spacer gap, then power
     right-aligned. `1fr` on the spacer creates the visible empty space. */
  .stats-primary {
    grid-template-columns: minmax(120px, 1fr) minmax(120px, 1fr) minmax(120px, 1fr) 1fr repeat(1, minmax(120px, 1fr));
  }
  .stat-spacer {
    /* Intentionally empty; creates the 4th "column" between the three cores
       and the power box, so the row reads as 4 slots with a gap. */
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
    color: #999;
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
  .bars {
    margin-top: 1rem;
    border-top: 1px solid #222;
    padding-top: 1rem;
  }
  .history {
    margin-top: 1.75rem;
    border-top: 1px solid #222;
    padding-top: 1rem;
  }
  .history .group-label {
    display: flex;
    gap: 0.75rem;
    align-items: baseline;
    font-size: 0.85rem;
    font-weight: 600;
    color: #bbb;
    letter-spacing: 0.02em;
    margin-bottom: 0.75rem;
    text-transform: none; /* this card's base .group-label is uppercase; opt out here */
  }
  .history .hint {
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
