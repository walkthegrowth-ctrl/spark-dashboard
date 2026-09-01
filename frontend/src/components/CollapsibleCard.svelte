<script>
  import { createEventDispatcher } from 'svelte';

  // A reusable card shell: title (left) + expand/collapse toggle (top-right),
  // with two interchangeable body slots.
  //   `summary` — the most informative indicator(s) for that element, shown
  //               while the card is COLLAPSED.
  //   (default)  — the full expanded content, shown while EXPANDED.
  // Both slots are always mounted (toggled with CSS, not {#if}), so their data
  // streams keep flowing whether or not the card is open — no refetch/teardown.
  export let title = '';
  export let collapsed = false; // expanded by default
  const dispatch = createEventDispatcher();
</script>

<div class="card">
  <div class="card-header">
    <h2>{title}</h2>
    <button
      class="toggle"
      class:collapse-anim={!collapsed}
      type="button"
      aria-expanded={!collapsed}
      aria-label={collapsed ? `Expand ${title} card` : `Collapse ${title} card`}
      on:click={() => dispatch('toggle')}
    >
      <svg class="chev" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <path d="M4 6 L8 10 L12 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path>
      </svg>
    </button>
  </div>

  <div class="summary" class:show={collapsed}>
    <slot name="summary" />
  </div>
  <div class="content" class:hide={collapsed}>
    <slot />
  </div>
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
    gap: 1rem;
  }
  h2 {
    font-size: 1.125rem;
    font-weight: 600;
    color: #fff;
    margin: 0;
  }
  .toggle {
    flex: 0 0 auto;
    width: 30px;
    height: 30px;
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
  /* Expanded (collapse affordance): flip the same chevron to point up (^) so
     both states read as the same "v" glyph. Collapsed (expand affordance): the
     base chevron points down (v). Negative angle = counterclockwise sweep, so
     the tip rotates to the RIGHT (through ">") instead of the left. */
  .collapse-anim .chev { transform: rotate(-180deg); }
  /* Collapsed: hide the full content, show the summary. Expanded: the reverse.
     Both stay in the DOM so live data keeps updating in either state. */
  .summary { display: none; }
  .summary.show { display: block; margin-bottom: 0.25rem; }
  .content.hide { display: none; }
</style>
