<script>
  import { createEventDispatcher, tick } from 'svelte';

  // A reusable card shell that provides:
  //   - A top-level toggle that collapses / expands the whole card (with an
  //     animated height transition on the body wrapper).
  //   - Two mutually-exclusive panels inside the body:
  //        `summary` slot  — shown when the card is collapsed (a one-line
  //                          indicator).
  //        default slot    — shown when the card is expanded. Compose
  //                          CollapsibleSection components here for
  //                          per-section collapse/expand; the parent owns
  //                          their state and is responsible for resetting
  //                          them on a top-toggle click (see on:toggle).
  //
  // Props:
  //   title:      string
  //   collapsed:  bool — true = card collapsed (summary visible).
  //
  // Behaviour:
  //   - Clicking the top toggle dispatches 'toggle'. The parent flips
  //     `collapsed` (which Svelte then applies as a class on the next tick).
  //   - The body animates from its pre-toggle on-screen height to its new
  //     natural height (measured after Svelte applies the class flip).
  //   - prefers-reduced-motion: skip the animation, change state instantly.
  export let title = '';
  export let collapsed = false;

  const dispatch = createEventDispatcher();

  let bodyRef = null;
  const DURATION_MS = 220;
  const reducedMotion =
    typeof window !== 'undefined' &&
    window.matchMedia &&
    window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  // Per-body animation tokens so a rapid re-click doesn't double-run a
  // transition from a stale measurement.
  const animTokens = new Map();
  function nextToken(el) {
    const n = (animTokens.get(el) || 0) + 1;
    animTokens.set(el, n);
    return n;
  }
  function isCurrent(el, n) { return animTokens.get(el) === n; }

  function settle(el) {
    if (!el) return;
    el.style.height = 'auto';
    el.style.overflow = '';
    el.style.transitionDuration = '';
    el.style.transitionTimingFunction = '';
  }

  // Animate `el.height` from its pre-toggle on-screen height to its new
  // natural height (measured after Svelte has applied the class flip).
  // A double rAF forces the browser to commit the "from" state before
  // transitioning; a transitionend + setTimeout safety net guarantees the
  // inline overrides are cleared even if the tab is hidden mid-animation.
  async function animateHeight(el, beforeH) {
    const my = nextToken(el);
    await tick();
    if (!isCurrent(el, my) || !el) return;

    el.style.height = 'auto';
    const toH = el.getBoundingClientRect().height;
    if (toH === beforeH) { settle(el); return; }

    el.style.height = beforeH + 'px';
    el.style.overflow = 'hidden';
    el.style.transitionDuration = '0ms';

    requestAnimationFrame(function f1() {
      if (!isCurrent(el, my)) return;
      requestAnimationFrame(function f2() {
        if (!isCurrent(el, my) || !el) return;
        el.style.transitionDuration = DURATION_MS + 'ms';
        el.style.transitionTimingFunction = 'ease-in-out';
        el.style.height = toH + 'px';
      });
    });

    const onEnd = (e) => {
      if (!isCurrent(el, my) || e.propertyName !== 'height') return;
      el.removeEventListener('transitionend', onEnd);
      settle(el);
    };
    el.addEventListener('transitionend', onEnd);
    setTimeout(() => {
      if (isCurrent(el, my)) {
        el.removeEventListener('transitionend', onEnd);
        settle(el);
      }
    }, DURATION_MS + 120);
  }

  function toggleTop() {
    dispatch('toggle');
    if (reducedMotion || !bodyRef) return;
    const beforeH = bodyRef.getBoundingClientRect().height;
    animateHeight(bodyRef, beforeH);
  }
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
      on:click={toggleTop}
    >
      <svg class="chev" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true">
        <path d="M4 6 L8 10 L12 6" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"></path>
      </svg>
    </button>
  </div>

  <div class="body" bind:this={bodyRef}>
    <div class="panel" class:active={collapsed} data-panel="summary">
      <slot name="summary" />
    </div>
    <div class="panel" class:active={!collapsed} data-panel="main">
      <slot />
    </div>
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
  .collapse-anim .chev { transform: rotate(-180deg); }

  /* Two mutually-exclusive panels inside the body. The inactive one is
     display:none (zero layout contribution); the active one is block. The
     body's height is always the active panel's natural height, and the JS
     animation transitions the body's height between them. */
  .body { display: block; }
  .panel { display: none; }
  .panel.active { display: block; }

  @media (prefers-reduced-motion: reduce) {
    .chev { transition: none; }
    .toggle { transition: none; }
  }
</style>
