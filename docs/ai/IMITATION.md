# Imitation AIs

Crate `imitate/` (`altered-imitate`, command `imitate`). For each player of a set of recorded
games, an AI that plays that player's deck and makes the choices that player would make. The
recorded games are the native replays of `docs/REPLAYS.md`; the player data stay in `private/`.

## Decision dataset (`imitate extract`)

Every step of every native replay where the acting player had two or more candidate answers
(`altered_ai::legal::candidates`) is a decision. The game is rebuilt before the step
(`native::start`, `native::apply`, plus what the step sets before its answer: preferences,
counters, deck order). For each candidate:

- the features below, computed on a copy of the game redealt from that player's point of view
  (`Observation::determinize`: the opponent's hand, both decks and face-down Mana dealt again),
  so they hold only what the player saw;
- the label: the candidate is equivalent to the recorded answer when the game afterwards is the
  same (`oracle_state` and who must play next) or when the two answers are the same once each card
  id is replaced by what the card is (class, place, owner, exhausted, boosts): two copies of a
  card count as one choice.

Single-candidate steps and `act` requests are skipped and counted. The dataset is written once
(`dataset.jsonl`, `names.json`); training never touches the engine. Extraction runs on all cores
(~2 minutes for 70 games, most of it the deck AI).

`imitate baseline` records, for the same decisions, whether the simple bot (Bot.php) and the
deck AI (`ai/params`) give the recorded answer.

## Features (`features.rs`)

Generic only: nothing names a game, a move or a player; a player's specific behaviour is only in
the weights. Families (prefix of the feature names):

- `i:` / `f:` — the change of each evaluation term (`eval::Weights`: progress, Dusk wins,
  margins, board, empty expeditions, hand, Reserve, landmarks, Mana, ready Mana, deck, discard),
  for each side, right after the answer and once the fast policy has finished the action; `f:eval`
  the change of the deck AI's evaluation; `f:win`.
- `a:` — the action class (kind and type: play, end of turn, support, exhaust, select, pass...),
  alone and with the prompt's class (its text with the numbers removed); `a:first` the first
  candidate (the simple bot's answer).
- `c:CLASS:x` — the context crossed with the action class (only differences between the
  candidates of a decision count): day, Mana available and total, hand, Reserve, opponent passed,
  progress lead, Dusk lead, first player, playable cards, opponent's hand.
- card roles (`play`, `mana`, `tgt`, `clean`, `support`, `tap`, `xhand`, `xres`): for each card
  an answer names, its class (`key=`, so per-card preferences are learnable), type, owner, zone,
  cost, power, boosts, exhausted, and the keywords of its texts (`fx=`: `{J}`, `{R}`, `{H}`,
  `<BOOST>`, `[[hand]]`..., so what is learnt on one card carries to cards with the same texts).
  Play: destination, origin, Mana left, share of the Mana, card x destination, card x day; Mana:
  card x day.
- `*rel:` — where the card stands among the cards the decision offers: dearest, cheapest,
  strongest, weakest, its copies, cost against the mean (`playrel:` is extracted but dropped: it
  lowered the score).
- `dst:` — the expedition a character is played into: my and the opposing statistics there, the
  best-biome margin, whether it wins or loses at Dusk, empty, the card's fit to the region's
  biomes, both distances, the side.
- `tgtexp:` — a targeted card's expedition wins or is behind at Dusk.
- `tw:WORD:PROP` — each word of the prompt (`discard`, `sacrifice`, `boost`, `reserve`...)
  crossed with the selected card's owner, place, rank (strongest/weakest...), power and cost:
  players discard their weakest card and boost their strongest.
- `choice:`, `exp:`, `loc:`, `num:`, `player:`, `btn:` — the choice's label and source card, the
  side and owner of an expedition or location, the die value.
- `s:pick`, `sv:` — the deck AI (`search::Ai` with the deck's parameters **without its card
  priors**, which were fit on the replays) on the same decision: its pick, each candidate's 1-ply
  score and rollout score against the best.

## Model (`model.rs`)

Linear score of each candidate, softmax over the candidates of a decision, maximum likelihood of
the recorded answer (a decision with several equivalent candidates counts the sum of their
probabilities). Two levels: shared weights `w` and per-player offsets `u_p`; a candidate of
player p scores `x . (w + u_p)`. L2 regularization `l_shared/2 |w|^2 + l_player/2 sum |u_p|^2`,
so a player with one or two games stays close to the shared model. Features are divided by
their typical size where non-zero. L-BFGS from zero, at most 300 iterations.

## Score (`cv.rs`, `imitate cv`)

Leave-one-game-out: for each game, the model (shared weights and offsets) is trained on all the
other games and predicts both players' decisions in the held-out game. Reported per player and
overall: top-1 agreement (the best candidate is equivalent to the recorded answer, the main
metric), top-3, mean log-likelihood, by decision kind and day, and the training-set agreement.
Overall score: the mean of the per-player top-1 (each player counts the same), and the
decision-weighted mean. A decision whose recorded answer is no candidate counts as a miss.

`--drop P1,P2` leaves out the features whose names start with a prefix (`*x`: end with x, `~x`:
contain x), so a feature family is tested without re-extracting. `--l-shared`, `--l-player`,
`--player-mult PREFIX:X` (per-family offset regularization), `--cand-power` tune the training.

## The AIs (`agent.rs`, `imitate fit`)

`imitate fit` trains on every game and writes the shared weights (`shared.json`, in raw feature
units, with a version) and one file per player (`players/*.json`: the shared version, the
player, the deck, the hero, the label "player — hero", the offsets). `agent::Imitator` (a
`play::Agent`) adds the player's offsets to the shared weights, runs the deck AI when the model
reads its pick, extracts the features of every candidate and plays the best one (or draws from
the softmax with `temperature`). The repeated effect-order guard of `search::Ai` applies.

## Gates (`imitate gate`)

Each AI plays N full games (default 20) against the deck AIs of other decks, mixed opponents,
fixed seeds, seats alternated; the player's deck AI plays the same matchups. Reported: errors
(panics, stuck games), refused answers, the win rate of both, and AIs under 15 % wins.

## Method of improvement

Each change was one generic feature family or training setting, kept when the held-out player
mean rose and no more than a quarter of the players lost more than 2 points (`imitate report`
writes the report; the journal of the changes stays with the private data). Comparisons are
made within one extraction: redealing the hidden cards with another seed moves the score by about
half a point.
