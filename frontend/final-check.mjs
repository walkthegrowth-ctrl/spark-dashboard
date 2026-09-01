import { chromium } from 'playwright';

const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
page.on('console', (msg) => { if (msg.type() === 'error') errors.push(`console: ${msg.text()}`); });

await page.goto('http://127.0.0.1:8090/', { waitUntil: 'networkidle' });
await page.waitForSelector('h1', { timeout: 10000 });

// Memory: "In Use" present
const memory = await page.evaluate(() => {
  const h = Array.from(document.querySelectorAll('h2')).find(t => t.textContent === 'Memory');
  const card = h.closest('.card');
  const labels = Array.from(card.querySelectorAll('.label')).map(l => l.textContent);
  const vals = Array.from(card.querySelectorAll('.stat')).map(s => ({
    label: s.querySelector('.label').textContent,
    value: s.querySelector('.value').textContent,
  }));
  return { labels, vals };
});

// Thermal: SoC group present and full; labels: no "(est.)", no "NVIDIA GB10"; WiFi; NVMe1/NVMe2
const thermal = await page.evaluate(() => {
  const h = Array.from(document.querySelectorAll('h2')).find(t => t.textContent === 'Thermal');
  const card = h.closest('.card');
  const all = Array.from(card.querySelectorAll('.stat .label')).map(l => l.textContent);
  const peripherals = Array.from(card.querySelectorAll('.group-label'))
    .slice(1)
    .map(g => Array.from(g.nextElementSibling.querySelectorAll('.stat .label')).map(l => l.textContent));
  return {
    allLabels: all,
    groups: Array.from(card.querySelectorAll('.group-label')).map(g => g.textContent),
    peripherals,
    noEst: !all.some(l => l.includes('(est.)')),
    noGpuTextInGpu: !all.some(l => l.includes('NVIDIA GB10')),
    hasWiFi: all.some(l => l === 'WiFi'),
    hasNVMe1: all.some(l => l === 'NVMe1'),
    hasNVMe2: all.some(l => l === 'NVMe2'),
    tooltipSamples: Array.from(card.querySelectorAll('.stat[data-tooltip]')).slice(0,6).map(e => e.dataset.tooltip),
  };
});

// Compute: spider svg exists, 20 spokes, center avg visible, GPU labeled
const compute = await page.evaluate(() => {
  const h = Array.from(document.querySelectorAll('h2')).find(t => t.textContent === 'Compute Load');
  const card = h.closest('.card');
  const svg = card.querySelector('svg.spider');
  const dots = card.querySelectorAll('circle.spider-dot').length;
  const rings = card.querySelectorAll('path.spider-ring').length;
  const ticks = Array.from(card.querySelectorAll('.spider-tick')).map(t => t.textContent);
  const gpuLabel = Array.from(card.querySelectorAll('.metric .label')).find(l => l.textContent === 'GPU')?.textContent;
  const legend = card.querySelector('.legend-note')?.textContent || null;
  return {
    hasSpiderSvg: !!svg,
    dotCount: dots,
    ringCount: rings,
    tickCount: ticks.length,
    ticks: ticks.slice(0,5),
    gpuLabel,
    legend,
    noGB10InGpu: !gpuLabel.includes('GB10'),
  };
});

console.log(JSON.stringify({ memory, thermal, compute, errors }, null, 2));
await page.screenshot({ path: '/tmp/opencode/final-dashboard.png', fullPage: true });
await browser.close();
