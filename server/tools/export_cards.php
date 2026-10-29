<?php
/**
 * Exports every card definition of the PHP engine (GAME_DIR) as data for the new engine:
 *   cards.min.json    class name => properties (effects are the engine's flow trees)
 *   card_own_id.json  class name => paths of the properties that hold the card's own id
 *   starters.min.json starter deck id => [class name => count]
 *   constants.json engine constants (locations, types, keywords...)
 * Run in the server container: php tools/export_cards.php /engine-data (engine/data mounted there)
 */
require dirname(__DIR__) . '/src/bootstrap.php';

$out = $argv[1] ?? '/server/out';
@mkdir($out, 0777, true);

/** Paths (a/b/0/c) where $v holds $needle */
function pathsOf($v, $needle, string $p = ''): array
{
  if (!is_array($v)) {
    return $v === $needle ? [$p] : [];
  }
  $out = [];
  foreach ($v as $k => $x) {
    $out = array_merge($out, pathsOf($x, $needle, $p === '' ? (string) $k : "$p/$k"));
  }
  return $out;
}

const OWN_ID_SENTINEL = 987654321;
$cards = [];
$ownId = [];
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
    // Properties built from the card's own id (FT::GAIN/AUGMENT/LOOSE($this), 'sourceId' => $this->getId()):
    // a card loaded from the DB has its id there, the export above has 'me' or null
    $withId = new $fqcn(['card_id' => OWN_ID_SENTINEL]);
    $paths = pathsOf(json_decode(json_encode($withId->getProperties()), true), OWN_ID_SENTINEL);
    if ($paths) {
      $ownId[$class] = $paths;
    }
  } catch (\Throwable $e) {
    $errors[$class] = $e->getMessage();
  }
}
ksort($cards);
ksort($ownId);
file_put_contents("$out/cards.min.json", json_encode($cards, JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES));
file_put_contents("$out/card_own_id.json", json_encode($ownId, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES) . "\n");

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
file_put_contents("$out/starters.min.json", json_encode($starters));

$consts = array_filter(get_defined_constants(true)['user'], fn($v) => is_scalar($v));
file_put_contents("$out/constants.json", json_encode($consts, JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
echo count($cards) . " cards, " . count($starters) . " starters, " . count($errors) . " errors\n";
print_r(array_slice($errors, 0, 10));
