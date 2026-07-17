import { chromium } from 'playwright';

const browser = await chromium.launch({
  headless: true,
  executablePath: 'C:\\Users\\SunJialei\\AppData\\Local\\ms-playwright\\chromium_headless_shell-1228\\chrome-headless-shell-win64\\chrome-headless-shell.exe',
});
const page = await browser.newPage({ viewport: { width: 480, height: 720 } });

const errors = [];
page.on('pageerror', e => errors.push(`PAGE ERROR: ${e.message}`));
page.on('console', msg => {
  if (msg.type() === 'error') errors.push(`CONSOLE ERROR: ${msg.text()}`);
});

await page.addInitScript(() => {
  const cbMap = new Map();
  let cbId = 0;
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd) => {
      const defaults = {
        load_brain_state: '{"ok":true}',
        start_cold_start: null,
        get_llm_config: '{"baseUrl":"","apiKey":"","model":""}',
        get_embedding_status: '{"loaded":false,"loading":false}',
      };
      if (defaults[cmd] !== undefined) return defaults[cmd];
      return null;
    },
    transformCallback: (fn) => {
      const id = ++cbId;
      cbMap.set(id, fn);
      return id;
    },
    convertFileSrc: (path) => `asset://localhost/${path}`,
    metadata: { currentWindow: { label: 'main' } },
  };
});

await page.goto('http://localhost:1420/', { waitUntil: 'networkidle', timeout: 15000 });
await page.waitForTimeout(3000);

await page.screenshot({ path: 'screenshot-final.png', fullPage: false });

const info = await page.evaluate(() => ({
  canvas: !!document.querySelector('canvas'),
  petContainer: !!document.querySelector('[class*="pet"]'),
  settingsBtn: !!document.querySelector('button'),
  bodyText: document.body.innerText.substring(0, 200),
}));

console.log('=== UI Check ===');
console.log(JSON.stringify(info, null, 2));
console.log('=== Errors ===');
console.log(errors.length === 0 ? 'NONE' : errors.join('\n'));

await browser.close();
