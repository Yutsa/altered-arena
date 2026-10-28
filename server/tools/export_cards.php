<?php
/**
 * Exports every card definition of the PHP engine (GAME_DIR) as data for the new engine:
 *   cards.json   class name => properties (effects are the engine's flow trees)
 *   starters.json  starter deck id => [class name => count]
 *   constants.json engine constants (locations, types, keywords...)
 * Run in the server container: php tools/export_cards.php /out
 */
require dirname(__DIR__) . '/src/bootstrap.php';

$out = $argv[1] ?? '/server/out';
@mkdir($out, 0777, true);

$cards = [];
$errors = [];
foreach (glob(GAME_DIR . '/modules/php/Cards/*/*.php') as $file) {
  $dir = basename(dirname($file));
  if (in_array($dir, ['StarterDecks', 'DemoDecks'])) continue;
  $class = basename($file, '.php');
  $fqcn = "\\ALT\\Cards\\$dir\\$class";
  try {
    $card = new $fqcn(null);
    $props = $card->getProperties();
    $cards[$class] = json_decode(json_encode($props), true);
  } catch (\Throwable $e) {
    $errors[$class] = $e->getMessage();
  }
}
ksort($cards);
file_put_contents("$out/cards.json", json_encode($cards, JSON_PRETTY_PRINT | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES));

$starters = [];
foreach (glob(GAME_DIR . '/modules/php/Cards/StarterDecks/*.inc.php') as $file) {
  $before = get_defined_constants(true)['user'] ?? [];
  require_once $file;
  $after = get_defined_constants(true)['user'] ?? [];
  foreach (array_diff_key($after, $before) as $name => $list) {
    if (is_array($list)) $starters[basename($file, '.inc.php')] = $list;
  }
}
ksort($starters);
file_put_contents("$out/starters.json", json_encode($starters, JSON_PRETTY_PRINT));

$consts = array_filter(get_defined_constants(true)['user'], fn($v) => is_scalar($v));
file_put_contents("$out/constants.json", json_encode($consts, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
echo count($cards) . " cards, " . count($starters) . " starters, " . count($errors) . " errors\n";
print_r(array_slice($errors, 0, 10));
