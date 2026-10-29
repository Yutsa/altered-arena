<?php

/**
 * Reference values for rules/tests/storm.rs, from the original engine: positions with storm
 * powers (defenders, region changes, gigantic, ...), the storm computations on them, then the
 * Dusk moves (Players::computeStorm(true)) and the Night cleanup (Player::nightCleanup).
 *
 *   docker compose exec -T server php < rules/tests/data/storm_php.php > rules/tests/data/storm_php.json
 */

require '/server/src/bootstrap.php';

use Arena\Arena;
use Arena\V1;
use ALT\Core\Engine;
use ALT\Core\Globals;
use ALT\Managers\Cards;
use ALT\Managers\Meeples;
use ALT\Managers\Players;

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

function token(int $cardId, int $pId, string $type): void
{
  \BgaDb::conn()->query("INSERT INTO meeples (meeple_location, meeple_state, type, player_id) VALUES ('card-$cardId', 0, '$type', $pId)");
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
    $cards[] = [(int) $r['card_id'], $r['card_location'], (int) $r['card_state'], (int) $r['player_id'], $p['tapped'] ?? null];
  }
  $meeples = [];
  foreach (rows('SELECT * FROM meeples ORDER BY 1') as $r) {
    $meeples[] = [(int) $r['meeple_id'], $r['meeple_location'], (string) $r['meeple_state'], $r['type'], (string) $r['player_id']];
  }
  return ['cards' => $cards, 'meeples' => $meeples];
}

$scenarios = [
  // Plain expeditions in revealed regions, boosts, statuses
  'plain' => function (int $me, int $opp, array $ids) {
    place($me, HERO, 2);
    place($me, COMPANION, 5);
    place($opp, HERO, 1);
    place($opp, COMPANION, 6);
    token($ids['meLeft'], $me, BOOST);
    token($ids['meLeft'], $me, BOOST);
    token($ids['meRight'], $me, ANCHORED);
    token($ids['oppLeft'], $opp, FLEETING);
    token($ids['oppRight'], $opp, ASLEEP);
  },
  // Defenders, region modifiers, gigantic, terrain marker, eternal, Jinn, Don Juan, Sleigh
  'powers' => function (int $me, int $opp, array $ids) {
    place($me, HERO, 1);
    place($me, COMPANION, 6);
    place($opp, HERO, 1);
    place($opp, COMPANION, 5);
    $bird = create($opp, 'MU_Common_ArmoredBird', STORM_LEFT, 20);
    create($me, 'LY_Common_WillotheWisp', STORM_RIGHT, 21);
    create($me, 'MU_Common_MagicBeans', STORM_LEFT, 22);
    create($opp, 'LY_Common_WinterOutfits', STORM_RIGHT, 23);
    $hyd = create($me, 'MU_Common_Hydracaena', STORM_RIGHT, 24);
    token($hyd, $me, FLEETING);
    create($opp, 'AX_Common_LucanHungryLeviathan', STORM_RIGHT, 25);
    create($me, 'BR_Common_MightyJinn', STORM_LEFT, 26);
    create($opp, 'LY_Common_DonJuan', STORM_LEFT, 27);
    create($me, 'LY_Common_TheMagicSleigh', STORM_RIGHT, 28);
    token($bird, $opp, BOOST);
    \BgaDb::conn()->query("INSERT INTO meeples (meeple_location, meeple_state, type, player_id) VALUES ('storm-6', 3, 'ocean', $me)");
    \BgaDb::conn()->query("INSERT INTO meeples (meeple_location, meeple_state, type, player_id) VALUES ('storm-6', 2, 'forest', $me)");
  },
  // Opposite defender, ignore defender, global blocks, Eris / Pegasus replacing the move
  'blocks' => function (int $me, int $opp, array $ids) {
    place($me, HERO, 3);
    place($me, COMPANION, 4);
    place($opp, HERO, 3);
    place($opp, COMPANION, 4);
    create($opp, 'OD_Common_Issitoq', STORM_LEFT, 20);
    create($me, 'OD_Rare_MobileArmory', STORM_LEFT, 21);
    create($opp, 'MU_Common_ArmoredBird', STORM_RIGHT, 22);
    create($me, 'LY_Common_Eris', STORM_RIGHT, 23);
    create($me, 'OD_Common_Pegasus', STORM_RIGHT, 24);
    create($opp, 'OD_Rare_Galatea', STORM_LEFT, 25);
    Globals::setBlockedExpeditions([$opp => [STORM_RIGHT]]);
  },
];

$out = [];
foreach ($scenarios as $name => $fn) {
  $g = setupGame();
  Arena::openGame($g);
  $me = (int) V1::view($g, Arena::PLAYER_IDS[0])['waitingFor'][0];
  $opp = $me == Arena::PLAYER_IDS[0] ? Arena::PLAYER_IDS[1] : Arena::PLAYER_IDS[0];
  Arena::inRequest($g, null, function () use ($me, $opp, $fn) {
    $ids = [
      'meLeft' => create($me, 'BR_Common_DevotedProtector', STORM_LEFT, 10),
      'meRight' => create($me, 'AX_Common_IraFairAttendee', STORM_RIGHT, 11),
      'oppLeft' => create($opp, 'AX_Common_IraFairAttendee', STORM_LEFT, 12),
      'oppRight' => create($opp, 'BR_Common_Kappa', STORM_RIGHT, 13),
    ];
    create($me, 'OD_Rare_PicnicArea', LANDMARK, 14);
    Globals::getDay(); // getStorm() reads the data without fetching it
    $storm = Globals::getStorm();
    foreach ($storm as &$s) {
      $s['visible'] = true;
    }
    Globals::setStorm($storm);
    $fn($me, $opp, $ids);
  });
  Arena::openGame($g);
  $db = dbDump();

  $values = Arena::inRequest($g, null, function () {
    $isDefender = [];
    $biomes = [];
    $strength = [];
    foreach (Cards::getPlayedCards(null) as $cId => $card) {
      $isDefender[$cId] = $card->isDefender();
    }
    foreach (Players::getAll() as $pId => $player) {
      $biomes[$pId] = [STORM_LEFT => $player->getBiomes(STORM_LEFT), STORM_RIGHT => $player->getBiomes(STORM_RIGHT)];
      $strength[$pId] = $player->getBiomeStrength(STORMS, false);
    }
    return [
      'biomeTotals' => Players::getBiomeTotals(),
      'strengthNoModifiers' => $strength,
      'movements' => Players::computeStorm(false),
      'blocked' => Players::getBlockedExpeditions(),
      'defenders' => Players::getDefenders(false),
      'regions' => Players::getRegionsInfo(),
      'winning' => Players::getWinningPlayerByStorms(),
      'actionInstead' => Players::getActionInsteadOfAdvance(),
      'isDefender' => $isDefender,
      'biomes' => $biomes,
    ];
  });

  $dusk = Arena::inRequest($g, null, function () {
    Engine::setup(['type' => NODE_SEQ, 'childs' => []], ['order' => 'advancePhase']);
    $movements = Players::computeStorm(true);
    return ['movements' => $movements, 'stormMoves' => Globals::getStormMoves(), 'storm' => Globals::getStorm(), 'engine' => Globals::getEngine()];
  });
  $dusk += tables();

  $night = Arena::inRequest($g, null, function () {
    Engine::setup(['type' => NODE_SEQ, 'childs' => []], ['order' => 'nightCleanup']);
    $returns = [];
    foreach (Players::getTurnOrder() as $pId) {
      $returns[$pId] = Players::get($pId)->nightCleanup();
    }
    Meeples::nightCleanup();
    return ['returns' => $returns, 'engine' => Globals::getEngine()];
  });
  $night += tables();

  $out[$name] = ['me' => $me, 'db' => $db, 'values' => $values, 'dusk' => $dusk, 'night' => $night];
}
echo json_encode($out, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
