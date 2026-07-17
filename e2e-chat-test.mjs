import { chromium } from 'playwright';
const browser = await chromium.launch({
  headless: true,
  executablePath: 'C:\\Users\\SunJialei\\AppData\\Local\\ms-playwright\\chromium_headless_shell-1228\\chrome-headless-shell-win64\\chrome-headless-shell.exe',
});
const page = await browser.newPage({ viewport: { width: 480, height: 720 } });
const errors = [];
page.on('pageerror', e => errors.push(`PAGE ERROR: ${e.message}`));
await page.addInitScript(() => {
  window.__TAURI_INTERNALS__ = {
    invoke: async (cmd, args) => {
      switch(cmd) {
        case 'send_message': { return `嗯嗯，${args?.text || ''}，我记住啦～`; }
        case 'get_emotion_state': return { mood: 0.6, mood_label: '开心', physical_energy: 0.7, social_battery: 0.8, stress: 0.2, loneliness: 0.0 };
        case 'check_cold_start': return null;
        case 'check_proactive': return null;
        case 'get_debug_snapshot': return { closeness: 5.0, mood: 0.6 };
        case 'get_llm_config': return { baseUrl: 'https://api.deepseek.com/v1', apiKey: 'test', model: 'deepseek-chat' };
        case 'get_embedding_status': return { loaded: false, loading: false };
        default: return null;
      }
    },
    transformCallback: () => Math.floor(Math.random() * 100000),
    convertFileSrc: (p) => `asset://localhost/${p}`,
    metadata: { currentWindow: { label: 'main' } },
  };
});
await page.goto('http://localhost:1420/', { waitUntil: 'networkidle', timeout: 15000 });
await page.waitForTimeout(2000);
const elements = await page.evaluate(() => ({
  canvas: !!document.querySelector('canvas'),
  buttons: document.querySelectorAll('button').length,
}));
console.log('Test 1 - UI Elements:', JSON.stringify(elements));
console.log('Test 2 - Page Errors:', errors.length === 0 ? 'NONE' : errors.join('; '));
await page.screenshot({ path: 'e2e-idle.png' });
await browser.close();
console.log('All E2E checks passed.');
