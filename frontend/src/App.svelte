<script>
  import MemoryStats from './components/MemoryStats.svelte';
  import ThermalStats from './components/ThermalStats.svelte';
  import ComputeStats from './components/ComputeStats.svelte';
  import { lastUpdated } from './lib/lastUpdated.js';

  function formatTime(ms) {
    if (!ms) return 'waiting for data…';
    return new Date(ms).toLocaleTimeString();
  }
</script>

<div class="container">
  <header>
    <h1>Spark Dashboard</h1>
    <p class="subtitle">NVIDIA DGX Spark Monitoring</p>
  </header>

  <main>
    <MemoryStats />
    <ThermalStats />
    <ComputeStats />
  </main>

  <footer class="trailer">
    <span class="updated" data-lastupdated={$lastUpdated}>Updated {formatTime($lastUpdated)}</span>
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
  }
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
    text-align: center;
  }
  .updated {
    font-size: 0.75rem;
    color: #666;
  }
</style>
