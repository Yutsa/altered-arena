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

$ref = $argv[1] ?? exit("usage: explore.php CARD_REF [BOARD] [OUT_DIR]\n");
$board = $argv[2] ?? 'rich';
$out = $argv[3] ?? dirname(__DIR__) . '/goldens';

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

$t = microtime(true);
$g = setupGame();
Arena::openGame($g);
[$me, $cardId] = prepare($g, $ref, $board);
// The position, in the snapshot format both engines load (Game::from_snapshot)
$initial = Arena::oracleState($g);
// ...and the original engine's own tables, for an engine that loads them as they are
$db = [];
foreach (['cards', 'meeples', 'global_variables', 'player', 'user_preferences'] as $table) {
  $db[$table] = \BgaDb::conn()->query("SELECT * FROM `$table` ORDER BY 1")->fetch_all(MYSQLI_ASSOC);
}
// ...and what the framework keeps besides: the `global` table (1 = state, 2 = active player,
// 10 = logging), the next ids of cards and meeples, the position in the dice list
$db['global'] = \BgaDb::conn()->query("SELECT * FROM `global` ORDER BY 1")->fetch_all(MYSQLI_ASSOC);
$db['arena_rng'] = \BgaDb::conn()->query("SELECT * FROM `arena_rng`")->fetch_all(MYSQLI_ASSOC);
$db['auto_increment'] = [];
foreach (['cards', 'meeples'] as $table) {
  $db['auto_increment'][$table] = (string) \BgaDb::conn()->query("SELECT AUTO_INCREMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '$table'")->fetch_row()[0];
}
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
$db = \BgaDb::conn();
foreach ($db->query("SHOW DATABASES LIKE 'arena_g{$g}%'")->fetch_all() as [$name]) {
  if ($name === "arena_g$g" || str_starts_with($name, "arena_g{$g}_")) {
    $db->query("DROP DATABASE `$name`");
  }
}
