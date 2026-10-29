#!/usr/bin/env python3
"""Builds the client card catalog and starter deck list from the engine export.

Input:  rules/data/cards.min.json, rules/data/starters.min.json (tools: server/tools/export_cards.php)
Output: protocol/data/cards.json (CardCatalog), protocol/data/decks.json (DeckSummary[])
"""
import json, os, sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
src = os.path.join(ROOT, 'engine', 'data')
cards = json.load(open(os.path.join(src, 'cards.min.json')))
starters = json.load(open(os.path.join(src, 'starters.min.json')))

RARITY = {0: 'common', 1: 'rare', 2: 'unique', 3: 'exalted'}
KEYWORDS = ['fleeting', 'anchored', 'asleep', 'gigantic', 'tough', 'defender', 'eternal', 'seasoned', 'scout', 'cooldown']

ART = os.path.join(os.path.dirname(ROOT), 'AlteredBGA', 'img', 'illustrations')
art_index = {}
if os.path.isdir(ART):
    for folder in os.listdir(ART):
        for f in os.listdir(os.path.join(ART, folder)):
            art_index[os.path.splitext(f)[0]] = folder

catalog = {}
for ref, p in cards.items():
    d = {
        'ref': ref,
        'name': p.get('name', ref),
        'type': 'token' if p.get('token') else p.get('type', 'character'),
        'typeline': p.get('typeline', ''),
        'subtypes': p.get('subtypes') or [],
        'faction': p.get('faction', 'NE'),
        'rarity': RARITY.get(p.get('rarity', 0), 'common'),
        'asset': (p.get('asset') or p.get('uid') or ref).replace('_R1', '_R'),
        'effect': p.get('effectDesc', '') or '',
        'keywords': [k for k in KEYWORDS if p.get(k)],
    }
    if d['asset'] in art_index:
        d['art'] = f"{art_index[d['asset']]}/{d['asset']}.jpg"
    for k in ('costHand', 'costReserve', 'forest', 'mountain', 'ocean', 'reserveSlots', 'landmarkSlots'):
        if isinstance(p.get(k), int):
            d[k] = p[k]
    if p.get('supportDesc'):
        d['support'] = p['supportDesc']
    if p.get('flavorText'):
        d['flavor'] = p['flavorText']
    if p.get('artist'):
        d['artist'] = p['artist']
    if p.get('type') == 'hero':
        d['type'] = 'hero'
    catalog[ref] = d

decks = []
for deck_id, lst in sorted(starters.items()):
    hero = next((r for r in lst if cards.get(r, {}).get('type') == 'hero'), None)
    decks.append({
        'id': deck_id,
        'name': cards[hero]['name'] if hero else deck_id,
        'faction': cards[hero]['faction'] if hero else 'NE',
        'heroRef': hero,
        'size': sum(n for r, n in lst.items() if r != hero),
    })

out = os.path.join(ROOT, 'protocol', 'data')
os.makedirs(out, exist_ok=True)
json.dump(catalog, open(os.path.join(out, 'cards.json'), 'w'), ensure_ascii=False, separators=(',', ':'))
json.dump(decks, open(os.path.join(out, 'decks.json'), 'w'), ensure_ascii=False, indent=1)
print(f'{len(catalog)} cards, {len(decks)} decks')
