<?php

/**
 * Reference runs for rules/tests/storm.rs: flows of the expedition actions (MoveExpedition,
 * BlockExpedition, TargetExpedition, TargetPlayer, RollDie, MarkRegion, MoveRegionMarker) set up
 * on a position, then answered (first answer each time), recording the decisions and the tables.
 *
 *   docker compose exec -T server php < rules/tests/data/storm_actions_php.php > rules/tests/data/storm_actions_php.json
 */

require '/server/src/bootstrap.php';

use Arena\Arena;
use Arena\V1;
use ALT\Core\Engine;
use ALT\Core\Globals;
use ALT\Managers\Cards;

const DICE = [3, 1, 6, 4, 2, 5];
const DECKS = ['ax_1_sierra', 'br_1_kojo'];

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
  \BgaDb::conn()->query("UPDATE cards SET card_state = $state WHERE card_id = $id");
  Cards::invalidate();
  return $id;
}

function meeple(string $location, int $pId, string $type, int $state = 0): void
{
  \BgaDb::conn()->query("INSERT INTO meeples (meeple_location, meeple_state, type, player_id) VALUES ('$location', $state, '$type', $pId)");
}

function place(int $pId, string $type, int $region): void
{
  \BgaDb::conn()->query("UPDATE meeples SET meeple_location = 'storm-$region' WHERE player_id = $pId AND type = '$type'");
}

function rows(string $sql): array
{
  return \BgaDb::conn()->query($sql)->fetch_all(MYSQLI_ASSOC);
}

function dbDump(): array
{
  $db = [];
  foreach (['cards', 'meeples', 'global_variables', 'player', 'user_preferences'] as $table) {
    $db[$table] = rows("SELECT * FROM `$table` ORDER BY 1");
  }
  $db['global'] = rows("SELECT * FROM `global` ORDER BY 1");
  $db['arena_rng'] = rows("SELECT * FROM `arena_rng`");
  $db['auto_increment'] = [];
  foreach (['cards', 'meeples'] as $table) {
    $db['auto_increment'][$table] = (string) \BgaDb::conn()->query("SELECT AUTO_INCREMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = '$table'")->fetch_row()[0];
  }
  return $db;
}

function tables(): array
{
  $cards = [];
  foreach (rows('SELECT card_id, card_location, card_state, player_id, properties FROM cards ORDER BY 1') as $r) {
    $p = json_decode($r['properties'], true);
    $cards[] = [(int) $r['card_id'], $r['card_location'], (int) $r['card_state'], (int) $r['player_id'], $p['tapped'] ?? null, $p['extraDatas'] ?? null];
  }
  $meeples = [];
  foreach (rows('SELECT * FROM meeples ORDER BY 1') as $r) {
    $meeples[] = [(int) $r['meeple_id'], $r['meeple_location'], (string) $r['meeple_state'], $r['type'], (string) $r['player_id']];
  }
  $globals = [];
  foreach (rows("SELECT * FROM global_variables WHERE name IN ('blockedExpeditions', 'expeditionMoves', 'stormMoves', 'diceRolls')") as $r) {
    $globals[$r['name']] = json_decode($r['value'], true);
  }
  return ['cards' => $cards, 'meeples' => $meeples, 'globals' => $globals];
}

function slim(?array $d): ?array
{
  if ($d === null) {
    return null;
  }
  unset($d['id']);
  return $d;
}

/** The answer taken at a decision: the first one */
function firstAnswer(array $d): array
{
  switch ($d['kind']) {
    case 'expedition':
      return ['type' => 'expedition', 'keys' => array_slice(array_column($d['expeditions'], 'key'), 0, max(1, $d['min']))];
    case 'player':
      return ['type' => 'player', 'player' => $d['players'][0]];
    case 'number':
      return ['type' => 'number', 'value' => end($d['values'])];
    case 'buttons':
      return empty($d['buttons']) ? ['type' => 'pass'] : ['type' => 'button', 'key' => $d['buttons'][0]['key']];
    case 'choice':
      foreach ($d['choices'] as $c) {
        if ($c['enabled']) {
          return ['type' => 'choice', 'id' => $c['id']];
        }
      }
      return ['type' => 'pass'];
    case 'select':
      return ['type' => 'select', 'cardIds' => array_slice($d['cardIds'], 0, $d['min'])];
    case 'confirm':
      return ['type' => 'confirm'];
  }
  return ['type' => 'pass'];
}

$g = setupGame();
Arena::openGame($g);
$me = (int) V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'][0];
$opp = $me == Arena::PLAYER_IDS[0] ? Arena::PLAYER_IDS[1] : Arena::PLAYER_IDS[0];
$ids = Arena::inRequest($g, null, function () use ($me, $opp) {
  $ids = [
    'meLeft' => create($me, 'BR_Common_DevotedProtector', STORM_LEFT, 10),
    'meRight' => create($me, 'AX_Common_IraFairAttendee', STORM_RIGHT, 11),
    'oppLeft' => create($opp, 'AX_Common_IraFairAttendee', STORM_LEFT, 12),
    'oppRight' => create($opp, 'BR_Common_Kappa', STORM_RIGHT, 13),
  ];
  Globals::getDay();
  $storm = Globals::getStorm();
  $storm[1]['visible'] = true;
  $storm[2]['visible'] = true;
  Globals::setStorm($storm);
  place($me, HERO, 2);
  place($me, COMPANION, 5);
  place($opp, HERO, 1);
  place($opp, COMPANION, 6);
  meeple('card-' . $ids['meLeft'], $me, OCEAN);
  meeple('card-' . $ids['meLeft'], $me, MOUNTAIN);
  meeple('storm-3', $me, FOREST, 2);
  $card = Cards::get($ids['meLeft']);
  $card->setExtraDatas(['counter' => 2]);
  return $ids;
});
Arena::openGame($g);
$db = dbDump();
\BgaDb::saveSnapshot('actions');

$move = fn($args) => ['action' => MOVE_EXPEDITION, 'args' => $args];
$cases = [
  'blockAll' => ['action' => BLOCK_EXPEDITION, 'args' => ['expedition' => 'all']],
  'blockOne' => ['action' => BLOCK_EXPEDITION, 'args' => []],
  'moveForced' => $move(['forceExpedition' => [$opp, STORM_LEFT], 'n' => 1]),
  'moveChoice' => $move(['n' => 1]),
  'moveBackward' => $move(['n' => -1, 'pId' => ME]),
  'moveEffect' => $move(['expedition' => EFFECT, 'pId' => OPPONENT, 'n' => 2]),
  'moveOther' => $move(['expedition' => STORM_LEFT, 'pId' => ME, 'n' => 1, 'moveOtherExpedition' => true]),
  'moveOtherBack' => $move(['expedition' => STORM_RIGHT, 'pId' => ME, 'n' => -1, 'moveOtherExpedition' => true]),
  'targetExpedition' => ['action' => TARGET_EXPEDITION, 'args' => ['n' => 1, 'effect' => $move(['n' => 1])]],
  'targetExpeditionAll' => ['action' => TARGET_EXPEDITION, 'args' => ['expedition' => 'all', 'players' => OPPONENT, 'effect' => $move(['n' => -1])]],
  'targetExpeditionOther' => ['action' => TARGET_EXPEDITION, 'args' => ['otherThanMe' => true, 'players' => ME, 'effect' => ['type' => NODE_SEQ, 'childs' => [$move(['n' => 1]), ['action' => BLOCK_EXPEDITION, 'args' => ['expedition' => 'all']]]]]],
  'targetPlayer' => ['action' => TARGET_PLAYER, 'args' => ['effect' => $move(['n' => 1])]],
  'targetPlayerChoice' => ['action' => TARGET_PLAYER, 'args' => ['opponentsOnly' => false, 'effect' => $move(['n' => 1])]],
  'rollDie' => ['action' => ROLL_DIE, 'args' => ['effect' => ['1-3' => $move(['n' => 1, 'pId' => ME, 'expedition' => STORM_LEFT]), '4+' => ['action' => BLOCK_EXPEDITION, 'args' => ['expedition' => 'all']]]]],
  'rollDieCounters' => ['action' => ROLL_DIE, 'args' => ['allowCounterIncrease' => true, 'n' => 2, 'effect' => ['1-4' => $move(['n' => 1, 'pId' => ME, 'expedition' => STORM_RIGHT]), '5' => 'OPPOSITE', '6+' => ['action' => BLOCK_EXPEDITION, 'args' => ['expedition' => 'all']]]]],
  'markRegion' => ['action' => MARK_REGION, 'args' => []],
  'markRegionCreate' => ['action' => MARK_REGION, 'args' => ['create' => true, 'regionType' => MOUNTAIN], 'sourceId' => 'meRight'],
  'moveRegionMarker' => ['action' => MOVE_REGION_MARKER, 'args' => ['markerType' => FOREST]],
];

$out = ['me' => $me, 'db' => $db, 'cases' => []];
foreach ($cases as $name => $flow) {
  \BgaDb::restoreSnapshot('actions');
  Arena::openGame($g);
  $flow['sourceId'] = $ids[$flow['sourceId'] ?? 'meLeft'];
  $flow['pId'] = $me;
  $error = null;
  try {
    Arena::inRequest($g, null, function () use ($flow) {
      Globals::getDay();
      Engine::setup(['type' => NODE_SEQ, 'childs' => [$flow]], ['order' => 'assignment']);
      Engine::proceed();
    });
  } catch (\Throwable $e) {
    $error = substr($e->getMessage(), 0, 200);
  }
  $steps = [['tables' => tables(), 'error' => $error]];
  for ($i = 0; $i < 8 && $error === null; $i++) {
    $w = V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'] ?? [];
    if (empty($w)) {
      break;
    }
    $p = (int) $w[0];
    $d = V1::view($g, $p)['decision'];
    $step = ['player' => $p, 'decision' => slim($d)];
    if ($d === null || $d['kind'] == 'turn') {
      $steps[] = $step;
      break;
    }
    $a = firstAnswer($d);
    $step['answer'] = $a;
    try {
      V1::answer($g, $p, $a + ['decisionId' => $d['id']]);
    } catch (\Throwable $e) {
      $error = substr($e->getMessage(), 0, 200);
    }
    $step['error'] = $error;
    $step['tables'] = tables();
    $steps[] = $step;
  }
  $out['cases'][$name] = ['flow' => $flow, 'steps' => $steps];
}
echo json_encode($out, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
