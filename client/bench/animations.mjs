// Which running animation makes the table repaint: replays bench/views.json up to a step, lists the
// animations running there, then pauses each one in turn and counts the repaints (Paint events in
// 0.7 s). An animation the compositor runs (transform, opacity) costs no repaint; one that drops the
// count when paused repaints on every frame and is worth a look.
//
//   npx vite --port 5180 &                 # the dev server (it serves bench/views.json)
//   node bench/animations.mjs [--step 9] [--base http://localhost:5180] [--css "<extra CSS to try>"]
import { chromium } from 'playwright-core';

const arg = (k, d) => {
  const i = process.argv.indexOf(`--${k}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const BASE = arg('base', 'http://localhost:5180');
const STEP = Number(arg('step', 9));
const CSS = arg('css', null);

const b = await chromium.launch({ args: ['--disable-gpu'] });
const p = await b.newPage({ viewport: { width: 915, height: 412 } });
await p.goto(`${BASE}/?replay=/bench/views.json`);
await p.waitForSelector('.arena-hand', { state: 'attached', timeout: 30000 });
await p.waitForTimeout(1500);
for (let i = 0; i < STEP; i++) {
  await p.keyboard.press('ArrowRight');
  await p.waitForTimeout(120);
}
await p.waitForTimeout(2000);
if (CSS) {
  await p.addStyleTag({ content: CSS });
  await p.waitForTimeout(300);
}
const list = await p.evaluate(() =>
  document.getAnimations().map((a, i) => {
    const t = a.effect?.target;
    const cls = String(t?.className?.baseVal ?? t?.className ?? '').split(' ').filter(Boolean).slice(0, 2).join('.');
    return `${i} ${a.animationName ?? a.transitionProperty ?? a.constructor.name} on ${t?.tagName?.toLowerCase()}.${cls}${a.effect?.pseudoElement ?? ''}`;
  }),
);
async function paints() {
  await b.startTracing(p, { categories: ['devtools.timeline'] });
  await p.waitForTimeout(700);
  return JSON.parse((await b.stopTracing()).toString()).traceEvents.filter((e) => e.name === 'Paint').length;
}
console.log(`step ${STEP}: ${list.length} animations running, ${await paints()} repaints in 0.7 s`);
for (let i = 0; i < list.length; i++) {
  await p.evaluate((i) => document.getAnimations()[i]?.pause(), i);
  console.log(`  paused ${list[i]}: ${await paints()}`);
  await p.evaluate((i) => document.getAnimations()[i]?.play(), i);
}
await b.close();
