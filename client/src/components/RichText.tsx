import { Fragment, type ReactNode } from 'react';
import { Glyph, ManaIcon } from './Glyph';
import { KEYWORDS, humanizeKeyword } from '../game/keywords';
import { CardName } from './CardName';

const MARKERS: Record<string, string> = {
  J: 'played',
  j: 'played',
  H: 'hand',
  h: 'hand',
  R: 'reserve',
  r: 'reserve',
  D: 'discard',
  T: 'tap',
  V: 'forest',
  M: 'mountain',
  O: 'ocean',
  E: 'ocean',
  I: 'infinity',
  COUNTER: 'charge',
};

const MARKER_TITLES: Record<string, string> = {
  played: 'When played',
  hand: 'From hand',
  reserve: 'From reserve',
  discard: 'Discard me',
  tap: 'Exhaust',
  forest: 'Forest',
  mountain: 'Mountain',
  ocean: 'Ocean',
  infinity: 'Infinity',
  charge: 'Counter',
};

const ICON_KEYWORDS = new Set(['BOOST', 'ANCHORED', 'FLEETING', 'ASLEEP']);

function KeywordNode({ name, arg, withReminder }: { name: string; arg?: string; withReminder: boolean }) {
  const kw = KEYWORDS[name];
  const text = kw ? kw.text : humanizeKeyword(name);
  const icon = kw?.icon && (ICON_KEYWORDS.has(name) || arg !== undefined);
  return (
    <>
      {text || icon ? (
        <span className={`rt-keyword ${icon ? 'rt-keyword-icon' : ''}`} title={kw?.reminder?.replace(/\{([^}]+)\}/g, '$1')}>
          {icon && <img className="rt-icon" src={`/img/icons/${kw!.icon}.png`} alt="" />}
          {arg !== undefined && <span>{arg}</span>}
          {text}
        </span>
      ) : null}
      {withReminder && kw?.reminder && <span className="rt-paren"> ({parseRich(kw.reminder, 'r')})</span>}
    </>
  );
}

function findClosingParen(s: string, from: number): number {
  let depth = 0;
  for (let i = from; i < s.length; i++) {
    if (s[i] === '(') depth++;
    else if (s[i] === ')') {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

/**
 * Parses the effect markup of the catalog and of server prompts/logs:
 * {J} {H} {R} {D} {T} {V} {M} {O} {I} icons, {1} mana, <KEYWORD>, $<KEYWORD> (with reminder),
 * <BOOST:2>, <BR>, [[card name]], [reference], #emphasis#, (parenthesis) as reminders.
 */
export function parseRich(s: string, keyPrefix = ''): ReactNode[] {
  const out: ReactNode[] = [];
  let buf = '';
  let k = 0;
  const key = () => `${keyPrefix}${k++}`;
  const flush = () => {
    if (buf) out.push(buf);
    buf = '';
  };
  let i = 0;
  while (i < s.length) {
    const ch = s[i];
    const rest = s.slice(i);
    if (ch === '\\') {
      i++;
      continue;
    }
    if (ch === '#') {
      const j = s.indexOf('#', i + 1);
      if (j > i) {
        flush();
        out.push(
          <span key={key()} className="rt-rare">
            {parseRich(s.slice(i + 1, j), key())}
          </span>,
        );
        i = j + 1;
        continue;
      }
    }
    if (ch === '(') {
      const j = findClosingParen(s, i);
      if (j > i) {
        flush();
        out.push(
          <span key={key()} className="rt-paren">
            ({parseRich(s.slice(i + 1, j), key())})
          </span>,
        );
        i = j + 1;
        continue;
      }
    }
    if (ch === '{') {
      const m = /^\{([A-Za-z0-9$]+)\}/.exec(rest);
      if (m) {
        flush();
        const t = m[1];
        if (MARKERS[t]) out.push(<Glyph key={key()} name={MARKERS[t]} title={MARKER_TITLES[MARKERS[t]]} />);
        else if (/^[0-9X]+$/.test(t)) out.push(<ManaIcon key={key()} value={t} />);
        else out.push(`{${t}}`);
        i += m[0].length;
        continue;
      }
    }
    if (ch === '$') {
      const m = /^\$(?:<([A-Z0-9_]+)>|\[([A-Z0-9_]+)\])/.exec(rest);
      if (m) {
        flush();
        out.push(<KeywordNode key={key()} name={m[1] ?? m[2]} withReminder />);
        i += m[0].length;
        continue;
      }
    }
    if (ch === '<') {
      const m = /^<([A-Z0-9_]+)(?::([^>]+))?>/.exec(rest);
      if (m) {
        flush();
        if (m[1] === 'BR') out.push(<br key={key()} />);
        else out.push(<KeywordNode key={key()} name={m[1]} arg={m[2]} withReminder={false} />);
        i += m[0].length;
        continue;
      }
    }
    if (ch === '[') {
      const m2 = /^\[\[([^\]]+)\]\]/.exec(rest);
      if (m2) {
        flush();
        out.push(<CardName key={key()} name={m2[1]} />);
        i += m2[0].length;
        continue;
      }
      const m1 = /^\[([A-Z0-9_]+)\]/.exec(rest);
      if (m1) {
        flush();
        out.push(<KeywordNode key={key()} name={m1[1]} withReminder={false} />);
        i += m1[0].length;
        continue;
      }
      const m = /^\[([^\]]+)\]/.exec(rest);
      if (m) {
        flush();
        out.push(
          <span key={key()} className="rt-ref">
            {m[1]}
          </span>,
        );
        i += m[0].length;
        continue;
      }
    }
    buf += ch;
    i++;
  }
  flush();
  return out;
}

export function RichText({ text, className }: { text: string | undefined | null; className?: string }) {
  if (!text) return null;
  // Paragraphs: the catalog separates abilities with two spaces
  const parts = text.split(/ {2,}/);
  return (
    <span className={`rich-text ${className ?? ''}`}>
      {parts.map((p, i) => (
        <Fragment key={i}>
          {i > 0 && <br />}
          {parseRich(p, `p${i}-`)}
        </Fragment>
      ))}
    </span>
  );
}

/** Strips the markup, for plain text contexts (aria labels, titles). */
export function plainText(s: string): string {
  return s
    .replace(/\\/g, '')
    .replace(/\[\[([^\]]+)\]\]/g, '$1')
    .replace(/\$?<([A-Z0-9_]+)(?::([^>]+))?>/g, (_m, kw: string, arg?: string) => `${arg ?? ''}${KEYWORDS[kw]?.text ?? humanizeKeyword(kw)}`)
    .replace(/#/g, '')
    .replace(/\{([^}]+)\}/g, (_m, t: string) => MARKER_TITLES[MARKERS[t]] ?? t);
}
