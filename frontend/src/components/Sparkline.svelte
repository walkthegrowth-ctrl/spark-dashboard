<script>
  // A small, dependency-free line chart (SVG) for one metric over the current
  // session. `values` (oldest → newest) drive the plot. The y-axis is a FIXED
  // scale — always 0 up to `max` — so a change reads against a stable reference
  // rather than being auto-zoomed (auto-scale made small moves look large).
  // `max` is the metric's natural ceiling (total RAM/swap, 100 °C, 150 W, 100 %).
  export let label = '';
  export let values = [];
  export let timestamps = [];
  export let max = null;
  export let format = (v) => `${v}`;
  export let color = '#38bdf8';
  // Optional override for the left footer label ("Max: xxx"). Defaults to
  // `format(scale_.max)` when not provided. Used by the Memory card to show
  // "Max: NA" for the swap-free chart when the machine has no swap.
  export let maxLabel = null;

  // Unit-space viewBox [0..1]; preserveAspectRatio="none" stretches the path to
  // the container. vector-effect keeps strokes crisp regardless of that stretch.
  const PAD = 0.02;

  $: scaleMax = max && max > 0 ? max : (values.length ? Math.max.apply(null, values) : 1);
  $: scaleMin = 0;

  $: scale_ = (function () {
    const n = values.length;
    if (!n) return null;
    const mn = scaleMin;
    const mx = Math.max(scaleMax, mn + 1); // avoid a zero-height span
    const pts = [];
    for (let i = 0; i < n; i++) {
      const x = n === 1 ? 0.5 : (i / (n - 1)) * (1 - 2 * PAD) + PAD;
      const y = 1 - PAD - ((values[i] - mn) / (mx - mn)) * (1 - 2 * PAD);
      pts.push([x, y]);
    }
    const line = pts.map((p, i) => `${i ? 'L' : 'M'}${(p[0] * 100).toFixed(2)} ${(p[1] * 100).toFixed(2)}`).join(' ');
    const area =
      `M${(PAD * 100).toFixed(2)} 100 L` +
      pts.map((p) => `${(p[0] * 100).toFixed(2)} ${(p[1] * 100).toFixed(2)}`).join(' L ') +
      ` L${(100 - PAD * 100).toFixed(2)} 100 Z`;
    return { min: mn, max: mx, line, area, last: pts[pts.length - 1] };
  })();

  $: current = values.length ? values[values.length - 1] : null;

  // Recording time = span of the in-memory history (newest sample − oldest).
  // Shown bottom-right as "Rec time: H:MM:SS".
  $: recMs = (timestamps.length >= 2)
    ? Math.max(0, timestamps[timestamps.length - 1] - timestamps[0])
    : 0;
  function fmtDuration(ms) {
    const total = Math.max(0, Math.floor(ms / 1000));
    const h = Math.floor(total / 3600);
    const m = Math.floor((total % 3600) / 60);
    const s = total % 60;
    const p = (n) => String(n).padStart(2, '0');
    return `${h}:${p(m)}:${p(s)}`;
  }

  // Left footer label: "Max: <axis max>". Callers can override with
  // `maxLabel` (e.g. the Memory card shows "NA" for swap-free when the
  // machine has no swap, but still plots against a 1 GB scale). `scale_`
  // is null until the first sample arrives, so guard it.
  $: leftMaxLabel = maxLabel !== null ? maxLabel : (scale_ ? `Max: ${format(scale_.max)}` : 'Max: —');

  let container = null;
  let hover = null; // { i, px, py, value }

  function position(e) {
    if (!container || !values.length) return;
    const r = container.getBoundingClientRect();
    const n = values.length;
    const i = Math.min(n - 1, Math.max(0, Math.round(Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)) * (n - 1))));
    const x = n === 1 ? 0.5 : (i / (n - 1)) * (1 - 2 * PAD) + PAD;
    const y = 1 - PAD - ((values[i] - scale_.min) / (scale_.max - scale_.min)) * (1 - 2 * PAD);
    hover = { i, px: x, py: y, value: values[i] };
  }
  function clear() { hover = null; }
  function fmtTime(ms) { return ms ? new Date(ms).toLocaleTimeString() : ''; }
</script>

{#if values.length === 0}
  <div class="chart">
    <div class="head"><span class="name">{label}</span></div>
    <div class="empty">collecting…</div>
  </div>
{:else}
  <div class="chart" bind:this={container} on:mousemove={position} on:mouseleave={clear}>
    <div class="head">
      <span class="name">{label}</span>
      <span class="cur" style="color: {color}">{format(current)}</span>
    </div>

    <div class="plot">
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" class="svg" aria-hidden="true">
        <line x1="0" y1="25" x2="100" y2="25" class="grid"></line>
        <line x1="0" y1="50" x2="100" y2="50" class="grid"></line>
        <line x1="0" y1="75" x2="100" y2="75" class="grid"></line>
        <path d={scale_.area} class="area" style="fill: {color}"></path>
        <path d={scale_.line} class="line" style="stroke: {color}"></path>
        {#if values.length > 1}
          <circle cx={scale_.last[0] * 100} cy={scale_.last[1] * 100} r="2" class="dot" style="fill: {color}"></circle>
        {/if}
        {#if hover}
          <line x1={hover.px * 100} y1="0" x2={hover.px * 100} y2="100" class="cursor"></line>
          <circle cx={hover.px * 100} cy={hover.py * 100} r="2.4" class="hoverdot" style="fill: {color}"></circle>
        {/if}
      </svg>
      {#if hover}
        <div class="tip" style="left: {Math.min(82, Math.max(18, hover.px * 100))}%">
          <span style="color: {color}">{format(hover.value)}</span>
          <span class="tip-time">{fmtTime(timestamps[hover.i])}</span>
        </div>
      {/if}
    </div>

    <div class="range">
      <span>{leftMaxLabel}</span>
      <span>Rec time: {fmtDuration(recMs)}</span>
    </div>
  </div>
{/if}

<style>
  .chart { display: flex; flex-direction: column; gap: 0.4rem; padding: 0.5rem 0.6rem; background: #0a0a0a; border: 1px solid #222; border-radius: 6px; min-width: 0; }
  .head { display: flex; justify-content: space-between; align-items: baseline; gap: 0.5rem; }
  .name { font-size: 0.7rem; color: #888; text-transform: uppercase; letter-spacing: 0.05em; }
  .cur { font-size: 0.9rem; font-weight: 600; font-variant-numeric: tabular-nums; color: #e5e7eb; }
  .empty { color: #555; font-size: 0.75rem; text-align: center; padding: 1.2rem 0; }
  .plot { position: relative; height: 64px; }
  .svg { width: 100%; height: 100%; display: block; overflow: visible; }
  .grid { stroke: #1c1c1c; stroke-width: 0.5; vector-effect: non-scaling-stroke; }
  .area { fill-opacity: 0.12; stroke: none; }
  .line { fill: none; stroke-width: 1.6; vector-effect: non-scaling-stroke; stroke-linejoin: round; stroke-linecap: round; }
  .dot { stroke: #0a0a0a; stroke-width: 0.6; }
  .cursor { stroke: #556; stroke-width: 1; vector-effect: non-scaling-stroke; }
  .hoverdot { stroke: #0a0a0a; stroke-width: 0.8; }
  .tip { position: absolute; top: -4px; transform: translateX(-50%); background: #161616; border: 1px solid #444; border-radius: 4px; padding: 2px 6px; font-size: 0.68rem; display: flex; gap: 6px; pointer-events: none; white-space: nowrap; z-index: 5; box-shadow: 0 2px 8px rgba(0, 0, 0, 0.5); }
  .tip-time { color: #888; }
  .range { display: flex; justify-content: space-between; font-size: 0.62rem; color: #555; font-variant-numeric: tabular-nums; }
</style>
