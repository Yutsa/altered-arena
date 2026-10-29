<?php

/**
 * Reference runs for rules/tests/v1_games.rs: games created by V1::createGame (decks chosen in
 * the lobby, or by the bot, or still to choose), the tables right after creation, then a few
 * dozen answers (the first option of each decision), recording both players' GameViews after
 * each step. The Rust translation creates the same games (same game id, so the same seeded
 * randomness) and must give the same tables and views.
 *
 *   docker compose exec -T server php < rules/tests/data/v1_games_php.php > rules/tests/data/v1_games_php.json
 */

require '/server/src/bootstrap.php';

use Arena\Arena;
use Arena\V1;

const STEPS = 30;

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

function dropGame(int $g): void
{
  $db = \BgaDb::conn();
  foreach ($db->query("SHOW DATABASES LIKE 'arena_g{$g}%'")->fetch_all() as [$name]) {
    if ($name === "arena_g$g" || str_starts_with($name, "arena_g{$g}_")) {
      $db->query("DROP DATABASE `$name`");
    }
  }
}

/** The first option of a decision */
function firstAnswer(array $d): array
{
  $pass = ['type' => 'pass'];
  switch ($d['kind']) {
    case 'deck':
      return ['type' => 'deck', 'deck' => $d['decks'][0]['id']];
    case 'turn':
      foreach ($d['play'] as $p) {
        if (!empty($p['destinations'])) {
          return ['type' => 'play', 'cardId' => $p['cardId'], 'destination' => $p['destinations'][0]];
        }
      }
      return !empty($d['canEndTurn']) ? ['type' => 'endTurn'] : $pass;
    case 'select':
      return ['type' => 'select', 'cardIds' => array_slice($d['cardIds'], 0, $d['max'])];
    case 'choice':
      foreach ($d['choices'] as $c) {
        if ($c['enabled']) {
          return ['type' => 'choice', 'id' => $c['id']];
        }
      }
      return $pass;
    case 'expedition':
      return ['type' => 'expedition', 'keys' => array_slice(array_column($d['expeditions'], 'key'), 0, max(1, $d['min']))];
    case 'player':
      return ['type' => 'player', 'player' => $d['players'][0]];
    case 'number':
      return ['type' => 'number', 'value' => $d['values'][0]];
    case 'location':
      return ['type' => 'location', 'key' => $d['locations'][0]['key']];
    case 'buttons':
      return $d['buttons'] ? ['type' => 'button', 'key' => $d['buttons'][0]['key']] : $pass;
    case 'cleanup':
      return ['type' => 'cleanup', 'reserve' => array_slice($d['reserve']['cardIds'], 0, $d['reserve']['n']), 'landmarks' => array_slice($d['landmarks']['cardIds'], 0, $d['landmarks']['n'])];
    case 'confirm':
      return ['type' => 'confirm'];
  }
  return $pass;
}

$runs = [];
$configs = [
  ['players' => [['name' => 'Alice', 'deck' => 'ax_1_sierra'], ['name' => 'Bob', 'deck' => 'or_85_matz']], 'dice' => [3, 1, 6, 4, 2, 5]],
  ['players' => [['name' => 'Alice', 'deck' => 'mu_65_kauri'], ['name' => 'Bot', 'bot' => true]], 'dice' => [6, 2, 4, 1, 5, 3]],
  ['players' => [['name' => 'Bot', 'bot' => true], ['name' => 'Zoé!', 'deck' => 'yz_105_sam']], 'dice' => [2, 2, 5, 6, 1, 4]],
  ['players' => [['name' => 'Alice'], ['name' => 'Bob']], 'dice' => [1, 2, 3, 4, 5, 6]],
];
foreach ($configs as $config) {
  $created = V1::createGame($config);
  $g = (int) $created['gameId'];
  Arena::openGame($g);
  $db = dumpTables();
  $steps = [];
  $humans = array_values(array_filter($created['players'], fn($p) => !$p['bot']));
  for ($i = 0; $i < STEPS; $i++) {
    $views = [];
    foreach ($humans as $p) {
      $views[$p['id']] = V1::view($g, $p['id']);
    }
    $step = ['views' => $views];
    $waiting = array_values(array_filter(reset($views)['waitingFor'], fn($p) => isset($views[$p])));
    if (empty($waiting)) {
      $steps[] = $step;
      break;
    }
    $p = $waiting[0];
    $d = $views[$p]['decision'];
    $a = firstAnswer($d) + ['decisionId' => $d['id']];
    $step['player'] = $p;
    $step['answer'] = $a;
    try {
      V1::answer($g, $p, $a);
    } catch (\Throwable $e) {
      $step['error'] = $e->getMessage();
      $steps[] = $step;
      break;
    }
    $steps[] = $step;
  }
  $runs[] = ['config' => $config, 'phpGame' => $g, 'players' => $created['players'], 'db' => $db, 'steps' => $steps];
  dropGame($g);
}
echo json_encode(['runs' => $runs], JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
