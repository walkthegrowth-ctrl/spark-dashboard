<script>
  import MemoryStats from './components/MemoryStats.svelte';
  import ThermalStats from './components/ThermalStats.svelte';
  import ComputeStats from './components/ComputeStats.svelte';
  import { lastUpdated } from './lib/lastUpdated.js';

  function formatTime(ms) {
    if (!ms) return 'waiting for data…';
    return new Date(ms).toLocaleTimeString();
  }

  // Opens the about/licensing page in a new tab. Using window.open (a user
  // gesture) rather than a plain <a target=_blank> so the child tab is
  // script-opened — which in turn lets it call window.close() to return
  // focus to the original dashboard tab (which keeps its live session).
  // The <a> href is preserved as a no-JS fallback.
  function onClickOpenAbout(e) {
    e.preventDefault();
    const w = window.open('/about', '_blank');
    if (!w) location.href = '/about'; // popup blocked: navigate current tab
  }
</script>

<div class="container">
  <header>
    <div class="title">
      <h1>Spark Dashboard</h1>
      <p class="subtitle">NVIDIA DGX Spark Monitoring</p>
    </div>
  </header>

  <main>
    <MemoryStats />
    <ThermalStats />
    <ComputeStats />
  </main>

  <footer class="trailer">
    <a
      class="trailer-link"
      href="/about"
      on:click={onClickOpenAbout}
    >Notes and Licensing</a>
    <span class="updated" data-lastupdated={$lastUpdated}>Updated {formatTime($lastUpdated)}</span>
    <a class="trailer-link" href="http://localhost:11000/" target="_blank" rel="noopener">Original NVIDIA dashboard</a>
  </footer>
</div>

<style>
  .container {
    max-width: 800px;
    margin: 0 auto;
  }
  header {
    margin-bottom: 2rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid #333;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }
  .title { min-width: 0; }
  h1 {
    font-size: 1.5rem;
    font-weight: 600;
    color: #fff;
  }
  .subtitle {
    font-size: 0.875rem;
    color: #888;
    margin-top: 0.25rem;
  }
  main > * + * {
    margin-top: 1.5rem;
  }
  .trailer {
    margin-top: 2rem;
    padding-top: 1rem;
    border-top: 1px solid #222;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 1rem;
  }
  .trailer-link {
    font-size: 0.75rem;
    color: #888;
    text-decoration: none;
    transition: color 0.15s ease;
  }
  .trailer-link:hover { color: #c7d2fe; }
  .trailer-link:focus-visible { outline: 2px solid #3b82f6; outline-offset: 2px; }
  .trailer .trailer-link:first-child { text-align: left; }
  .trailer .trailer-link:last-child { text-align: right; }
  .updated {
    font-size: 0.75rem;
    color: #666;
    justify-self: center;
  }
</style>
