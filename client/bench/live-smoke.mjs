// Live smoke test: creates a game against the bot on the game server (:8080), opens it in the client
// (phone viewport) and plays random legal moves through the UI until the game ends.
//   node bench/live-smoke.mjs [client url, default http://localhost:5199] [timeout ms]
// (the setPointerCapture page error comes from the synthetic pointer events, not from the app)
import { chromium } from 'playwright-core';
import { readFileSync } from 'node:fs';
const base = process.argv[2] ?? 'http://localhost:5199';
const decks = await (await fetch('http://localhost:8080/api/v1/decks')).json();
const g = await (await fetch('http://localhost:8080/api/v1/games', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ players: [{ name: 'Smoke', deck: decks[0].id }, { name: 'Bot', bot: true }] }) })).json();
const me = g.players.find((p) => !p.bot);
const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 844, height: 390 }, hasTouch: true, isMobile: true });
const errs = [];
p.on('console', (m) => { if (m.type() === 'error') errs.push(m.text().slice(0, 300)); });
p.on('pageerror', (e) => errs.push('pageerror ' + e.message));
await p.goto(`${base}/?game=${g.gameId}&player=${me.id}#token=${me.token}`);
await p.waitForSelector('.arena', { timeout: 30000 });
await p.evaluate(() => {
  // Random legal play through the real UI: picker, selectable or playable cards, drop zones, main button
  window.__auto = { log: [], errors: [] };
  const A = window.__auto;
  const rnd = (l) => l[Math.floor(Math.random() * l.length)];
  const tap = (el) => {
    const o = { bubbles: true, pointerId: 7, button: 0, pointerType: 'touch', isPrimary: true };
    el.dispatchEvent(new PointerEvent('pointerdown', o));
    el.dispatchEvent(new PointerEvent('pointerup', o));
    el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  };
  setInterval(() => {
    if (document.querySelector('.end-screen')) return void (A.log.at(-1) !== 'END' && A.log.push('END'));
    const insp = document.querySelector('.inspector'); if (insp) return void insp.click();
    const modal = document.querySelector('.modal');
    if (modal) {
      const items = [...modal.querySelectorAll('.picker-item.selectable:not(.picked)')];
      const ok = [...modal.querySelectorAll('.modal-foot button:not([disabled])')];
      if (items.length && Math.random() < 0.6) return void items[0].querySelector('*')?.click?.() ?? items[0].click();
      if (ok.length) return void (A.log.push('modal'), rnd(ok).click());
      return;
    }
    const main = document.querySelector('.end-dock .track-btn:not([disabled])');
    const drops = [...document.querySelectorAll('.drop-valid')];
    if (drops.length) return void (A.log.push('drop'), drops[0].click());
    const cards = [...document.querySelectorAll('.is-selectable:not(.is-picked), .is-playable, .pick-overlay:not(.picked), button.marker:not([disabled]), .player-chip.selectable')];
    const btns = [...document.querySelectorAll('.prompt button:not([disabled]), .effect-queue button:not(.eq-min):not([disabled])')];
    if (cards.length && Math.random() < 0.5) return void (A.log.push('card'), tap(rnd(cards)));
    if (btns.length && Math.random() < 0.5) return void (A.log.push('btn'), rnd(btns).click());
    if (main) return void (A.log.push('main'), main.click());
  }, 400);
});
const t0 = Date.now();
await p.waitForTimeout(6000); await p.screenshot({ path: '/private/tmp/claude-501/live.png' }); console.log(await p.evaluate(() => document.querySelector('.center-dock')?.outerHTML.slice(0, 800)));
while (Date.now() - t0 < Number(process.argv[3] ?? 240000)) {
  await p.waitForTimeout(5000);
  if (await p.evaluate(() => window.__auto.log.some((l) => l.startsWith('END')))) break;
}
await p.screenshot({ path: '/private/tmp/claude-501/live-end.png' }); console.log(await p.evaluate(() => document.querySelector('.end-dock')?.textContent + ' | ' + document.querySelectorAll('.is-selectable,.is-playable,.drop-valid').length + ' | ' + document.querySelector('.prompt')?.textContent));
const a = await p.evaluate(() => ({ n: window.__auto.log.length, last: window.__auto.log.slice(-3), errors: window.__auto.errors }));
console.log(JSON.stringify(a), '\nconsole errors:', [...new Set(errs)].slice(0, 10));
await b.close();
