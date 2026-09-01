import { chromium } from 'playwright';

const browser = await chromium.launch();
const page = await browser.newPage();
const errors = [];
page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });

await page.goto('http://127.0.0.1:8090/', { waitUntil: 'networkidle' });
await page.waitForSelector('svg.radar', { timeout: 10000 });

const result = await page.evaluate(() => {
  const h = Array.from(document.querySelectorAll('h2')).find(t => t.textContent === 'Compute Load');
  const card = h.closest('.card');
  const blocks = Array.from(card.querySelectorAll('.radar-block')).map((b) => {
    const title = b.querySelector('.group-label')?.textContent.trim();
    const svg = b.querySelector('svg.radar');
    const rings = svg.querySelectorAll('path.radar-ring').length;
    const dots = svg.querySelectorAll('circle.radar-dot').length;
    const tips = Array.from(svg.querySelectorAll('text.radar-tip')).map(t => t.textContent);
    const hasLetterText = Array.from(svg.querySelectorAll('text')).some(t => /[a-zA-Z]/.test(t.textContent));
    return { title, rings, dots, tipCount: tips.length, firstTips: tips.slice(0,5), lastTips: tips.slice(-3), hasLetterText };
  });
  return {
    blocks,
    legendGone: !card.querySelector('.spider-legend, .radar-legend'),
    gpuTooltip: card.querySelector('.group-label[data-tooltip]')?.dataset.tooltip || null,
  };
});

console.log(JSON.stringify(result, null, 2));
console.log('errors:', errors.length ? errors : 'none');
await page.screenshot({ path: '/tmp/opencode/radars.png', fullPage: false });
await browser.close();
