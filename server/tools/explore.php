<?php

/**
 * Golden files: plays one card on a prepared board in the original engine and explores every
 * branch of the decisions it leads to (bounded), recording at each step the decision asked
 * (protocol v1) and the full state. The Rust engine replays these trees (engine tests).
 *
 *   php tools/explore.php CARD_REF [BOARD] [OUT_DIR]
 *
 * BOARD: rich (default: characters, Reserve, Landmarks on both sides), empty (only the card),
 * reserve (rich board, the card starts in Reserve).
 *
 * Whole games: two starter decks (chosen from SEED), the position right after deck selection,
 * then random answers for both players until the game ends; each step is a node with one `next`.
 *
 *   php tools/explore.php game SEED [OUT_DIR] [--keep] [--dump=STEP,...]
 *
 * Output in OUT_DIR/games/SEED.json (OUT_DIR defaults to server/goldens). --keep keeps the
 * game's database (to read its log with tools/php_log.py). --dump writes the tables before the given
 * steps to /tmp/golden-dumps/SEED.stepN.json (in the container) (to compare globals with the other engine).
 *
 * Replays of real games: a game recorded on Board Game Arena (AlteredBGA misc/harness/replays/*.json),
 * played again in the oracle with the requests its players sent (rebuilt by the harness's
 * ReplayRunner, REPLAY_HARNESS = its tests/Replay directory, default /harness/tests/Replay)
 *
 *   php tools/explore.php replay BGA_REPLAY.json [OUT_DIR] [--keep]
 *
 * Output in OUT_DIR/replays/TABLE.json, in the format of whole games (see playReplay).
 */

require dirname(__DIR__) . '/src/bootstrap.php';

use Arena\Arena;
use Arena\V1;
use ALT\Managers\Cards;

const MAX_DEPTH = 30;
const MAX_NODES = 400;
const MAX_ANSWERS = 8;
const DICE = [3, 1, 6, 4, 2, 5];
const DECKS = ['ax_1_sierra', 'br_1_kojo'];

$nodes = 0;

/** Answers the setup (decks are chosen at creation; mana: the first cards offered) */
function setupGame(): int
{
  $g = (int) V1::createGame(['players' => [['name' => 'A', 'deck' => DECKS[0]], ['name' => 'B', 'deck' => DECKS[1]]], 'dice' => DICE])['gameId'];
  for ($i = 0; $i < 10; $i++) {
    $w = V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'] ?? [];
    if (empty($w)) {
      break;
    }
    $d = V1::view($g, $w[0])['decision'];
    if ($d['kind'] == 'turn') {
      return $g;
    }
    $answer = ['decisionId' => $d['id']] + match ($d['kind']) {
      'select' => ['type' => 'select', 'cardIds' => array_slice($d['cardIds'], 0, $d['max'])],
      'confirm' => ['type' => 'confirm'],
      default => ['type' => 'pass'],
    };
    V1::answer($g, $w[0], $answer);
  }
  return $g;
}

function create(int $pId, string $ref, string $location, int $state = 0): int
{
  $class = '\\ALT\\Cards\\' . substr($ref, 0, 2) . '\\' . $ref;
  $card = new $class(null);
  $id = (int) Cards::singleCreate(['player_id' => $pId, 'location' => $location, 'nbr' => 1, 'properties' => $card->getProperties()])->getId();
  if ($state) {
    \BgaDb::conn()->query("UPDATE cards SET card_state = $state WHERE card_id = $id");
  }
  Cards::invalidate();
  return $id;
}

/** The position every exploration starts from; returns [me, card id] */
function prepare(int $g, string $ref, string $board): array
{
  $me = (int) V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'][0];
  $opp = $me == Arena::PLAYER_IDS[0] ? Arena::PLAYER_IDS[1] : Arena::PLAYER_IDS[0];
  return Arena::inRequest($g, null, function () use ($me, $opp, $ref, $board) {
    $cardId = create($me, $ref, $board == 'reserve' ? RESERVE : HAND);
    // Mana for anything: 10 ready orbs each
    foreach ([$me, $opp] as $p) {
      for ($i = 0; $i < 7; $i++) {
        create($p, 'BR_Common_Kappa', MANA);
      }
    }
    \BgaDb::conn()->query("UPDATE cards SET card_state = 0 WHERE card_location = '" . MANA . "'");
    if ($board != 'empty') {
      create($me, 'BR_Common_DevotedProtector', STORM_LEFT);
      create($me, 'AX_Common_IraFairAttendee', STORM_RIGHT);
      create($me, 'MU_Common_ColorfulCuckoo', RESERVE);
      create($me, 'BR_Common_Kappa', RESERVE);
      create($me, 'OD_Rare_PicnicArea', LANDMARK);
      create($me, 'BR_Common_Kappa', DISCARD_PILE);
      create($opp, 'AX_Common_IraFairAttendee', STORM_LEFT);
      create($opp, 'BR_Common_Kappa', STORM_RIGHT);
      create($opp, 'BR_Common_Tengu', RESERVE);
      create($opp, 'NE_Common_Manaseed', LANDMARK);
      create($opp, 'BR_Common_Kappa', DISCARD_PILE);
    }
    return [$me, $cardId];
  });
}

/** What the engines must agree on: the decision without its id */
function slim(?array $d): ?array
{
  if ($d === null) {
    return null;
  }
  unset($d['id']);
  return $d;
}

/**
 * State in a compact form: card ids per player zone (deck in order, top last; other zones by id),
 * the cards' non-default marks, expedition positions. The first node keeps every card's ref.
 */
function compactState(array $state, bool $refs = false): array
{
  $zones = [];
  $marks = [];
  $byOrder = $state['cards'];
  usort($byOrder, fn($a, $b) => [$a['order'], $a['id']] <=> [$b['order'], $b['id']]);
  foreach ($byOrder as $c) {
    $zones[$c['owner'] . ':' . $c['zone']][] = $c['id'];
    $m = [];
    if ($c['exhausted'] && !in_array($c['zone'], ['deck', 'hand', 'discard'])) {
      $m['x'] = 1;
    }
    if ($c['boosts']) {
      $m['b'] = $c['boosts'];
    }
    if ($c['statuses']) {
      $m['s'] = $c['statuses'];
    }
    if ($c['counter']) {
      $m['c'] = $c['counter'];
    }
    if ($m) {
      $marks[$c['id']] = $m;
    }
  }
  foreach ($zones as $k => &$ids) {
    if (!str_ends_with($k, ':deck')) {
      sort($ids);
    }
  }
  $out = [
    'zones' => $zones,
    'marks' => (object) $marks,
    'positions' => array_column($state['players'], 'position', 'id'),
    'day' => $state['day'],
    'phase' => $state['phase'],
    'first' => $state['first'],
    'storm' => array_map(fn($t) => (int) $t['revealed'], $state['storm']),
  ];
  if ($refs) {
    $out['refs'] = array_column($state['cards'], 'ref', 'id');
  }
  return $out;
}

function combos(array $items, int $k): array
{
  if ($k == 0) {
    return [[]];
  }
  $out = [];
  foreach ($items as $i => $x) {
    foreach (combos(array_slice($items, $i + 1), $k - 1) as $rest) {
      $out[] = array_merge([$x], $rest);
      if (count($out) >= MAX_ANSWERS) {
        return $out;
      }
    }
  }
  return $out;
}

/** The answers explored at a decision (a sample when there are too many) */
function answers(array $d, ?int $onlyCard): array
{
  $a = [];
  switch ($d['kind']) {
    case 'turn':
      foreach ($d['play'] as $p) {
        if ($onlyCard === null || $p['cardId'] == $onlyCard) {
          foreach ($p['destinations'] as $dest) {
            $a[] = ['type' => 'play', 'cardId' => $p['cardId'], 'destination' => $dest];
          }
        }
      }
      break;
    case 'select':
      for ($k = $d['min']; $k <= $d['max'] && $k <= count($d['cardIds']); $k++) {
        foreach (combos($d['cardIds'], $k) as $ids) {
          $a[] = ['type' => 'select', 'cardIds' => $ids];
        }
      }
      break;
    case 'choice':
      foreach ($d['choices'] as $c) {
        if ($c['enabled']) {
          $a[] = ['type' => 'choice', 'id' => $c['id']];
        }
      }
      break;
    case 'expedition':
      foreach (combos(array_column($d['expeditions'], 'key'), max(1, $d['min'])) as $keys) {
        $a[] = ['type' => 'expedition', 'keys' => $keys];
      }
      break;
    case 'player':
      foreach ($d['players'] as $p) {
        $a[] = ['type' => 'player', 'player' => $p];
      }
      break;
    case 'number':
      foreach ($d['values'] as $v) {
        $a[] = ['type' => 'number', 'value' => $v];
      }
      break;
    case 'location':
      foreach ($d['locations'] as $l) {
        $a[] = ['type' => 'location', 'key' => $l['key']];
      }
      break;
    case 'buttons':
      foreach ($d['buttons'] as $b) {
        $a[] = ['type' => 'button', 'key' => $b['key']];
      }
      break;
    case 'exchange':
      foreach (array_slice($d['hand'], 0, 2) as $h) {
        foreach (array_slice($d['reserve'], 0, 2) as $r) {
          $a[] = ['type' => 'exchange', 'hand' => $h, 'reserve' => $r];
        }
      }
      break;
    case 'cleanup':
      $a[] = ['type' => 'cleanup', 'reserve' => array_slice($d['reserve']['cardIds'], 0, $d['reserve']['n']), 'landmarks' => array_slice($d['landmarks']['cardIds'], 0, $d['landmarks']['n'])];
      break;
    case 'confirm':
      $a[] = ['type' => 'confirm'];
      break;
  }
  if (!empty($d['canPass']) && $d['kind'] != 'turn') {
    $a[] = ['type' => 'pass'];
  }
  return array_slice($a, 0, MAX_ANSWERS);
}

function explore(int $g, int $depth, ?int $onlyCard): array
{
  global $nodes;
  $nodes++;
  $state = compactState(Arena::oracleState($g));
  $w = V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'] ?? [];
  if (empty($w)) {
    return ['state' => $state, 'end' => 'nobody to play'];
  }
  $p = (int) $w[0];
  $d = V1::view($g, $p)['decision'];
  $node = ['player' => $p, 'decision' => slim($d), 'state' => $state];
  if ($d === null || ($depth > 0 && $d['kind'] == 'turn')) {
    return $node;
  }
  if ($depth >= MAX_DEPTH || $nodes >= MAX_NODES) {
    return $node + ['end' => 'truncated'];
  }
  $snap = "x$depth";
  \BgaDb::saveSnapshot($snap);
  $node['next'] = [];
  foreach (answers($d, $depth == 0 ? $onlyCard : null) as $a) {
    \BgaDb::restoreSnapshot($snap);
    Arena::openGame($g);
    try {
      V1::answer($g, $p, $a + ['decisionId' => $d['id']]);
    } catch (\Throwable $e) {
      $node['next'][] = ['answer' => $a, 'error' => substr($e->getMessage(), 0, 200)];
      continue;
    }
    $node['next'][] = ['answer' => $a, 'node' => explore($g, $depth + 1, null)];
  }
  return $node;
}

/**
 * The original engine's own tables, for an engine that loads them as they are, and what the
 * framework keeps besides: the `global` table (1 = state, 2 = active player, 10 = logging), the
 * next ids of cards and meeples, and arena_rng: the position in the dice list and the seed of the
 * next act (game id, acts so far: Arena::seedRng)
 */
function dumpTables(): array
{
  $db = [];
  foreach (['cards', 'meeples', 'global_variables', 'player', 'user_preferences'] as $table) {
    $db[$table] = \BgaDb::conn()->query("SELECT * FROM `$table` ORDER BY 1")->fetch_all(MYSQLI_ASSOC);
  }
  $db['global'] = \BgaDb::conn()->query("SELECT * FROM `global` ORDER BY 1")->fetch_all(MYSQLI_ASSOC);
  $db['arena_rng'] = \BgaDb::conn()->query("SELECT * FROM `arena_rng`")->fetch_all(MYSQLI_ASSOC);
  $db['auto_increment'] = [];
  foreach (['cards', 'meeples'] as $table) {
    $db['auto_increment'][$table] = (string) \BgaDb::conn()->query("SELECT AUTO_INCREMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '$table'")->fetch_row()[0];
  }
  return $db;
}

/** Drops the game's database and its snapshots */
function dropGame(int $g): void
{
  $db = \BgaDb::conn();
  foreach ($db->query("SHOW DATABASES LIKE 'arena_g{$g}%'")->fetch_all() as [$name]) {
    if ($name === "arena_g$g" || str_starts_with($name, "arena_g{$g}_")) {
      $db->query("DROP DATABASE `$name`");
    }
  }
}

// ---------------------------------------------------------------------------
// Whole games
// ---------------------------------------------------------------------------

const GAME_MAX_STEPS = 3000;
const GAME_DICE = 400;

/** $n items of $items picked at random, in their original order */
function pickSome(\Random\Randomizer $r, array $items, int $n): array
{
  $items = array_values($items);
  $n = max(0, min($n, count($items)));
  if ($n == 0) {
    return [];
  }
  $keys = $r->pickArrayKeys($items, $n);
  return array_map(fn($k) => $items[$k], $keys);
}

/** A random answer to a decision (turn: play something ~80% of the time, else end the turn) */
function randomAnswer(\Random\Randomizer $r, array $d): array
{
  $pass = ['type' => 'pass'];
  if (!empty($d['canPass']) && $d['kind'] != 'turn' && $r->getInt(1, 4) == 1) {
    return $pass;
  }
  switch ($d['kind']) {
    case 'turn':
      $actions = [];
      foreach ($d['play'] as $p) {
        if (!empty($p['destinations'])) {
          $actions[] = ['type' => 'play', 'cardId' => $p['cardId'], 'destination' => $p['destinations'][$r->getInt(0, count($p['destinations']) - 1)]];
        }
      }
      foreach ($d['support'] ?? [] as $id) {
        $actions[] = ['type' => 'support', 'cardId' => $id];
      }
      foreach ($d['tap'] ?? [] as $id) {
        $actions[] = ['type' => 'tap', 'cardId' => $id];
      }
      $stop = !empty($d['canEndTurn']) ? ['type' => 'endTurn'] : (!empty($d['canPass']) ? $pass : null);
      if ($actions && ($stop === null || $r->getInt(1, 100) <= 80)) {
        return $actions[$r->getInt(0, count($actions) - 1)];
      }
      return $stop ?? ['type' => 'endTurn'];
    case 'select':
      $max = min($d['max'], count($d['cardIds']));
      $min = min($d['min'], $max);
      return ['type' => 'select', 'cardIds' => pickSome($r, $d['cardIds'], $r->getInt($min, $max))];
    case 'choice':
      $enabled = array_values(array_filter($d['choices'], fn($c) => $c['enabled']));
      return $enabled ? ['type' => 'choice', 'id' => $enabled[$r->getInt(0, count($enabled) - 1)]['id']] : $pass;
    case 'expedition':
      $keys = array_column($d['expeditions'], 'key');
      $max = min($d['max'], count($keys));
      $min = min(max(1, $d['min']), $max);
      return ['type' => 'expedition', 'keys' => pickSome($r, $keys, $r->getInt($min, $max))];
    case 'player':
      return ['type' => 'player', 'player' => $d['players'][$r->getInt(0, count($d['players']) - 1)]];
    case 'number':
      return ['type' => 'number', 'value' => $d['values'][$r->getInt(0, count($d['values']) - 1)]];
    case 'location':
      return ['type' => 'location', 'key' => $d['locations'][$r->getInt(0, count($d['locations']) - 1)]['key']];
    case 'buttons':
      return $d['buttons'] ? ['type' => 'button', 'key' => $d['buttons'][$r->getInt(0, count($d['buttons']) - 1)]['key']] : $pass;
    case 'exchange':
      if (!$d['hand'] || !$d['reserve']) {
        return $pass;
      }
      return ['type' => 'exchange', 'hand' => $d['hand'][$r->getInt(0, count($d['hand']) - 1)], 'reserve' => $d['reserve'][$r->getInt(0, count($d['reserve']) - 1)]];
    case 'cleanup':
      return ['type' => 'cleanup', 'reserve' => pickSome($r, $d['reserve']['cardIds'], $d['reserve']['n']), 'landmarks' => pickSome($r, $d['landmarks']['cardIds'], $d['landmarks']['n'])];
    case 'confirm':
      return ['type' => 'confirm'];
  }
  return $pass;
}

/** Plays a whole game with random answers; writes OUT/games/SEED.json */
function playGame(int $seed, string $out, bool $keep, array $dumpSteps = []): void
{
  $t = microtime(true);
  $r = new \Random\Randomizer(new \Random\Engine\Mt19937($seed));
  $decks = array_column(V1::decks(), 'id');
  $pair = [$decks[$r->getInt(0, count($decks) - 1)], $decks[$r->getInt(0, count($decks) - 1)]];
  $dice = array_map(fn() => $r->getInt(1, 6), range(1, GAME_DICE));
  $g = (int) V1::createGame(['players' => [['name' => 'A', 'deck' => $pair[0]], ['name' => 'B', 'deck' => $pair[1]]], 'dice' => $dice])['gameId'];

  // The position right after deck selection
  Arena::openGame($g);
  $initial = Arena::oracleState($g);
  $db = dumpTables();

  // Random events other than dice (deck reshuffles, a random card of a hand; seeded per act by
  // Arena::seedRng): record where they happen, to tell such divergences apart
  $shuffles = [];
  $prev = \Harness\Rng::$shuffle;
  \Harness\Rng::$shuffle = function (array $a) use (&$shuffles, $prev): ?array {
    $forced = $prev !== null ? $prev($a) : null;
    if ($forced === null) {
      $forced = $a;
      \shuffle($forced);
    }
    $shuffles[] = $forced;
    return $forced;
  };
  $random = [];
  $notif = fn() => (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM arena_notif')->fetch_row()[0];

  $tree = [];
  $node = &$tree;
  $steps = 0;
  $end = null;
  while (true) {
    if (in_array($steps, $dumpSteps, true)) {
      Arena::openGame($g);
      @mkdir("/tmp/golden-dumps");
      file_put_contents("/tmp/golden-dumps/$seed.step$steps.json", json_encode(dumpTables(), JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE));
    }
    $state = compactState(Arena::oracleState($g));
    $w = V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'] ?? [];
    if (empty($w)) {
      $node = ['state' => $state, 'end' => $end = 'nobody to play'];
      break;
    }
    $p = (int) $w[0];
    $d = V1::view($g, $p)['decision'];
    $node = ['player' => $p, 'decision' => slim($d), 'state' => $state];
    if ($d === null) {
      $end = 'no decision';
      break;
    }
    if ($steps >= GAME_MAX_STEPS) {
      $node['end'] = $end = 'truncated';
      break;
    }
    $node['next'] = [];
    $done = false;
    for ($try = 0; $try < 20 && !$done; $try++) {
      $a = randomAnswer($r, $d);
      $nShuffles = count($shuffles);
      $before = $notif();
      try {
        V1::answer($g, $p, $a + ['decisionId' => $d['id']]);
      } catch (\Throwable $e) {
        $node['next'][] = ['answer' => $a, 'error' => substr($e->getMessage(), 0, 200)];
        continue;
      }
      $done = true;
      if (count($shuffles) > $nShuffles) {
        $random[] = ['step' => $steps, 'kind' => 'shuffle'];
      }
      $hand = (int) \BgaDb::conn()->query("SELECT COUNT(*) FROM arena_notif WHERE id > $before AND name = 'revealHand'")->fetch_row()[0];
      if ($hand) {
        $random[] = ['step' => $steps, 'kind' => 'random hand card'];
      }
      $node['next'][] = ['answer' => $a, 'node' => []];
      $node = &$node['next'][count($node['next']) - 1]['node'];
    }
    if (!$done) {
      $end = 'stuck';
      break;
    }
    $steps++;
  }
  unset($node);
  \Harness\Rng::$shuffle = $prev;

  $final = Arena::oracleState($g);
  @mkdir("$out/games", 0777, true);
  file_put_contents("$out/games/$seed.json", json_encode([
    'mode' => 'game',
    'seed' => $seed,
    'decks' => $pair,
    'phpGame' => $g,
    'steps' => $steps,
    'end' => $end,
    'day' => $final['day'],
    'state' => $final['state'],
    'random' => $random,
    'shuffles' => $shuffles,
    'dice' => $dice,
    'initial' => $initial,
    'db' => $db,
    'tree' => $tree,
  ], JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE, 100000));
  printf("game %d (%s vs %s): %d steps, %s, day %d, %s%s, %.1fs (php game %d)\n", $seed, $pair[0], $pair[1], $steps, $end, $final['day'], $final['state'],
    $random ? ', random events: ' . count($random) : '', microtime(true) - $t, $g);
  if (!$keep) {
    dropGame($g);
  }
}

// ---------------------------------------------------------------------------
// Replays of real games (Board Game Arena)
// ---------------------------------------------------------------------------

/**
 * A BGA replay ("logs" request of a finished table, see AlteredBGA misc/harness/README.md) with
 * its players made anonymous: ids become Arena::PLAYER_IDS in seat order, names A and B, and
 * the API decks lose their name and id (the id embeds the player's name and id). The engine only
 * stores these, so the replay still matches.
 */
function anonymizeReplay(array $data): array
{
  $map = [];
  $names = [];
  foreach (array_values($data['players']) as $i => $p) {
    $map[(string) $p['id']] = Arena::PLAYER_IDS[$i];
    $names[$p['name']] = chr(65 + $i);
  }
  $re = '/(?<![0-9])(' . implode('|', array_map(fn($id) => preg_quote((string) $id, '/'), array_keys($map))) . ')(?![0-9])/';
  $ids = fn(string $s) => preg_replace_callback($re, fn($m) => (string) $map[$m[1]], $s);
  $walk = function ($v) use (&$walk, $map, $names, $ids) {
    if (is_array($v)) {
      $out = [];
      foreach ($v as $k => $x) {
        $out[is_int($k) ? ($map[(string) $k] ?? $k) : $ids($k)] = $walk($x);
      }
      if (isset($out['deckName'], $out['cards'])) {
        $out['deckName'] = 'API deck';
        $out['id'] = 'api-deck';
      }
      return $out;
    }
    if (is_int($v)) {
      return $map[(string) $v] ?? $v;
    }
    if (is_string($v)) {
      return $names[$v] ?? $ids($v);
    }
    return $v;
  };
  return $walk($data);
}

/**
 * The requests of a replay, rebuilt by the AlteredBGA harness's ReplayRunner (each request is
 * searched among the actions the state allows, until its notifications are the recorded ones),
 * run here on the oracle's engine in a scratch database.
 * @return array{0: bool, 1: array, 2: ?array, 3: int} [replayed to the end, played actions ([move, action label]), divergence, requests]
 */
function recoverRequests(string $file, int $table): array
{
  $db = "arena_replay_$table";
  putenv("DB_NAME=$db");
  \BgaDb::resetDatabase(GAME_DIR);
  \Table::harnessResetGlobalsCache();
  \Table::$notifications = [];
  $runner = new \ReplayRunner(new \Replay($file), [OPTION_UNDO => 1]);
  ob_start();
  try {
    $ok = $runner->run();
  } finally {
    ob_end_clean();
  }
  \BgaDb::conn()->query("DROP DATABASE `$db`");
  putenv('DB_NAME');
  \Harness\Rng::$shuffle = null;
  \Harness\Rng::$randInt = null;
  return [$ok, $runner->played, $runner->divergence, $runner->requestCount()];
}

/** Game methods the runner calls directly; the other actions go through actTakeAtomicAction */
const REPLAY_CALLS = ['actConfirmAPIDeck', 'actFirstDayManaSelection', 'actChooseAction', 'actConfirmTurn', 'actConfirmPartialTurn', 'actPassOptionalAction', 'actRestart'];

/** The protocol v1 answer to $d that sends the action the runner found ($method, $args) */
function replayAnswer(string $method, array $args, array $d): array
{
  $dest = ['stormLeft' => 'heroExpedition', 'stormRight' => 'companionExpedition', 'landmark' => 'landmark', 'reserve' => 'reserve',
    'limbo' => 'spell', 'stormLeft_scout' => 'heroExpeditionScout', 'stormRight_scout' => 'companionExpeditionScout'];
  $ints = fn($a) => array_map('intval', (array) $a);
  switch ($method) {
    case 'actFirstDayManaSelection':
    case 'actTarget':
    case 'actDiscard':
      return ['type' => 'select', 'cardIds' => $ints($args[0])];
    case 'actPlay':
    case 'actPlayCard':
      return ['type' => 'play', 'cardId' => (int) $args[0], 'destination' => $dest[$args[1]] ?? $args[1]];
    case 'actSupport':
      return ['type' => 'support', 'cardId' => (int) $args[0]];
    case 'actTap':
      return ['type' => 'tap', 'cardId' => (int) $args[0]];
    case 'actPass':
      return ['type' => 'endTurn'];
    case 'actChooseAction':
      return ['type' => 'choice', 'id' => (int) $args[0]];
    case 'actConfirmTurn':
    case 'actConfirmPartialTurn':
      return ['type' => 'confirm'];
    case 'actPassOptionalAction':
      return ['type' => 'pass'];
    case 'actRestart':
      return ['type' => 'undo'];
    case 'actNightCleanup':
      return ['type' => 'cleanup', 'reserve' => $ints($args[0]), 'landmarks' => $ints($args[1])];
    case 'actInvokeToken':
      // v1 keys name the expedition's owner (stormLeft-<pId>); the client may send it without
      $keys = array_column($d['locations'] ?? [], 'key');
      $key = in_array($args[0], $keys, true) ? $args[0] : (current(array_filter($keys, fn($k) => str_starts_with($k, $args[0] . '-'))) ?: $args[0]);
      return ['type' => 'location', 'key' => $key];
    case 'actTargetExpedition':
      return ['type' => 'expedition', 'keys' => (array) $args[0]];
    case 'actMoveExpedition':
      return ['type' => 'expedition', 'keys' => [json_encode($args)]];
    case 'actBlockExpedition':
      return ['type' => 'expedition', 'keys' => [$args[0]]];
    case 'actMarkRegion':
      return ['type' => 'button', 'key' => $args[0] . ':' . $args[1]];
    case 'actMoveRegionMarker':
      return ['type' => 'button', 'key' => (string) $args[0]];
    case 'actInteruptReveal':
      return ['type' => 'button', 'key' => 'ok'];
    case 'actTargetPlayer':
      return ['type' => 'player', 'player' => (int) $args[0]];
    case 'actRollDie':
    case 'actPay':
    case 'actSpend':
      return ['type' => 'number', 'value' => (int) $args[0]];
    case 'actExchange':
      return ['type' => 'exchange', 'reserve' => (int) $args[0], 'hand' => (int) $args[1]];
  }
  throw new \Exception("No protocol v1 answer for $method");
}

/** Notifications stored since $after, grouped like a replay's (per channel, ignored types removed) */
function notificationsSince(int $after): array
{
  $out = [];
  foreach (\BgaDb::conn()->query("SELECT target, name, args FROM arena_notif WHERE id > $after ORDER BY id")->fetch_all(MYSQLI_ASSOC) as $n) {
    if (!in_array($n['name'], \Replay::IGNORED)) {
      $out[$n['target'] === null ? 'table' : (int) $n['target']][] = ['type' => $n['name'], 'args' => json_decode($n['args'], true)];
    }
  }
  return $out;
}

/**
 * Order in which a player's cards first show up from move $from, with the order within a move
 * corrected where the replay could not tell it ($corrections: move => player => card ids)
 */
function revealOrder(\Replay $replay, int $pId, int $from, array $corrections): array
{
  $order = $replay->revealOrder($pId, $from);
  foreach ($corrections as $move => $byPlayer) {
    if ($move >= $from && !empty($byPlayer[$pId])) {
      $positions = array_keys(array_filter($order, fn($id) => in_array($id, $byPlayer[$pId])));
      foreach ($positions as $i => $pos) {
        $order[$pos] = $byPlayer[$pId][$i];
      }
    }
  }
  return $order;
}

/** Every merge of the lists that keeps the order within each list */
function interleavings(array $lists): array
{
  $lists = array_values(array_filter($lists));
  if (count($lists) <= 1) {
    return [$lists[0] ?? []];
  }
  $out = [];
  foreach ($lists as $i => $list) {
    $rest = $lists;
    $rest[$i] = array_slice($list, 1);
    foreach (interleavings($rest) as $tail) {
      $out[] = array_merge([$list[0]], $tail);
    }
  }
  return array_values(array_unique($out, SORT_REGULAR));
}

/** Card ids of a player's deck, bottom first */
function deckIds(int $pId): array
{
  return array_map('intval', array_column(\BgaDb::conn()->query("SELECT card_id FROM cards WHERE card_location = 'deck-$pId' ORDER BY card_state, card_id")->fetch_all(), 0));
}

/**
 * Cards that leave a player's deck in a request but show up on different channels (a draw to hand
 * and a card revealed to both players): packets of different channels are not in chronological
 * order, so the order they left the deck in is unknown. player => [one list per channel]
 */
function ambiguousDraws(\Replay $replay, int $move): array
{
  $out = [];
  foreach ($replay->players as $pId) {
    $deck = deckIds($pId);
    $lists = array_values(array_filter(array_map(fn($ids) => array_values(array_intersect($ids, $deck)), $replay->revealedIn($pId, $move))));
    if (count($lists) > 1) {
      $out[$pId] = $lists;
    }
  }
  return $out;
}

/**
 * One run of a replay in the arena (see playReplay). Returns the golden file's data, or
 * ['correction' => [move, player => card ids]] when a request only matches BGA with the cards
 * of an ambiguous draw leaving the deck in another order: the run must start again with it.
 */
function arenaReplay(\Replay $replay, array $played, array $corrections): array
{
  // Dice: every roll of the replay, in order
  $dice = [];
  foreach ($replay->moves as $channels) {
    foreach ($channels['table'] ?? [] as $n) {
      if ($n['type'] == 'roll') {
        foreach ((array) ($n['args']['rolls'] ?? []) as $roll) {
          $dice[] = (int) $roll;
        }
      }
    }
  }
  // Deck shuffles: the cards come out in the order the replay shows them (as ReplayRunner forces
  // them); those after the dump are recorded for the other engine (injectShuffles)
  $move = 0;
  $recording = false;
  $injected = [];
  \Harness\Rng::$shuffle = function (array $a) use ($replay, &$move, &$recording, &$injected, $corrections): ?array {
    foreach ($replay->players as $pId) {
      $known = array_values(array_filter(revealOrder($replay, $pId, $move, $corrections), fn($id) => in_array($id, $a)));
      if (!empty($known)) {
        $unknown = array_values(array_filter($a, fn($id) => !in_array((int) $id, $known)));
        $order = array_map('intval', array_merge($unknown, array_reverse($known)));
        if ($recording) {
          $injected[] = $order;
        }
        return $order;
      }
    }
    return null;
  };
  // A dice list makes the game run in the 'prod' environment, as on BGA
  $g = (int) Arena::createGame(['players' => [['name' => 'A'], ['name' => 'B']], 'dice' => $dice ?: [1]])['id'];
  $storm = $replay->storm();
  Arena::inRequest($g, null, function () use ($replay, $storm) {
    \ALT\Core\Globals::setTokenStyles($replay->tokenStyles());
    // The runner sets the undo preference it can infer and leaves the others unset (null). A view
    // (getAllDatas) would create the missing ones with their defaults: write each one, with 0 where
    // the runner had null (the engine only compares them with == to values that 0 and null are
    // both equal to (OPTION_CONFIRM_DISABLED) or both different from)
    $inferred = $replay->preferences();
    foreach ($replay->players as $pId) {
      foreach ([OPTION_CONFIRM, OPTION_CONFIRM_UNDOABLE, OPTION_PLAYER_UNDO] as $prefId) {
        $value = (int) ($inferred[$pId][$prefId] ?? 0);
        \BgaDb::query("INSERT INTO user_preferences (player_id, pref_id, pref_value) VALUES ($pId, $prefId, $value)");
      }
    }
    // Tumult cards revealed during the game (the others keep their random draw)
    $tiles = \ALT\Core\Globals::getStorm();
    $rest = array_values(array_diff([1, 2, 3], array_column($storm, 0)));
    for ($i = 1; $i <= 3; $i++) {
      $tiles[$i]['cardId'] = $storm[$i][0] ?? array_shift($rest);
      if (isset($storm[$i])) {
        $tiles[$i]['rotated'] = $storm[$i][1];
      }
    }
    \ALT\Core\Globals::setStorm($tiles);
  });

  $initial = $db = null;
  $tree = [];
  $node = &$tree;
  $steps = 0;
  $end = null;
  $mismatches = [];
  $moves = array_count_values(array_column($played, 'move'));
  // Rows created then reverted by an undo consumed ids on BGA: after an undo, the next new card or
  // meeple id the replay shows tells where the counter was (as ReplayRunner::prepareRandomness)
  $firstNewIds = $replay->firstNewIds();
  $afterUndo = [];
  $nextIds = [];
  $lastMove = null;
  foreach ($played as $entry) {
    $label = $entry['action'];
    if ($db !== null && $entry['move'] !== $lastMove) {
      Arena::openGame($g);
      foreach ($afterUndo ? ($firstNewIds[$entry['move']] ?? []) : [] as $t => $id) {
        unset($afterUndo[$t]);
        $next = (int) \BgaDb::conn()->query("SELECT AUTO_INCREMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '$t'")->fetch_row()[0];
        $maxId = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(' . ($t == 'cards' ? 'card_id' : 'meeple_id') . "), 0) FROM `$t`")->fetch_row()[0];
        if ($id > $next && $id > $maxId) {
          \BgaDb::conn()->query("ALTER TABLE `$t` AUTO_INCREMENT = $id");
          $nextIds[$t] = $id;
        }
      }
    }
    $lastMove = $entry['move'];
    // An undone request whose state was already reached
    if (str_starts_with($label, 'erased: no-op')) {
      $afterUndo = ['cards' => true, 'meeples' => true];
      continue;
    }
    $erased = str_starts_with($label, 'erased: ');
    preg_match('/^(?:erased: )?(\w+) (.*)$/s', $label, $m);
    $method = $m[1];
    if ($method == 'actConfirmAPIDeck') {
      // Deck selection (no v1 answer: decks are chosen at creation), before the dump
      foreach ($replay->players as $pId) {
        $deck = $replay->apiDeck($pId);
        if (str_starts_with('actConfirmAPIDeck ' . json_encode([$deck]), $label)) {
          Arena::act($g, $pId, 'actConfirmAPIDeck', [$deck]);
        }
      }
      continue;
    }
    $move = $entry['move'];
    $args = json_decode($m[2], true);
    if (!is_array($args)) {
      throw new \Exception("Cannot read the request of move $move: $label");
    }
    if ($db === null) {
      // The position right after deck selection, as in whole games
      Arena::openGame($g);
      $initial = Arena::oracleState($g);
      $db = dumpTables();
      $recording = true;
    }

    $full = Arena::oracleState($g);
    $w = V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'] ?? [];
    $p = (int) ($w[0] ?? 0);
    if (count($w) > 1) {
      // Several players to play (Mana selection): the one whose cards are named
      $owners = array_column($full['cards'], 'owner', 'id');
      foreach (array_merge(...array_map(fn($x) => (array) $x, $args)) as $id) {
        if (is_numeric($id) && isset($owners[(int) $id]) && in_array($owners[(int) $id], $w)) {
          $p = (int) $owners[(int) $id];
          break;
        }
      }
    }
    $raw = json_decode(json_encode(Arena::view($g, $p), JSON_PARTIAL_OUTPUT_ON_ERROR), true);
    $d = V1::view($g, $p)['decision'];
    $node = ['move' => $move, 'player' => $p, 'waiting' => $w, 'decision' => slim($d), 'state' => compactState($full)];
    if ($nextIds) {
      // Not the engine's doing: the other engine sets them too before answering
      $node['nextIds'] = $nextIds;
      $nextIds = [];
    }
    if ($d === null) {
      $node['end'] = $end = "no decision for move $move";
      break;
    }
    $a = replayAnswer($method, $args, $d);
    // The answer must send what the runner sent
    $sent = (new \ReflectionMethod(V1::class, 'toAction'))->invoke(null, $raw['state'], $a);
    $want = in_array($method, REPLAY_CALLS) ? [$method, $args] : ['actTakeAtomicAction', [$method, $args]];
    if (\ReplayRunner::normalize($sent) != \ReplayRunner::normalize($want)) {
      $mismatches[] = "move $move: answer " . json_encode($a) . ' sends ' . json_encode($sent) . ', the request was ' . json_encode($want);
    }
    // Same notifications as on BGA (an undo, or requests erased by one, left none to compare)
    $compare = !$erased && $method != 'actRestart' && $moves[$move] == 1;
    $ambiguous = $compare && !isset($corrections[$move]) ? ambiguousDraws($replay, $move) : [];
    if ($ambiguous) {
      \BgaDb::saveSnapshot("m$move");
    }
    $before = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM arena_notif')->fetch_row()[0];
    try {
      V1::answer($g, $p, $a + ['decisionId' => $d['id']]);
    } catch (\Throwable $e) {
      $node['next'] = [['answer' => $a, 'error' => substr($e->getMessage(), 0, 200)]];
      $end = "move $move refused: " . substr($e->getMessage(), 0, 200);
      break;
    }
    $diff = $compare ? \ReplayRunner::diff($replay->moves[$move] ?? [], notificationsSince($before)) : null;
    if ($diff !== null && $ambiguous) {
      // Try the other orders of the cards leaving the decks
      $variants = [[]];
      foreach ($ambiguous as $pId => $lists) {
        $variants = array_merge(...array_map(fn($v) => array_map(fn($order) => $v + [$pId => $order], interleavings($lists)), $variants));
      }
      foreach (array_slice($variants, 0, 24) as $variant) {
        \BgaDb::restoreSnapshot("m$move");
        Arena::openGame($g);
        foreach ($variant as $pId => $order) {
          // On top of the deck (the highest state), first to leave last
          foreach (array_merge(array_values(array_diff(deckIds($pId), $order)), array_reverse($order)) as $state => $id) {
            \BgaDb::conn()->query("UPDATE cards SET card_state = $state WHERE card_id = $id");
          }
        }
        try {
          V1::answer($g, $p, $a + ['decisionId' => $d['id']]);
        } catch (\Throwable $e) {
          continue;
        }
        if (\ReplayRunner::diff($replay->moves[$move] ?? [], notificationsSince($before)) === null) {
          dropGame($g);
          return ['correction' => [$move, $variant]];
        }
      }
      \BgaDb::restoreSnapshot("m$move");
      Arena::openGame($g);
      V1::answer($g, $p, $a + ['decisionId' => $d['id']]);
    }
    if ($diff !== null) {
      $mismatches[] = "move $move ($label): $diff";
    }
    if ($erased || in_array($method, ['actRestart', 'actUndoToStep'])) {
      $afterUndo = ['cards' => true, 'meeples' => true];
    }
    $node['next'] = [['answer' => $a, 'node' => []]];
    $node = &$node['next'][0]['node'];
    $steps++;
  }
  if ($end === null) {
    $state = compactState(Arena::oracleState($g));
    $w = V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'] ?? [];
    if (empty($w)) {
      $node = ['state' => $state, 'end' => $end = 'nobody to play'];
    } else {
      $d = V1::view($g, (int) $w[0])['decision'];
      $node = ['player' => (int) $w[0], 'waiting' => $w, 'decision' => slim($d), 'state' => $state, 'end' => $end = 'end of the replay'];
    }
  }
  unset($node);
  \Harness\Rng::$shuffle = null;
  $final = Arena::oracleState($g);
  return [
    'phpGame' => $g,
    'steps' => $steps,
    'end' => $end,
    'day' => $final['day'],
    'state' => $final['state'],
    'mismatches' => $mismatches,
    'injectShuffles' => $injected,
    'dice' => $dice ?: [1],
    'initial' => $initial,
    'db' => $db,
    'tree' => $tree,
  ];
}

/**
 * Replays a game recorded on BGA in the oracle: same decks, deck orders, Tumult, preferences and
 * dice, then each request as a protocol v1 answer; writes OUT/replays/TABLE.json (tree format of
 * the whole games: a node per request, with one `next`)
 */
function playReplay(string $bgaFile, string $out, bool $keep): void
{
  $t = microtime(true);
  $harness = getenv('REPLAY_HARNESS') ?: '/harness/tests/Replay';
  require_once "$harness/Replay.php";
  require_once "$harness/ReplayRunner.php";
  $table = (int) basename($bgaFile, '.json');
  $file = sys_get_temp_dir() . "/replay-$table.json";
  file_put_contents($file, json_encode(['data' => anonymizeReplay(json_decode(file_get_contents($bgaFile), true)['data'])]));

  // 1. The requests (the replay holds their notifications, not the requests themselves)
  [$ok, $played, $divergence, $requests] = recoverRequests($file, $table);
  if (!$ok) {
    unlink($file);
    printf("replay %d: the oracle diverges from BGA at move %d (%d/%d requests replayed): %s\n", $table, $divergence['move'] ?? -1,
      count(array_unique(array_column($played, 'move'))), $requests, json_encode($divergence['closest'] ?? null));
    exit(1);
  }
  $replay = new \Replay($file);
  unlink($file);

  // 2. The same game in the arena, until the order of every ambiguous draw is settled
  $corrections = [];
  do {
    $run = arenaReplay($replay, $played, $corrections);
    if (isset($run['correction'])) {
      [$move, $variant] = $run['correction'];
      $corrections[$move] = $variant;
    }
  } while (isset($run['correction']));

  @mkdir("$out/replays", 0777, true);
  file_put_contents("$out/replays/$table.json", json_encode(['mode' => 'replay', 'table' => $table, 'requests' => $requests, 'drawOrders' => $corrections] + $run,
    JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE, 100000));
  printf("replay %d: %d requests, %d steps, %s, day %d, %s, %d shuffles injected, %d draw orders settled, %d mismatches with BGA, %.1fs (php game %d)\n", $table,
    $requests, $run['steps'], $run['end'], $run['day'], $run['state'], count($run['injectShuffles']), count($corrections), count($run['mismatches']),
    microtime(true) - $t, $run['phpGame']);
  foreach (array_slice($run['mismatches'], 0, 10) as $line) {
    echo "  $line\n";
  }
  if (!$keep) {
    dropGame($run['phpGame']);
  }
}

if (($argv[1] ?? '') === 'replay') {
  $file = $argv[2] ?? exit("usage: explore.php replay BGA_REPLAY.json [OUT_DIR] [--keep]\n");
  $rest = array_values(array_filter(array_slice($argv, 3), fn($a) => !str_starts_with($a, '--')));
  playReplay($file, $rest[0] ?? dirname(__DIR__) . '/goldens', in_array('--keep', $argv, true));
  exit;
}

if (($argv[1] ?? '') === 'game') {
  $seed = (int) ($argv[2] ?? exit("usage: explore.php game SEED [OUT_DIR] [--keep] [--dump=STEP,...]\n"));
  $rest = array_values(array_filter(array_slice($argv, 3), fn($a) => !str_starts_with($a, '--')));
  $dump = [];
  foreach ($argv as $a) {
    if (str_starts_with($a, '--dump=')) {
      $dump = array_map('intval', explode(',', substr($a, 7)));
    }
  }
  playGame($seed, $rest[0] ?? dirname(__DIR__) . '/goldens', in_array('--keep', $argv, true), $dump);
  exit;
}

$ref = $argv[1] ?? exit("usage: explore.php CARD_REF [BOARD] [OUT_DIR]\n       explore.php game SEED [OUT_DIR] [--keep]\n");
$board = $argv[2] ?? 'rich';
$out = $argv[3] ?? dirname(__DIR__) . '/goldens';

$t = microtime(true);
$g = setupGame();
Arena::openGame($g);
[$me, $cardId] = prepare($g, $ref, $board);
// The position, in the snapshot format both engines load (Game::from_snapshot)
$initial = Arena::oracleState($g);
// ...and the original engine's own tables, for an engine that loads them as they are
$db = dumpTables();
$tree = explore($g, 0, $cardId);
@mkdir("$out/$board", 0777, true);
file_put_contents("$out/$board/$ref.json", json_encode([
  'card' => $ref,
  'board' => $board,
  'phpGame' => $g,
  'cardId' => $cardId,
  'nodes' => $nodes,
  'dice' => DICE,
  'initial' => $initial,
  'db' => $db,
  'tree' => $tree,
], JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE));
printf("%s %s: %d nodes, %.1fs (php game %d)\n", $ref, $board, $nodes, microtime(true) - $t, $g);

// The game and its snapshots are not needed any more
dropGame($g);
