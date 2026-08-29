const { chromium } = require('../frontend/node_modules/playwright');
const { spawn } = require('child_process');
const { execSync } = require('child_process');
const path = require('path');

// Change to project root
process.chdir(path.join(__dirname, '..'));

const PORT = process.argv.includes('--port')
  ? parseInt(process.argv[process.argv.indexOf('--port') + 1])
  : 8090;

const TIMEOUT = 10000;
const WAIT_AFTER_LOAD = 3000;

async function main() {
  console.log(`Starting debug session on port ${PORT}...\n`);

  // Start the server
  const server = spawn('bash', ['run.sh', '--port', String(PORT)], {
    detached: true,
    stdio: 'pipe',
  });

  server.stdout.on('data', (d) => {});
  server.stderr.on('data', (d) => {});

  // Wait for server to be ready
  await new Promise((r) => setTimeout(r, 8000));

  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });

  const consoleMessages = [];
  page.on('console', (msg) => {
    consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
  });

  const pageErrors = [];
  page.on('pageerror', (err) => {
    pageErrors.push(`[pageerror] ${err.message}`);
  });

  const networkErrors = [];
  page.on('requestfailed', (req) => {
    const err = req.failure()?.errorText || 'failed';
    networkErrors.push(`[network] ${err} - ${req.url()}`);
  });

  try {
    console.log(`Navigating to http://localhost:${PORT}...`);
    await page.goto(`http://localhost:${PORT}`, {
      waitUntil: 'networkidle',
      timeout: TIMEOUT,
    });

    // Wait for content to render
    await new Promise((r) => setTimeout(r, WAIT_AFTER_LOAD));

    // Take screenshot
    const screenshotPath = 'tools/screenshot.png';
    await page.screenshot({ path: screenshotPath, fullPage: true });

    // Print results
    console.log('=== Console Messages ===');
    if (consoleMessages.length === 0) {
      console.log('(none)');
    } else {
      consoleMessages.forEach((m) => console.log(m));
    }

    console.log('\n=== Page Errors ===');
    if (pageErrors.length === 0) {
      console.log('(none)');
    } else {
      pageErrors.forEach((e) => console.log(e));
    }

    console.log('\n=== Network Errors ===');
    if (networkErrors.length === 0) {
      console.log('(none)');
    } else {
      networkErrors.forEach((e) => console.log(e));
    }

    console.log(`\n=== Screenshot ===`);
    console.log(screenshotPath);

    const totalErrors = consoleMessages.filter((m) => m.startsWith('[error]')).length + pageErrors.length + networkErrors.length;
    console.log(`\n=== Status ===`);
    if (totalErrors === 0) {
      console.log('PASS: No errors found');
    } else {
      console.log(`FAIL: ${totalErrors} error(s) found`);
    }

    // Print page content for inspection
    console.log('\n=== Page Content (first 2000 chars) ===');
    const content = await page.content();
    console.log(content.substring(0, 2000));
  } catch (err) {
    console.error(`Error during debug: ${err.message}`);
  } finally {
    await browser.close();
    // Kill the server
    server.kill('SIGTERM');
    setTimeout(() => {
      try {
        server.kill('SIGKILL');
      } catch {}
    }, 2000);
  }
}

main().catch(console.error);
