#!/usr/bin/env python3
"""Prints a card's data (effects, triggers) from the engine export: tools/card.py REF_SUBSTRING"""
import json, sys
d = json.load(open('rules/data/cards.min.json'))
cards = d if isinstance(d, list) else d.get('cards', d)
items = cards.items() if isinstance(cards, dict) else enumerate(cards)
for k, c in items:
    ref = c.get('ref', k) if isinstance(c, dict) else k
    if sys.argv[1].lower() in str(ref).lower() or sys.argv[1].lower() in str(c.get('name', '')).lower():
        print(ref, json.dumps(c, indent=1)[:int(sys.argv[2]) if len(sys.argv) > 2 else 4000])
