<?php
/**
 * API decks (Altered Reunion decks) and Unique cards: data and golden files for the translation of
 * actGetDeckInfos, Cards::getCardClass, Cards::generateUnique and Helpers/FlowConvertor.php.
 *
 *   php tools/api_decks.php export DIR         writes to DIR (copy them to rules/data):
 *     flow.min.json  the FlowConvertor tables (triggers, conditions, outputs), the id lists
 *                    TRIGGER / CONDITION / OUTPUT and IN_PLAY (array constants are not in
 *                    constants.json)
 *     refs.min.json  MAP_REFS_CLASSES (card reference => class), MANUAL_ALT_ART, Cards::getAltArt
 *                    and the serial illustrations (img/illustrations/FACTION/UID.jpg whose UID ends
 *                    with _NNN or _XXX: Cards::illustrationExists is only asked about those)
 *   php tools/api_decks.php uniques [OUT_DIR]  writes OUT_DIR/uniques/NNN.json (default
 *     server/goldens): Cards::generateUnique on API contents, one per trigram of CEG
 *     (Cards/uniques.list.inc.php: every trigram of the real Uniques), then U_MULTI cards of 2 or 3
 *     trigrams drawn from it (mt_srand(1)), and U_SAME cards per output id and per trigger id whose
 *     trigrams share it, so that effects merge.
 *     A case: {"content": API content, "out": properties (null: generateUnique returned null)}
 *     or {"content": ..., "error": message}.
 *   php tools/api_decks.php cardclass [OUT_DIR] writes OUT_DIR/cardclass/NNN.json: Cards::getCardClass
 *     on every reference of MAP_REFS_CLASSES and on variants of them (KS, alternate arts, serial
 *     prints, unknown references). A case: {"ref", "class", "props", "token", "asset", "mainAsset",
 *     "fullArt"} or {"ref", "error"}.
 *   php tools/api_decks.php decks [OUT_DIR]    writes OUT_DIR/apidecks/NNN.json: games created with
 *     V1::createGame where player A brings a Reunion deck (`reunionDeck`: what the publisher API
 *     returns to get_player_deck_content, then actGetDeckInfos and actConfirmAPIDeck) and player B a
 *     starter deck; the tables after creation, or the error.
 *   php tools/api_decks.php decks OUT_DIR RESPONSES  the same with the responses of RESPONSES (a JSON
 *     list: real decks of decks-api converted by `cargo run -p goldens --example reunion_responses`),
 *     written to OUT_DIR/apidecks_real/NNN.json
 */
require dirname(__DIR__) . '/src/bootstrap.php';
require_once GAME_DIR . '/modules/php/Cards/uniques.list.inc.php';
require_once GAME_DIR . '/modules/php/Cards/cards.inc.php';

use Arena\Arena;
use Arena\V1;
use ALT\Helpers\FlowConvertor;
use ALT\Managers\Cards;

const U_MULTI = 40000;
const U_SAME = 20;
const U_PER_FILE = 2000;
const U_FLAGS = JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES | JSON_PRESERVE_ZERO_FRACTION;
const U_FACTIONS = ['AX', 'BR', 'LY', 'MU', 'OR', 'YZ'];
const U_SETS = ['CORE', 'COREKS', 'ALIZE', 'BISE', 'CYCLONE', 'DUSTER', 'EOLE', 'FUGUE'];
const U_SUBTYPES = [[], ['ADVENTURER'], ['ENGINEER', 'ROBOT'], ['ANIMAL'], ['SPIRIT', 'ANIMAL'], ['MAGE']];
const U_DECK_CARDS = 26;
const U_DICE = [3, 1, 6, 4, 2, 5];

/** The serial illustrations: FACTION/UID of img/illustrations/FACTION/UID.jpg ending with _NNN or _XXX */
function serialIllustrations(): array
{
  $out = [];
  foreach (glob(GAME_DIR . '/img/illustrations/*/*.jpg') as $f) {
    $uid = basename($f, '.jpg');
    if (preg_match('/_(?:XXX|\d{3})$/', $uid)) {
      $out[] = basename(dirname($f)) . '/' . $uid;
    }
  }
  sort($out);
  return $out;
}

/** An API content (decks-api normalizeBga) whose uniqueReduced holds the given trigrams */
function content(int $n, array $trigrams): array
{
  $faction = U_FACTIONS[$n % 6];
  $set = U_SETS[intdiv($n, 6) % count(U_SETS)];
  $number = 1 + ($n * 7) % 30;
  $ref = sprintf('ALT_%s_B_%s_%02d_U_%d', $set, $faction, $number, 1000 + $n);
  $elements = [];
  foreach ($trigrams as $t) {
    $elements[] = ['effects' => array_map('intval', explode('_', $t))];
  }
  return [
    'reference' => $ref,
    'name' => "Unique $n",
    'cardType' => ['reference' => 'CHARACTER'],
    'subTypes' => U_SUBTYPES[$n % count(U_SUBTYPES)],
    'rarity' => ['reference' => 'UNIQUE'],
    'typeline' => '(Character)',
    'mainFaction' => ['reference' => $faction],
    'illustrator' => ['nickName' => 'Someone'],
    'elements' => [
      'MAIN_COST' => $n % 6,
      'RECALL_COST' => ($n * 5) % 7,
      'FOREST_POWER' => $n % 4,
      'MOUNTAIN_POWER' => ($n >> 2) % 4,
      'OCEAN_POWER' => ($n >> 4) % 4,
    ],
    'uniqueReduced' => $elements,
  ];
}

/** Writes the cases in files of U_PER_FILE */
function writeCases(string $dir, string $mode, array $cases): void
{
  @mkdir($dir, 0777, true);
  foreach (glob("$dir/*.json") as $old) {
    unlink($old);
  }
  foreach (array_chunk($cases, U_PER_FILE) as $f => $chunk) {
    $json = json_encode(['mode' => $mode, 'cases' => $chunk], U_FLAGS);
    if ($json === false) {
      fwrite(STDERR, "file $f: " . json_last_error_msg() . "\n");
      exit(1);
    }
    file_put_contents(sprintf('%s/%03d.json', $dir, $f), $json);
  }
}

/** Drops the game's database */
function dropGame(int $g): void
{
  \BgaDb::conn()->query("DROP DATABASE IF EXISTS `arena_g$g`");
}

/** The tables of the open game (explore.php dumpTables) */
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

/** The references getCardClass is checked on: MAP_REFS_CLASSES, then variants of them */
function cardClassRefs(): array
{
  $refs = array_keys(MAP_REFS_CLASSES);
  $refs = array_merge($refs, array_keys(MANUAL_ALT_ART));
  foreach (array_keys(Cards::getAltArt()) as $alt) {
    foreach (['_C', '_R1', '_R2'] as $suffix) {
      $refs[] = $alt . $suffix;
    }
  }
  foreach (array_keys(MAP_REFS_CLASSES) as $i => $ref) {
    $p = explode('_', $ref);
    if (count($p) < 6) {
      continue;
    }
    foreach ([[1, 'COREKS'], [2, 'A'], [2, 'P'], [1, 'DUSTERCB'], [1, 'MUSUBI'], [1, 'EOLETOP']] as [$at, $v]) {
      $q = $p;
      $q[$at] = $v;
      $refs[] = implode('_', $q);
    }
    $serial = ['_001', '_014', '_XXX'][$i % 3];
    $refs[] = $ref . $serial;
    $q = $p;
    $q[1] = 'DUSTERCB';
    $q[2] = 'P';
    $refs[] = implode('_', $q) . $serial;
  }
  foreach (serialIllustrations() as $f) {
    $refs[] = explode('/', $f)[1];
  }
  $refs[] = 'ALT_NOPE_B_AX_99_C';
  $refs[] = 'NOPE';
  return array_values(array_unique($refs));
}

/** getCardClass($ref) as a golden case */
function cardClassCase(string $ref): array
{
  try {
    $c = Cards::getCardClass($ref);
    return [
      'ref' => $ref,
      'class' => (new \ReflectionClass($c))->getShortName(),
      'props' => $c->getProperties(),
      'token' => $c->isToken(),
      'asset' => $c->getAsset(),
      'mainAsset' => $c->getMainAsset(),
      'fullArt' => $c->getFullArt(),
    ];
  } catch (\Throwable $e) {
    return ['ref' => $ref, 'error' => get_class($e) . ': ' . $e->getMessage()];
  }
}

/** A scratch game, for code that writes to the database (Card setters) */
function withScratchGame(callable $fn)
{
  $g = Arena::createGame(['players' => [['name' => 'A'], ['name' => 'B']]])['id'];
  try {
    return Arena::inRequest($g, null, $fn);
  } finally {
    dropGame($g);
  }
}

$mode = $argv[1] ?? '';
$outRoot = $argv[2] ?? dirname(__DIR__) . '/goldens';

if ($mode === 'export') {
  $out = $argv[2] ?? '/engine-data';
  $flow = [
    'triggers' => FlowConvertor::getTriggers(),
    'conditions' => FlowConvertor::getConditions(),
    'outputs' => FlowConvertor::getOutput(),
    'TRIGGER' => TRIGGER,
    'CONDITION' => CONDITION,
    'OUTPUT' => OUTPUT,
    'IN_PLAY' => IN_PLAY,
  ];
  file_put_contents("$out/flow.min.json", json_encode($flow, U_FLAGS));
  $refs = [
    'refs' => MAP_REFS_CLASSES,
    'manualAltArt' => MANUAL_ALT_ART,
    'altArt' => Cards::getAltArt(),
    'serialIllustrations' => serialIllustrations(),
  ];
  file_put_contents("$out/refs.min.json", json_encode($refs, U_FLAGS));
  echo count($flow['triggers']) . ' triggers, ' . count($flow['conditions']) . ' conditions, ' . count($flow['outputs']) . ' outputs, '
    . count($refs['refs']) . ' refs, ' . count($refs['serialIllustrations']) . " serial illustrations\n";
  exit;
}

if ($mode === 'uniques') {
  $contents = [];
  foreach (CEG as $i => $t) {
    $contents[] = content($i, [$t]);
  }
  mt_srand(1);
  $n = count(CEG);
  for ($i = 0; $i < U_MULTI; $i++) {
    $k = mt_rand(2, 3);
    $trigrams = [];
    for ($j = 0; $j < $k; $j++) {
      $trigrams[] = CEG[mt_rand(0, $n - 1)];
    }
    $contents[] = content($n + $i, $trigrams);
  }

  // Effects that share their output or their trigger merge into the same properties (same
  // attribute, same passive trigger, PARALLEL nodes): U_SAME cards of 2 or 3 trigrams per output id
  // and per trigger id, drawn among the trigrams that have it
  $byOutput = [];
  $byTrigger = [];
  foreach (CEG as $t) {
    foreach (explode('_', $t) as $id) {
      if (in_array((int) $id, TRIGGER)) {
        $byTrigger[$id][] = $t;
      } elseif (in_array((int) $id, OUTPUT)) {
        $byOutput[$id][] = $t;
      }
    }
  }
  ksort($byOutput);
  ksort($byTrigger);
  foreach ([$byOutput, $byTrigger] as $groups) {
    foreach ($groups as $list) {
      for ($i = 0; $i < U_SAME; $i++) {
        $k = mt_rand(2, 3);
        $trigrams = [];
        for ($j = 0; $j < $k; $j++) {
          $trigrams[] = $list[mt_rand(0, count($list) - 1)];
        }
        $contents[] = content(count($contents), $trigrams);
      }
    }
  }

  $errors = 0;
  $cases = [];
  foreach ($contents as $content) {
    $case = ['content' => $content];
    try {
      $case['out'] = Cards::generateUnique($content);
    } catch (\Throwable $e) {
      $case['error'] = get_class($e) . ': ' . $e->getMessage();
      $errors++;
    }
    $cases[] = $case;
  }
  writeCases("$outRoot/uniques", 'uniques', $cases);
  echo count($cases) . " cases, $errors errors\n";
  exit;
}

if ($mode === 'cardclass') {
  $cases = withScratchGame(fn() => array_map('cardClassCase', cardClassRefs()));
  writeCases("$outRoot/cardclass", 'cardclass', $cases);
  echo count($cases) . ' cases, ' . count(array_filter($cases, fn($c) => isset($c['error']))) . " errors\n";
  exit;
}

if ($mode === 'decks') {
  // The cards getCardClass resolves, by kind
  $cases = withScratchGame(fn() => array_map('cardClassCase', cardClassRefs()));
  $heroes = $cards = $tokens = [];
  foreach ($cases as $c) {
    if (isset($c['error'])) {
      continue;
    }
    if ($c['token']) {
      $tokens[] = $c['ref'];
    } elseif (($c['props']['type'] ?? '') == 'hero') {
      $heroes[] = $c['ref'];
    } else {
      $cards[] = $c['ref'];
    }
  }
  mt_srand(2);
  shuffle($cards);

  /** A response of get_player_deck_content (altered-bga-api) */
  $deck = function (int $i, string $hero, array $entries, bool $legal = true): array {
    $count = 0;
    foreach ($entries as $e) {
      $count += $e['quantity'];
    }
    return [
      'deckName' => "Deck $i",
      'id' => "request-$i",
      'faction' => explode('_', $hero)[3] ?? 'AX',
      'legality' => $legal,
      'hero' => $hero,
      'cardCount' => $count,
      'cards' => $entries,
      'apiId' => sprintf('00000000-0000-4000-8000-%012d', $i),
    ];
  };
  $responses = [];
  $n = count(CEG);
  foreach (array_chunk($cards, U_DECK_CARDS) as $i => $chunk) {
    $entries = [];
    foreach ($chunk as $j => $ref) {
      $entries[$ref] = ['quantity' => 1 + ($i + $j) % 3];
    }
    if ($i % 4 == 0 && $tokens) {
      $entries[$tokens[$i % count($tokens)]] = ['quantity' => 1];
    }
    if ($i % 3 == 0) {
      for ($u = 0; $u < 1 + $i % 3; $u++) {
        // A trigram without 334 (generateUnique gives null on those: the deck is refused)
        do {
          $t = CEG[mt_rand(0, $n - 1)];
        } while (in_array('334', explode('_', $t)));
        $content = content(200000 + $i * 3 + $u, [$t]);
        $entries[$content['reference']] = ['quantity' => 1, 'content' => $content];
      }
    }
    $responses[] = $deck($i, $heroes[$i % count($heroes)], $entries);
  }
  // Refused decks
  $base = count($responses);
  $one = [$cards[0] => ['quantity' => 3]];
  $u334 = content(300000, ['334_367_519']);
  $jelly = content(300001, [CEG[0]]);
  $jelly['name'] = 'Moonlight Jellyfish';
  $responses[] = $deck($base, $heroes[0], $one, false);
  $responses[] = $deck($base + 1, $heroes[0], $one + ['ALT_CORE_B_AX_08_U_29' => ['quantity' => 1]]);
  $responses[] = $deck($base + 2, $heroes[0], $one + [$u334['reference'] => ['quantity' => 1, 'content' => $u334]]);
  $responses[] = $deck($base + 3, $heroes[0], $one + [$jelly['reference'] => ['quantity' => 1, 'content' => $jelly]]);
  $responses[] = $deck($base + 4, $heroes[0], $one + ['ALT_NOPE_B_AX_99_C' => ['quantity' => 2]]);
  $responses[] = $deck($base + 5, 'ALT_NOPE_B_AX_99_C', $one);

  $dir = "$outRoot/apidecks";
  if (isset($argv[3])) {
    $responses = json_decode(file_get_contents($argv[3]), true);
    $dir = "$outRoot/apidecks_real";
  }
  $out = [];
  foreach ($responses as $i => $response) {
    $players = [['name' => 'A', 'reunionDeck' => $response], ['name' => 'B', 'deck' => 'ax_1_sierra']];
    $case = ['players' => $players, 'dice' => U_DICE];
    $before = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM `arena`.`games`')->fetch_row()[0];
    try {
      $created = V1::createGame(['players' => $players, 'dice' => U_DICE]);
      $g = (int) $created['gameId'];
      $case['phpGame'] = $g;
      Arena::openGame($g);
      $case['db'] = dumpTables();
    } catch (\Throwable $e) {
      $g = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM `arena`.`games`')->fetch_row()[0];
      $case['phpGame'] = $g > $before ? $g : null;
      $case['error'] = get_class($e) . ': ' . $e->getMessage();
    }
    if (!empty($g)) {
      dropGame($g);
    }
    $out[] = $case;
  }
  // One game per file: each holds whole tables
  @mkdir($dir, 0777, true);
  foreach (glob("$dir/*.json") as $old) {
    unlink($old);
  }
  foreach ($out as $i => $case) {
    file_put_contents(sprintf('%s/%03d.json', $dir, $i), json_encode(['mode' => 'apideck'] + $case, U_FLAGS));
  }
  echo count($out) . ' games, ' . count(array_filter($out, fn($c) => isset($c['error']))) . " refused\n";
  exit;
}

fwrite(STDERR, "usage: php tools/api_decks.php export DIR | uniques [OUT_DIR] | cardclass [OUT_DIR] | decks [OUT_DIR]\n");
exit(1);
