// Drives the real UI (DOM clicks) until the game ends. Logs each step in window.__auto.
window.__auto = { log: [], stop: false, errors: [] };
const A = window.__auto;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const tap = (el) => {
  const o = { bubbles: true, pointerId: 7, button: 0, pointerType: 'touch', isPrimary: true };
  el.dispatchEvent(new PointerEvent('pointerdown', o));
  el.dispatchEvent(new PointerEvent('pointerup', o));
  el.dispatchEvent(new MouseEvent('click', { bubbles: true }));
};
const btn = (root, re) => [...(root || document).querySelectorAll('button')].find((b) => re.test(b.textContent.trim()) && !b.disabled);
const rnd = (l) => l[Math.floor(Math.random() * l.length)];
(async () => {
  let idle = 0;
  while (!A.stop) {
    await sleep(350);
    if (document.querySelector('.end-screen')) { A.log.push('END ' + document.querySelector('.end-title').textContent); break; }
    const t = document.querySelector('.toast.error');
    if (t) { A.errors.push(t.textContent); t.click(); }
    const insp = document.querySelector('.inspector.insp-modal'); if (insp) { insp.click(); continue; }
    const bar = document.querySelector('.decision-bar');
    if (!bar || bar.classList.contains('waiting') || bar.classList.contains('sending')) { idle++; continue; }
    const kind = [...bar.classList].find((c) => c.startsWith('kind-'))?.slice(5);
    const prompt = bar.querySelector('.db-prompt')?.textContent;
    A.log.push(kind + ': ' + prompt);
    const modal = document.querySelector('.modal');
    if (kind === 'turn') {
      const sa = bar.querySelector('.sa-buttons');
      if (sa) { const b = [...sa.querySelectorAll('button.primary')]; if (b.length) { rnd(b).click(); continue; } btn(sa, /Cancel/).click(); }
      const playable = [...document.querySelectorAll('.board-card.is-playable')];
      if (playable.length && Math.random() < 0.8) { tap(rnd(playable)); continue; }
      const end = btn(bar, /End turn|Sure/); if (end) { end.click(); await sleep(100); btn(bar, /Sure/)?.click(); continue; }
      const pass = btn(bar, /Pass|Skip|No/); if (pass) { pass.click(); continue; }
    } else if (kind === 'select') {
      const scope = modal || document;
      const cards = [...scope.querySelectorAll(modal ? '.picker-item.selectable .picker-card' : '.board-card.is-selectable')];
      for (const c of cards.sort(() => Math.random() - 0.5)) {
        if (btn(modal || bar, /^Confirm$/)) break;
        modal ? c.click() : tap(c);
        await sleep(60);
      }
      const ok = btn(modal || bar, /^Confirm$/); if (ok) { ok.click(); continue; }
      const pass = btn(modal || bar, /Pass|Skip|No|Don/); if (pass) { pass.click(); continue; }
    } else if (kind === 'cleanup' || kind === 'exchange') {
      for (const c of [...document.querySelectorAll('.board-card.is-selectable')]) { if (btn(bar, /Discard|Exchange/)) break; tap(c); await sleep(60); }
      const ok = btn(bar, /Discard|Exchange/); if (ok) { ok.click(); continue; }
    } else if (kind === 'deck') {
      const items = [...document.querySelectorAll('.modal .deck-item')]; rnd(items).click(); await sleep(80); btn(document.querySelector('.modal'), /Play this deck/).click(); continue;
    } else if (kind === 'expedition') {
      const b = [...bar.querySelectorAll('.db-buttons button.primary')].filter((x) => !x.disabled && !/Confirm/.test(x.textContent));
      rnd(b).click(); await sleep(80); btn(bar, /^Confirm$/)?.click(); continue;
    } else {
      const b = [...bar.querySelectorAll('.db-buttons button')].filter((x) => !x.disabled && !/Undo/.test(x.textContent));
      if (b.length) { rnd(b).click(); continue; }
    }
    A.errors.push('no action for ' + kind + ' ' + prompt);
    await sleep(1000);
  }
})();
'started';
