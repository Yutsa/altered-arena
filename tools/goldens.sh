#!/bin/sh
# Records golden files for every card on every board with the PHP oracle (6 processes):
#   tools/goldens.sh [REGEX] — output in server/goldens/<board>/<card>.json
cd "$(dirname "$0")/.."
python3 -c "
import json,re,sys
d=json.load(open('engine/data/cards.min.json'))
pat=re.compile(sys.argv[1] if len(sys.argv)>1 else '.')
for k in sorted(d):
    if re.match(r'^[A-Z]{2}_(Common|Rare|Unique|Exalted)_', k) and pat.search(k):
        for b in ('rich','empty','reserve'): print(k, b)
" "$1" > /tmp/goldens-todo.txt
wc -l < /tmp/goldens-todo.txt
cat /tmp/goldens-todo.txt | xargs -P 6 -L 1 sh -c 'test -s "server/goldens/$1/$0.json" || docker compose exec -T server php tools/explore.php "$0" "$1" > /dev/null 2>>/tmp/goldens-errors.txt || echo "FAILED $0 $1" >> /tmp/goldens-errors.txt'
ls server/goldens/*/ | wc -l
