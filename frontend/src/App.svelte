<script>
  import MemoryStats from './components/MemoryStats.svelte';
  import ThermalStats from './components/ThermalStats.svelte';
</script>

<div class="container">
  <header>
    <h1>Spark Dashboard</h1>
    <p class="subtitle">NVIDIA DGX Spark Monitoring</p>
  </header>

  <main>
    <MemoryStats />
    <ThermalStats />
  </main>
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
</style>