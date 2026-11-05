// Rendering bench of the game table. Replays a recorded game (bench/views.json, written by
// `node scripts/check-decisions.ts --games 1 --record bench/views.json`) in the replay page and
// measures, per step and per interaction: React commits, components rendered, React render time,
// time to the next painted frame, long tasks, DOM nodes inserted, card art not ready when painted.
//
//   npx vite build --mode bench          # profiling React, not minified, no service worker
//   npx vite preview --mode bench &      # :4173, proxies /api and /assets to the game server (:8080)
//   node bench/run.mjs [--runs 3] [--cpu 4] [--out bench/results/x.json]
import { chromium } from 'playwright-core';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';

const arg = (k, d) => {
  const i = process.argv.indexOf(`--${k}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const RUNS = Number(arg('runs', 3));
const CPU = Number(arg('cpu', 4));
const BASE = arg('base', 'http://localhost:4173');
const OUT = arg('out', null);
const VIEWS = readFileSync(new URL('./views.json', import.meta.url));
const STEP_WAIT = 700; // flights (320 ms + stagger) and reveals end before the next step

// Installed before React loads: a minimal DevTools hook that counts the components each commit rendered
const HOOK = () => {
  const stats = { commits: 0, renderMs: 0, rendered: 0, byName: {} };
  window.__bench = { stats, reset: () => Object.assign(stats, { commits: 0, renderMs: 0, rendered: 0, byName: {} }) };
  const nameOf = (f) => {
    const t = f.type;
    if (!t || typeof t === 'string') return null;
    if (typeof t === 'function') return t.displayName || t.name || 'Anonymous';
    if (t.render) return t.render.displayName || t.render.name || 'ForwardRef';
    if (t.type) return t.type.displayName || t.type.name || 'Memo';
    return null;
  };
  // function 0, class 1, forwardRef 11, memo 14, simple memo 15
  const COMPONENT = new Set([0, 1, 11, 14, 15]);
  const count = (f) => {
    if (!COMPONENT.has(f.tag)) return;
    const n = nameOf(f);
    if (!n) return;
    stats.rendered++;
    stats.byName[n] = (stats.byName[n] ?? 0) + 1;
  };
  const mount = (f) => {
    for (let c = f; c; c = c.sibling) {
      count(c);
      if (c.child) mount(c.child);
    }
  };
  // Same walk as React DevTools: an unchanged subtree keeps the same child fiber
  const update = (next, prev) => {
    if ((next.flags & 1) === 1) count(next); // PerformedWork
    if (next.child === prev.child) return;
    for (let c = next.child; c; c = c.sibling) {
      if (c.alternate) update(c, c.alternate);
      else {
        count(c);
        if (c.child) mount(c.child);
      }
    }
  };
  window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = {
    supportsFiber: true,
    isDisabled: false,
    renderers: new Map(),
    inject(r) {
      const id = this.renderers.size + 1;
      this.renderers.set(id, r);
      return id;
    },
    checkDCE() {},
    onScheduleFiberRoot() {},
    onCommitFiberUnmount() {},
    onPostCommitFiberRoot() {},
    onCommitFiberRoot(_id, root) {
      const cur = root.current;
      stats.commits++;
      stats.renderMs += cur.actualDuration ?? 0;
      if (cur.alternate) update(cur, cur.alternate);
      else if (cur.child) mount(cur.child);
    },
  };
  // DOM churn and long tasks
  window.__dom = { inserted: 0, imgs: 0 };
  new MutationObserver((ms) => {
    for (const m of ms)
      for (const n of m.addedNodes)
        if (n.nodeType === 1) {
          window.__dom.inserted += 1 + n.querySelectorAll('*').length;
          window.__dom.imgs += (n.tagName === 'IMG' ? 1 : 0) + n.querySelectorAll('img').length;
        }
  }).observe(document, { childList: true, subtree: true });
  window.__long = { n: 0, ms: 0 };
  new PerformanceObserver((l) => {
    for (const e of l.getEntries()) {
      window.__long.n++;
      window.__long.ms += e.duration;
    }
  }).observe({ type: 'longtask', buffered: false });
};

async function measure(page, action) {
  await page.evaluate(() => {
    window.__bench.reset();
    window.__dom.inserted = 0;
    window.__dom.imgs = 0;
    window.__long.n = 0;
    window.__long.ms = 0;
  });
  const t0 = await page.evaluate(() => performance.now());
  await action();
  // Time to the next painted frame after the input
  const paint = await page.evaluate(
    (t0) => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(() => r(performance.now() - t0)))),
    t0,
  );
  // Card art drawn on the table but not decoded yet: a blank card for a few frames
  const blank = await page.evaluate(
    () =>
      [...document.querySelectorAll('.arena img')].filter((i) => {
        const r = i.getBoundingClientRect();
        return r.width > 4 && r.height > 4 && getComputedStyle(i).visibility !== 'hidden' && (!i.complete || i.naturalWidth === 0);
      }).length,
  );
  await page.waitForTimeout(STEP_WAIT);
  const s = await page.evaluate(() => ({
    ...window.__bench.stats,
    byName: { ...window.__bench.stats.byName },
    inserted: window.__dom.inserted,
    imgs: window.__dom.imgs,
    longN: window.__long.n,
    longMs: window.__long.ms,
  }));
  return { ...s, paintMs: paint, blank };
}

async function run(browser, profile) {
  const ctx = await browser.newContext({ ...profile.ctx, serviceWorkers: 'block' });
  const page = await ctx.newPage();
  await page.addInitScript(HOOK);
  await page.route((u) => u.pathname === '/bench-views.json', (r) => r.fulfill({ body: VIEWS, contentType: 'application/json' }));
  const cdp = await ctx.newCDPSession(page);
  await page.goto(`${BASE}/?replay=/bench-views.json`);
  await page.waitForSelector('.arena [data-card-id]');
  await page.waitForTimeout(1500); // art of the first view
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: CPU });
  const total = await page.evaluate(() => Number(document.querySelector('.replay-controls small')?.textContent?.split('/')[1]) || 0);

  // 1. Every step of the game (a new GameView each time)
  const steps = [];
  for (let i = 0; i < total; i++) steps.push(await measure(page, () => page.keyboard.press('ArrowRight')));

  // 2. Interactions on a stable view: back to a turn decision with cards in hand
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: 1 });
  const views = JSON.parse(VIEWS);
  const turnAt = views.findIndex((v) => v.decision?.kind === 'turn' && v.cards.filter((c) => c.zone === 'hand' && c.owner === v.you).length >= 2);
  await page.evaluate((n) => {
    const r = document.querySelector('.replay-controls input[type=range]');
    const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
    set.call(r, String(n));
    r.dispatchEvent(new Event('input', { bubbles: true }));
  }, turnAt);
  await page.waitForTimeout(1500);
  await cdp.send('Emulation.setCPUThrottlingRate', { rate: CPU });
  const select = [];
  const handIds = await page.$$eval('.arena-hand [data-card-id]', (els) => els.map((e) => e.dataset.cardId));
  for (const id of handIds.slice(0, 3)) {
    // Select, then unselect, a card of the hand (UI state only, nothing sent)
    select.push(await measure(page, () => page.click(`.arena-hand [data-card-id="${id}"]`, { force: true })));
    select.push(await measure(page, () => page.click(`.arena-hand [data-card-id="${id}"]`, { force: true })));
  }
  const hover = [];
  if (!profile.ctx.hasTouch) {
    const boardIds = await page.$$eval('.arena-plane [data-card-id]', (els) => els.map((e) => e.dataset.cardId));
    for (const id of boardIds.slice(0, 6)) {
      // Hover preview (shown after 280 ms), then leave
      hover.push(
        await measure(page, async () => {
          await page.hover(`.arena-plane [data-card-id="${id}"]`, { force: true });
          await page.waitForTimeout(350);
        }),
      );
      hover.push(await measure(page, () => page.mouse.move(2, 2)));
    }
  }
  await ctx.close();
  return { steps, select, hover };
}

const PROFILES = {
  phone: { ctx: { viewport: { width: 844, height: 390 }, deviceScaleFactor: 3, isMobile: true, hasTouch: true } },
  desktop: { ctx: { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 2 } },
};

const pct = (xs, p) => {
  if (!xs.length) return 0;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.floor((p / 100) * s.length))];
};
const sum = (xs) => xs.reduce((a, b) => a + b, 0);
const median = (xs) => pct(xs, 50);

function summarize(samples) {
  // samples: one array of measures per run
  const per = samples.map((ms) => ({
    commits: sum(ms.map((m) => m.commits)) / ms.length,
    rendered: sum(ms.map((m) => m.rendered)) / ms.length,
    matCards: sum(ms.map((m) => m.byName.MatCard ?? 0)) / ms.length,
    renderMs: sum(ms.map((m) => m.renderMs)) / ms.length,
    paintP50: pct(ms.map((m) => m.paintMs), 50),
    paintP95: pct(ms.map((m) => m.paintMs), 95),
    longMs: sum(ms.map((m) => m.longMs)),
    inserted: sum(ms.map((m) => m.inserted)) / ms.length,
    imgs: sum(ms.map((m) => m.imgs)),
    blank: sum(ms.map((m) => m.blank)),
  }));
  const out = {};
  for (const k of Object.keys(per[0] ?? {})) out[k] = +median(per.map((p) => p[k])).toFixed(2);
  return out;
}

const browser = await chromium.launch();
const results = {};
for (const [name, profile] of Object.entries(PROFILES)) {
  const runs = [];
  for (let r = 0; r < RUNS; r++) runs.push(await run(browser, profile));
  results[name] = {
    steps: summarize(runs.map((x) => x.steps)),
    select: summarize(runs.map((x) => x.select)),
    ...(runs[0].hover.length ? { hover: summarize(runs.map((x) => x.hover)) } : {}),
    topRendered: Object.entries(
      runs[0].steps.reduce((acc, m) => {
        for (const [k, v] of Object.entries(m.byName)) acc[k] = (acc[k] ?? 0) + v;
        return acc;
      }, {}),
    )
      .sort((a, b) => b[1] - a[1])
      .slice(0, 12),
  };
}
await browser.close();
console.log(JSON.stringify(results, null, 1));
if (OUT) {
  mkdirSync(dirname(OUT), { recursive: true });
  writeFileSync(OUT, JSON.stringify({ cpu: CPU, runs: RUNS, results }, null, 1));
}
