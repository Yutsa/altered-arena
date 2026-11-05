// Smoothness bench of the game table on a low-end machine: counts the frames Chrome presents and
// the frames it drops while the player interacts (hand, drags, spells, card flights, the bot's
// turn). Complements run.mjs, which measures React's work per view.
//
// The main thread is slowed down (CDP CPU throttling, x6 by default: a low-end phone). The GPU side
// cannot be slowed down the same way, so its work is measured instead, as budgets that do not depend
// on the machine: how many times the page is repainted and the time spent rasterizing (raster and
// compositing are done in software, --disable-gpu, so they show in the trace), and the pixels the
// compositor blends on every frame at rest (layers x their area, in screens). Frames are read from
// the trace (PipelineReporter: presented, partially presented, dropped).
//
// Budgets: no dropped or partial frame; no repaint while nothing changes on screen.
//
//   npx vite build --mode bench && npx vite preview --mode bench &   # :4173, proxies /api and /assets to the game server (:8080)
//   DUMP=1 node bench/frames.mjs ... also prints where each thread spent its time
//   node bench/frames.mjs [--profile phone|desktop|both] [--cpu 4] [--runs 2] [--out bench/results/frames.json]
//
// Profiles: desktop = 1440x900, DPR 1, mouse; phone = 915x412 landscape, DPR 2, touch.
import { chromium } from 'playwright-core';
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';

const arg = (k, d) => {
  const i = process.argv.indexOf(`--${k}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const BASE = arg('base', 'http://localhost:4173');
const API = arg('api', 'http://localhost:8080');
const CPU = Number(arg('cpu', 6));
const RUNS = Number(arg('runs', 2));
const OUT = arg('out', null);
const PROFILES = { desktop: { w: 1440, h: 900, dpr: 1, touch: false }, phone: { w: 915, h: 412, dpr: 2, touch: true } };
const which = arg('profile', 'both');
const profiles = which === 'both' ? ['desktop', 'phone'] : [which];

const browser = await chromium.launch({ args: ['--disable-gpu', '--disable-gpu-compositing', '--num-raster-threads=1'] });

/** Where the renderer's main thread spent its time (ms), by kind of work */
const WORK = {
  script: /^(FunctionCall|EvaluateScript|TimerFire|FireAnimationFrame|EventDispatch|v8\.callFunction)$/,
  style: /^(UpdateLayoutTree|RecalculateStyles|ParseAuthorStyleSheet)$/,
  layout: /^(Layout|UpdateLayerTree|HitTest)$/,
  paint: /^(Paint|PaintImage|PrePaint|Layerize|Commit|RasterTask|Decode Image)$/,
};

/** Frames of a trace: presented, partial (the main thread missed it), dropped; and the main thread's work */
function frames(trace) {
  const c = { presented: 0, partial: 0, dropped: 0 };
  const events = JSON.parse(trace.toString()).traceEvents;
  const main = new Set(events.filter((e) => e.name === 'thread_name' && e.args?.name === 'CrRendererMain').map((e) => `${e.pid}:${e.tid}`));
  const work = { script: 0, style: 0, layout: 0, paint: 0, paints: 0, raster: 0 };
  for (const e of events) {
    if (e.ph !== 'X' || !e.dur) continue;
    if (e.name === 'RasterTask') work.raster += e.dur / 1000;
    if (!main.has(`${e.pid}:${e.tid}`)) continue;
    if (e.name === 'Paint') work.paints++;
    for (const [k, re] of Object.entries(WORK)) if (re.test(e.name)) work[k] += e.dur / 1000;
  }
  if (process.env.DUMP) {
    const tname = new Map(events.filter((e) => e.name === 'thread_name').map((e) => [`${e.pid}:${e.tid}`, e.args.name]));
    const agg = {};
    for (const e of events) if (e.ph === 'X' && e.dur) { const k = `${tname.get(`${e.pid}:${e.tid}`) ?? '?'} | ${e.name}`; agg[k] = (agg[k] ?? 0) + e.dur / 1000; }
    console.log(Object.entries(agg).sort((x, y) => y[1] - x[1]).slice(0, 18).map(([k, v]) => `  ${v.toFixed(0).padStart(6)} ms  ${k}`).join('\n'));
  }
  // A frame may be reported more than once (one report per pipeline that took part in it): its
  // best state counts
  const RANK = { STATE_PRESENTED_ALL: 3, STATE_PRESENTED_PARTIAL: 2, STATE_DROPPED: 1 };
  const best = new Map();
  for (const e of events) {
    if (e.name !== 'PipelineReporter' || e.ph !== 'b') continue;
    const r = e.args?.frame_reporter;
    const rank = RANK[r?.state];
    if (!rank) continue;
    const k = `${r.frame_source}:${r.frame_sequence}`;
    best.set(k, Math.max(best.get(k) ?? 0, rank));
  }
  for (const rank of best.values()) {
    if (rank === 3) c.presented++;
    else if (rank === 2) c.partial++;
    else c.dropped++;
  }
  const total = c.presented + c.partial + c.dropped;
  for (const k of Object.keys(work)) work[k] = Math.round(work[k]);
  return { ...c, total, work, droppedPct: total ? +((100 * c.dropped) / total).toFixed(1) : 0, partialPct: total ? +((100 * c.partial) / total).toFixed(1) : 0 };
}

async function newGame() {
  const r = await fetch(`${API}/api/v1/games`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ players: [{ name: 'Bench', deck: 'ax_1_sierra' }, { name: 'Bot', bot: true, ai: 'simple' }] }),
  });
  const g = await r.json();
  return { gameId: g.gameId, player: g.players[0] };
}

async function session(profileName) {
  const pr = PROFILES[profileName];
  const ctx = await browser.newContext({ viewport: { width: pr.w, height: pr.h }, deviceScaleFactor: pr.dpr, hasTouch: pr.touch, isMobile: pr.touch });
  const page = await ctx.newPage();
  const cdp = await ctx.newCDPSession(page);
  const sleep = (ms) => page.waitForTimeout(ms);
  // Pointer input: touch through CDP (real touch events), the mouse through Playwright
  const down = (x, y) => (pr.touch ? cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y, id: 1 }] }) : page.mouse.move(x, y).then(() => page.mouse.down()));
  const moveTo = (x, y) => (pr.touch ? cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x, y, id: 1 }] }) : page.mouse.move(x, y));
  const up = () => (pr.touch ? cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] }) : page.mouse.up());
  /** A move of `ms`, one input every 16 ms, as a hand moves */
  async function glide(x0, y0, x1, y1, ms) {
    const n = Math.max(1, Math.round(ms / 16));
    for (let i = 1; i <= n; i++) {
      await moveTo(x0 + ((x1 - x0) * i) / n, y0 + ((y1 - y0) * i) / n);
      await sleep(16);
    }
  }
  const tap = async (x, y) => {
    // The mouse leaves first: a card it hovers rises over its neighbours
    if (!pr.touch) await page.mouse.move(pr.w / 2, pr.h / 2);
    await down(x, y);
    await sleep(50);
    await up();
    await sleep(300);
  };
  /** Box of an element, null when it is gone (the card was played meanwhile) */
  const slotBox = (sel) => page.locator(sel).boundingBox({ timeout: 2000 }).catch(() => null);
  // The visible strip of a card of the hand (it hangs below the screen)
  const handPoint = (b) => ({ x: b.x + b.width / 2, y: Math.min(b.y + 16, pr.h - 10) });

  const { gameId, player } = await newGame();
  await page.goto(`${BASE}/?game=${gameId}&player=${player.id}#token=${player.token}`);
  await page.waitForSelector('.arena-hand .ah-slot', { state: 'attached' });
  await sleep(1500);
  // Setup: three cards to mana, then Confirm
  const slots = page.locator('.arena-hand .ah-slot');
  // Cards picked one by one, checked (a tap can land on a neighbour that a hover raised)
  for (let i = 0; i < 6 && (await page.locator('.arena-hand .is-picked').count()) < 3; i++) {
    const card = slots.nth(i).locator('[data-card-id]');
    if ((await card.getAttribute('class'))?.includes('is-picked')) continue;
    const b = await slots.nth(i).boundingBox();
    await tap(b.x + b.width * 0.3, Math.min(b.y + 16, pr.h - 10));
  }
  const cb = await page.locator('button:visible', { hasText: /confirm/i }).first().boundingBox();
  await tap(cb.x + cb.width / 2, cb.y + cb.height / 2);
  await page.waitForSelector('.arena-hand .is-playable', { state: 'attached', timeout: 30000 });
  await sleep(1200);

  // Pixels the compositor blends on every frame at rest: the layers that draw, by area
  let layers = [];
  cdp.on('LayerTree.layerTreeDidChange', (e) => e.layers && (layers = e.layers));
  await cdp.send('DOM.enable');
  await cdp.send('LayerTree.enable');
  // The layer tree is sent when it changes: one change
  await page.evaluate(() => document.body.style.setProperty('--bench', '1'));
  await page.mouse.move(2, 2);
  await sleep(600);
  const drawn = layers.filter((l) => l.drawsContent && !l.invisible);
  const composite = { layers: drawn.length, screens: +(drawn.reduce((a, l) => a + l.width * l.height, 0) / (pr.w * pr.h)).toFixed(1) };
  await cdp.send('LayerTree.disable');

  await cdp.send('Emulation.setCPUThrottlingRate', { rate: CPU });
  const results = {};
  async function measure(name, fn) {
    if (!(await page.locator('.arena-hand').count())) return;
    const cats = ['disabled-by-default-devtools.timeline.frame', 'devtools.timeline', 'benchmark', 'cc', 'viz'];
    if (process.env.PAINTS) cats.push('disabled-by-default-devtools.timeline.invalidationTracking');
    await browser.startTracing(page, { categories: cats });
    await fn();
    const trace = await browser.stopTracing();
    results[name] = frames(trace);
    // PAINTS=1: the elements repainted the most during the scenario
    if (process.env.PAINTS) {
      const count = new Map();
      for (const e of JSON.parse(trace.toString()).traceEvents) if (e.name === 'Paint' && e.args?.data?.nodeId) count.set(e.args.data.nodeId, (count.get(e.args.data.nodeId) ?? 0) + 1);
      const top = [...count].sort((a, b) => b[1] - a[1]).slice(0, 6);
      const named = [];
      for (const [id, n] of top) {
        const d = await cdp.send('DOM.describeNode', { backendNodeId: id }).catch(() => null);
        const cls = d?.node.attributes?.[d.node.attributes.indexOf('class') + 1] ?? '';
        named.push(`${n}x ${d?.node.localName ?? '?'}.${cls.split(' ').filter(Boolean).slice(0, 3).join('.')}`);
      }
      console.log(`  ${pr.touch ? 'phone' : 'desktop'} ${name}: ${named.join(' | ')}`);
      // What made them repaint: invalidations by element and reason
      const why = new Map();
      for (const e of JSON.parse(trace.toString()).traceEvents) {
        if (!/InvalidationTracking/.test(e.name)) continue;
        const d = e.args?.data ?? {};
        const k = `${e.name.replace('InvalidationTracking', '')} ${d.nodeName ?? ''} ${d.reason ?? ''}`.trim();
        why.set(k, (why.get(k) ?? 0) + 1);
      }
      const topWhy = [...why].sort((a, b) => b[1] - a[1]).slice(0, 5).map(([k, n]) => `${n}x ${k}`);
      if (topWhy.length) console.log(`    why: ${topWhy.join(' | ')}`);
    }
  }

  await measure('idle', () => sleep(2000));

  // The hand: hover along it (mouse), slide along it (finger)
  const n = await slots.count();
  const a = await slots.nth(0).boundingBox();
  const z = await slots.nth(n - 1).boundingBox();
  const hy = handPoint(a).y;
  await measure('hand', async () => {
    if (pr.touch) await down(a.x + a.width * 0.3, hy);
    else await page.mouse.move(a.x + a.width * 0.3, hy);
    await glide(a.x + a.width * 0.3, hy, z.x + z.width * 0.7, hy, 1000);
    await glide(z.x + z.width * 0.7, hy, a.x + a.width * 0.3, hy, 1000);
    if (pr.touch) await up();
    else await page.mouse.move(pr.w / 2, pr.h / 2);
    await sleep(300);
  });

  // Drags: each playable card is lifted; a spell is held over the board (and over Cast on a touch
  // screen), another card is carried around the board; then each is let go where it came from
  const ids = await page.$$eval('.arena-hand .is-playable', (els) => els.map((e) => e.dataset.cardId));
  let unit = null;
  let spell = null;
  for (const id of ids) {
    if (unit && spell) break;
    const box = await slotBox(`.arena-hand [data-card-id="${id}"]`);
    if (!box) continue;
    const p = handPoint(box);
    await down(p.x, p.y);
    await glide(p.x, p.y, p.x, p.y - 90, 150);
    const isSpell = await page.evaluate(() => !!document.querySelector('.spell-zones') || document.body.classList.contains('drag-spell'));
    await glide(p.x, p.y - 90, p.x, p.y, 100);
    await up();
    await sleep(500);
    if (isSpell && !spell) spell = id;
    if (!isSpell && !unit) unit = id;
  }
  for (const [name, id] of [['drag card', unit], ['drag spell', spell]]) {
    const box = id && (await slotBox(`.arena-hand [data-card-id="${id}"]`));
    if (!box) continue;
    const p = handPoint(box);
    await down(p.x, p.y);
    await glide(p.x, p.y, p.x, p.y - 100, 120);
    await measure(name, async () => {
      // Around the board: left, right (over Cast on a touch screen), the middle
      await glide(p.x, p.y - 100, pr.w * 0.25, pr.h * 0.35, 500);
      await glide(pr.w * 0.25, pr.h * 0.35, pr.w * 0.75, pr.h * 0.35, 700);
      await glide(pr.w * 0.75, pr.h * 0.35, pr.w * 0.75, pr.h * 0.55, 400);
      await glide(pr.w * 0.75, pr.h * 0.55, pr.w * 0.5, pr.h * 0.45, 400);
    });
    // Back to the hand, let go: no play
    await glide(pr.w * 0.5, pr.h * 0.45, p.x, pr.h - 4, 200);
    await up();
    await sleep(600);
  }

  // A card played into an expedition: the flight to the board, the new view
  const unitBox = unit && (await slotBox(`.arena-hand [data-card-id="${unit}"]`));
  if (unitBox) {
    const p = handPoint(unitBox);
    await down(p.x, p.y);
    await glide(p.x, p.y, p.x, p.y - 100, 120);
    // My left expedition: a valid drop zone
    const zone = await page.locator('[data-drop]').first().boundingBox({ timeout: 2000 }).catch(() => null);
    if (!zone) {
      // No zone for it (a Landmark with none free...): let go on the hand
      await up();
      await sleep(400);
    } else await glide(p.x, p.y - 100, zone.x + zone.width / 2, zone.y + zone.height / 2, 300);
  }
  if (unitBox && (await page.locator('.drag-ghost').count())) {
    await measure('play card', async () => {
      await up();
      await sleep(1500);
    });
  }

  // End of the turn: the bot plays (flights, reveals, the phase banner)
  const end = page.locator('button:visible', { hasText: /end turn|pass/i }).first();
  if (await end.count()) {
    const b = await end.boundingBox();
    await measure('bot turn', async () => {
      await tap(b.x + b.width / 2, b.y + b.height / 2);
      await sleep(3500);
    });
  }

  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 1 });
  await ctx.close();
  return { results, composite };
}

const all = {};
for (const pn of profiles) {
  const runs = [];
  let composite = null;
  for (let r = 0; r < RUNS; r++) {
    const s = await session(pn);
    runs.push(s.results);
    composite = s.composite;
  }
  // Worst run per scenario: a stutter is what the player sees
  const worst = {};
  for (const run of runs)
    for (const [k, v] of Object.entries(run)) if (!worst[k] || v.droppedPct + v.partialPct > worst[k].droppedPct + worst[k].partialPct) worst[k] = v;
  all[pn] = { runs, worst, composite };
  console.log(`\n${pn} (CPU x${CPU}, DPR ${PROFILES[pn].dpr}) — worst of ${RUNS} runs — at rest: ${composite.layers} layers, ${composite.screens} screens of pixels per frame`);
  console.table(Object.fromEntries(Object.entries(worst).map(([k, v]) => [k, { frames: v.total, 'dropped %': v.droppedPct, 'partial %': v.partialPct, script: v.work.script, style: v.work.style, layout: v.work.layout, 'paint ms': v.work.paint, repaints: v.work.paints, 'raster ms': v.work.raster }])));
}
await browser.close();
if (OUT) {
  mkdirSync(dirname(OUT), { recursive: true });
  writeFileSync(OUT, JSON.stringify({ date: new Date().toISOString(), cpu: CPU, profiles: PROFILES, results: all }, null, 2));
}
