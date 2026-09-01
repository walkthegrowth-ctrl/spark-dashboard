const { chromium } = require('../frontend/node_modules/playwright');

const PORT = process.env.PORT || 8090;

async function main() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const consoleMessages = [];
  const pageErrors = [];
  const networkErrors = [];
  page.on('console', (m) => consoleMessages.push(`[${m.type()}] ${m.text()}`));
  page.on('pageerror', (e) => pageErrors.push(`[pageerror] ${e.message}`));
  page.on('requestfailed', (r) => networkErrors.push(`[network] ${r.url()} - ${r.failure()?.errorText}`));

  try {
    await page.goto(`http://localhost:${PORT}`, { waitUntil: 'networkidle', timeout: 20000 });
    // Wait for the three panels to have rendered live values.
    await page.waitForTimeout(4000);

    // Grab visible text of each stat panel to prove data rendered.
    const bodyText = await page.evaluate(() => document.body.innerText);
    const hasMemory = /Memory|GiB|MiB|%/i.test(bodyText);
    const hasThermal = /°C|thermal|zone|SoC/i.test(bodyText);
    const hasCompute = /CPU|GPU|load|core/i.test(bodyText);

    await page.screenshot({ path: 'tools/screenshot.png', fullPage: true });

    console.log('=== Rendered data ===');
    console.log('memory panel present:', hasMemory);
    console.log('thermal panel present:', hasThermal);
    console.log('compute panel present:', hasCompute);
    console.log('\n=== Console ===');
    consoleMessages.forEach((m) => console.log(m));
    console.log('\n=== Page errors ===');
    pageErrors.forEach((e) => console.log(e));
    console.log('\n=== Network failures ===');
    networkErrors.forEach((e) => console.log(e));

    const totalErrors =
      consoleMessages.filter((m) => m.startsWith('[error]')).length +
      pageErrors.length +
      networkErrors.length;
    console.log(`\n=== Status === ${totalErrors === 0 ? 'PASS: no errors' : `FAIL: ${totalErrors} error(s)`}`);
  } catch (err) {
    console.error('ERROR:', err.message);
    process.exitCode = 1;
  } finally {
    await browser.close();
  }
}

main().catch((e) => { console.error(e); process.exit(1); });
