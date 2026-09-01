<script>
  // One labelled progress bar — a card's most informative indicator when
  // collapsed, and also reused as a footer bar in the expanded views.
  export let label = '';
  export let value = 0;      // 0–100 fill percentage
  export let color = null;   // hex, or null when there's no data
  export let display = '';   // right-side value text
  export let tooltip = '';
  $: width = Math.max(0, Math.min(Number(value) || 0, 100));
</script>

<div class="bar" data-tooltip={tooltip || undefined}>
  <div class="bar-label">
    <span>{label}</span>
    <span class="bar-val" style={color ? `color: ${color}` : ''}>{display}</span>
  </div>
  <div class="track"><div class="fill" style={`width: ${width}%; background: ${color || '#222'}`}></div></div>
</div>

<style>
  .bar { margin: 0 0 0.9rem; cursor: default; position: relative; }
  .bar[data-tooltip]:hover::after {
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
    box-shadow: 0 2px 8px rgba(0,0,0,0.5);
  }
  .bar-label { display: flex; justify-content: space-between; font-size: 0.75rem; color: #888; margin-bottom: 0.4rem; }
  .bar-val { font-variant-numeric: tabular-nums; color: #ccc; }
  .track { height: 8px; background: #222; border-radius: 4px; overflow: hidden; }
  .fill { height: 100%; border-radius: 4px; transition: width 0.4s ease, background 0.4s ease; }
</style>
