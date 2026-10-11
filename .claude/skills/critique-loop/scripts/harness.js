// Playwright harness for the critique loop. Copy next to an `npm i playwright`.
// const { open } = require('./harness.js');
// const { page, errs, close } = await open('/overview', { w: 1440, h: 900, engine: 'webkit' });
// CCUI_URL = the app's entry URL (with its one-time token if any); it is visited first so the
// session cookie is set, then `path` is opened on the same origin.
const pw = require('playwright');
const BASE = process.env.CCUI_URL;
if (!BASE) throw new Error('Set CCUI_URL to the app URL (e.g. $(cat <scratch>/url.txt))');

async function open(path = '/', { w = 1440, h = 900, engine = 'webkit', theme, settle = 1200 } = {}) {
  const browser = await pw[engine].launch();
  const page = await browser.newPage({ viewport: { width: w, height: h }, deviceScaleFactor: 1 });
  const errs = [];
  page.on('console', m => m.type() === 'error' && errs.push(m.text()));
  page.on('pageerror', e => errs.push(String(e)));
  await page.goto(BASE, { waitUntil: 'networkidle' });
  const origin = new URL(BASE).origin;
  if (path !== '/') await page.goto(origin + path, { waitUntil: 'networkidle' });
  if (theme) await page.evaluate(t => (document.documentElement.dataset.theme = t), theme);
  await page.waitForTimeout(settle);
  return { browser, page, errs, origin, close: () => browser.close() };
}
module.exports = { open, BASE };

if (require.main === module) (async () => {
  // node harness.js <path> <out.png> [w] [h] [engine] [readySelector]
  const [path, out, w = '1440', h = '900', engine = 'webkit', ready] = process.argv.slice(2);
  const { page, errs, close } = await open(path, { w: +w, h: +h, engine });
  if (ready) await page.waitForSelector(ready, { timeout: 30000 });
  await page.screenshot({ path: out, fullPage: false });
  console.log(JSON.stringify({ url: page.url(), errs }));
  await close();
})();
