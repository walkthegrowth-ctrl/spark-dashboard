<script>
  import { createEventDispatcher } from 'svelte';

  // A reusable collapsible region. The parent composes it anywhere (inside a
  // card's slot, in a list, etc.) and keeps its `open` state in sync by
  // listening to the `toggle` event it dispatches.
  //
  // Props:
  //   title:      string (optional) — a label shown to the LEFT of the
  //             chevron in the header row. Omit when the content has its own
  //             visible heading.
  //   hint:       string (optional) — a lighter secondary label shown beside
  //             the title (e.g. a scale note).
  //   open:       bool — the current open/closed state. The parent is the
  //             source of truth (keeps it in sync via on:toggle). When true,
  //             the content area is shown (natural height); when false, it's
  //             collapsed (height 0).
  //
  // The header (title + chevron) is ALWAYS visible and sits at the top of the
  // region, with the chevron on the top-right. The content below it animates
  // between 0 and its natural height. This mirrors how the parent card's top
  // toggle reads (control at top-right) and lets sub-sections cascade
  // downward naturally.
  //
  // Dispatches:
  //   on:toggle   — fires after the chevron is clicked; the parent should
  //             flip its `open` value (e.g. on:toggle={() => (open = !open)}).
  export let title = '';
  export let hint = '';
  export let open = false;

  const dispatch = createEventDispatcher();

  function onToggle() {
    dispatch('toggle');
  }
</script>

<div class="section" class:open={open}>
  <!-- Header: always visible. Title (left) + chevron (top-right). -->
  <div class="section-header">
    {#if title}
      <span class="section-title">
        {title}
        {#if hint}<span class="hint">{hint}</span>{/if}
      </span>
    {/if}
    <button
      class="toggle"
      type="button"
      aria-expanded={open}
      aria-label={open ? `Collapse ${title || 'section'}` : `Expand ${title || 'section'}`}
      on:click={onToggle}
    >
      <svg class="chev" viewBox="0 0 16 16" width="14" height="14" aria-hidden="true">
        <path d="M4 6 L8 10 L12 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path>
      </svg>
    </button>
  </div>

  <!-- Animating content (the area below the header). -->
  <div class="content-wrapper">
    <div class="content-inner">
      <slot />
    </div>
  </div>

  <style>
    .section { display: block; }

    /* Header row: title on the left, chevron pinned to the top-right. */
    .section-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 0.75rem;
    }
    .section-title {
      flex: 1 1 auto;
      display: flex;
      align-items: baseline;
      gap: 0.75rem;
      font-size: 0.85rem;
      font-weight: 600;
      color: #bbb;
      letter-spacing: 0.02em;
    }
    .hint {
      font-size: 0.68rem;
      font-weight: 400;
      color: #666;
      letter-spacing: 0;
    }

    /* Content area: a single-row grid. Collapsed = 0fr (height 0); open = 1fr
       (natural height). The inner element must have min-height: 0 + overflow
       hidden so it clips at 0fr; when open we switch to overflow: visible so
       content (tooltips, hover popups) can render normally. */
    .content-wrapper {
      display: grid;
      grid-template-rows: 0fr;
      transition: grid-template-rows 220ms ease-in-out;
    }
    .content-inner {
      min-height: 0;
      overflow: hidden;
    }
    .section.open .content-wrapper { grid-template-rows: 1fr; }
    .section.open .content-inner { overflow: visible; }

    /* Chevron: points down (v) when closed = "click to expand"; rotates up (^)
       when open = "click to collapse". Same 26px button style as the card's
       top toggle for visual consistency. */
    .toggle {
      flex: 0 0 auto;
      width: 26px;
      height: 26px;
      display: inline-flex;
      align-items: center;
      justify-content: center;
      color: #9aa0a6;
      background: transparent;
      border: 1px solid #2a2a2a;
      border-radius: 6px;
      cursor: pointer;
      transition: color 0.15s ease, border-color 0.15s ease, background 0.15s ease;
    }
    .toggle:hover { color: #e5e7eb; border-color: #3a3a3a; background: #161616; }
    .toggle:focus-visible { outline: 2px solid #3b82f6; outline-offset: 2px; }
    .chev { transition: transform 0.18s ease; }
    .open .toggle .chev { transform: rotate(-180deg); }

    @media (prefers-reduced-motion: reduce) {
      .content-wrapper { transition: none; }
      .toggle { transition: none; }
      .chev { transition: none; }
    }
  </style>
</div>
