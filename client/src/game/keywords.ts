/** Effect-text keywords (`<KEYWORD>` / `$<KEYWORD>` markup of the catalog), with their reminder text. */
export interface Keyword {
  text: string;
  reminder?: string;
  icon?: string;
}

const RESUPPLY = 'Put the top card of your deck in Reserve.';
const EXHAUSTED_RESUPPLY = 'Put the top card of your deck in Reserve, then exhaust it {T}.';
const SABOTAGE = 'Discard up to one target card from a Reserve.';
const SEASONED = 'I keep my boosts when I go to Reserve.';
const DEFENDER = "My Expedition can't advance during Dusk.";
const ETERNAL = "During Rest, I don't go to Reserve.";
const ANCHORED = "During Rest, I don't go to Reserve and I lose Anchored.";
const ASLEEP = "During Dusk, ignore my statistics. During Rest, I don't go to Reserve and I lose Asleep.";
const ASCEND = "Until Rest, it can move forward even if matched in its region's terrains by the opponent's Expedition.";
const tough = (n: string) => `Your opponent's Spells and abilities that target me cost {${n}} more.`;
const scout = (n: number) => `You may play me from hand for ${n} with "{h} Send me to Reserve".`;

export const KEYWORDS: Record<string, Keyword> = {
  AFTER_YOU: { text: 'After You', reminder: 'End your turn as if you had played a card. You may still play cards later this Day.' },
  ANCHORED: { text: 'Anchored', reminder: ANCHORED, icon: 'anchored' },
  ANCHORED_FS: { text: 'Anchored', icon: 'anchored' },
  ANCHORED_CHA_P: { text: 'Anchored', reminder: ANCHORED, icon: 'anchored' },
  ASLEEP: { text: 'Asleep', reminder: ASLEEP, icon: 'sleep' },
  ASLEEP_CHA_P: { text: 'Asleep', reminder: ASLEEP, icon: 'sleep' },
  BB: { text: '', reminder: 'A boost is a +1/+1/+1 counter. Remove it when it leaves the Expedition zone.' },
  BOOST: { text: 'boost', icon: 'boost' },
  BOODA: { text: 'Booda 2/2/2' },
  BOOSTED: { text: 'Boosted' },
  BOOSTED_CHA_P: { text: 'Boosted' },
  BOOSTED_TKN_P: { text: 'Boosted' },
  BRASSBUG: { text: 'Brassbug 2/2/2' },
  DEFENDER: { text: 'Defender', reminder: DEFENDER },
  DEFENDER_FS: { text: 'Defender', reminder: DEFENDER },
  DEFENDER_CHA_P: { text: 'Defender', reminder: DEFENDER },
  ETERNAL: { text: 'Eternal', reminder: ETERNAL },
  ETERNAL_FS: { text: 'Eternal', reminder: ETERNAL },
  FLEETING: { text: 'Fleeting', reminder: 'Send me to Discard instead of Reserve after my effect resolves.', icon: 'fleeting' },
  FLEETING_CHAR: { text: 'Fleeting', reminder: 'If I would be sent to Reserve, discard me instead.', icon: 'fleeting' },
  GIGANTIC: { text: 'Gigantic', reminder: 'I am considered present in each of your Expeditions.' },
  MAW: { text: 'Maw 0/0/0' },
  ORDIS_RECRUIT: { text: 'Ordis Recruit 1/1/1' },
  RESUPPLY: { text: 'Resupply', reminder: RESUPPLY },
  RESUPPLY_INF: { text: 'Resupply', reminder: RESUPPLY },
  RESUPPLY_LOW: { text: 'Resupply', reminder: RESUPPLY },
  RESUPPLY_T: { text: 'Resupply', reminder: RESUPPLY },
  RESUPPLY_THEY_S: { text: 'Resupply', reminder: RESUPPLY },
  RESUPPLIES: { text: 'Resupplies', reminder: RESUPPLY },
  EXHAUSTED_RESUPPLY: { text: 'Exhausted Resupply', reminder: EXHAUSTED_RESUPPLY },
  EXHAUSTED_RESUPPLIES: { text: 'Exhausted Resupplies', reminder: EXHAUSTED_RESUPPLY },
  EXHAUSTED_RESUPPLY_LOW: { text: 'Exhausted Resupply', reminder: EXHAUSTED_RESUPPLY },
  EXHAUSTED_RESUPPLY_INF: { text: 'Exhausted Resupply', reminder: EXHAUSTED_RESUPPLY },
  SABOTAGE: { text: 'Sabotage', reminder: SABOTAGE },
  SABOTAGE_LOW: { text: 'Sabotage', reminder: SABOTAGE },
  SABOTAGE_INF: { text: 'Sabotage', reminder: SABOTAGE },
  SEASONED: { text: 'Seasoned', reminder: SEASONED },
  SEASONED_CHA_P: { text: 'Seasoned', reminder: SEASONED },
  SEASONED_ME_FS: { text: 'Seasoned', reminder: SEASONED },
  TOUGH_1: { text: 'Tough 1', reminder: tough('1') },
  TOUGH_2: { text: 'Tough 2', reminder: tough('2') },
  TOUGH_X: { text: 'Tough X', reminder: tough('X') },
  TOUGH_FS_X: { text: 'Tough X', reminder: tough('X') },
  TOUGH_CHA_P_1: { text: 'Tough 1', reminder: tough('1') },
  TOUGH_CHA_P_5: { text: 'Tough 5', reminder: tough('5') },
  COOLDOWN: {
    text: 'Cooldown',
    reminder: "If I go to Reserve after my effect resolves, exhaust me {T}. Exhausted cards can't be played and have no Support abilities.",
  },
  MANA_MOTH: { text: 'Mana Moth 2/2/2' },
  DRAGON_SHADE: { text: 'Dragon Shade 5/5/5' },
  AUGMENT: { text: 'Augment', reminder: "If it has counters, it gains 1 more. This can't target Hero cards." },
  AUGMENT_IMP: { text: 'Augment', reminder: "If it has counters, it gains 1 more. This can't target Hero cards." },
  SCOUT_1: { text: 'Scout', reminder: scout(1) },
  SCOUT_2: { text: 'Scout', reminder: scout(2) },
  SCOUT_3: { text: 'Scout', reminder: scout(3) },
  SCOUT_4: { text: 'Scout', reminder: scout(4) },
  AEROLITH: { text: 'Aerolith' },
  HALUA: { text: 'Halua' },
  RUSH: { text: 'Rush' },
  WOOLLYBACK: { text: 'Woollyback 1/1/1' },
  ASCENDS: { text: 'Ascends', reminder: ASCEND },
  ASCEND: { text: 'Ascend', reminder: ASCEND },
  ASCEND_INF: { text: 'Ascend', reminder: ASCEND },
  ASCENDED_S: { text: 'Ascended', reminder: ASCEND },
  ASCENDED_P: { text: 'Ascended', reminder: ASCEND },
  DUE_TO_ASCENSION: { text: 'Due to Ascension', reminder: 'if it moved forward due to at least one matched stat.' },
  MANASEED: { text: 'Manaseed' },
  IN_CONTACT: { text: 'In contact', reminder: "I'm In Contact if another player's Expedition is in my region." },
  GIFT: { text: 'Gift' },
  COMPLETED_LOW: { text: 'Completed' },
};

/** Reminder of the statuses shown on cards (CardView.statuses). */
export const STATUS_INFO: Record<string, { label: string; icon?: string; reminder?: string }> = {
  fleeting: { label: 'Fleeting', icon: 'fleeting', reminder: KEYWORDS.FLEETING_CHAR.reminder },
  anchored: { label: 'Anchored', icon: 'anchored', reminder: ANCHORED },
  asleep: { label: 'Asleep', icon: 'sleep', reminder: ASLEEP },
  tough: { label: 'Tough', icon: 'tough', reminder: tough('X') },
  defender: { label: 'Defender', reminder: DEFENDER },
  gigantic: { label: 'Gigantic', reminder: KEYWORDS.GIGANTIC.reminder },
  eternal: { label: 'Eternal', reminder: ETERNAL },
  seasoned: { label: 'Seasoned', reminder: SEASONED },
  scout: { label: 'Scout' },
  cooldown: { label: 'Cooldown', reminder: KEYWORDS.COOLDOWN.reminder },
};

/** "SOME_KEYWORD" -> "Some Keyword", for markup the client does not know yet. */
export function humanizeKeyword(k: string): string {
  return k
    .toLowerCase()
    .split('_')
    .filter(Boolean)
    .map((w) => w[0].toUpperCase() + w.slice(1))
    .join(' ');
}
