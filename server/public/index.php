<?php

/**
 * HTTP entry point (php -S router). Routes:
 *   GET  /api/health
 *   GET  /api/games                         list
 *   POST /api/games                         {players: [{name, bot}, {name, bot}]}
 *   GET  /api/games/{id}/view?player=       table as seen by a player
 *   GET  /api/games/{id}/notifs?player=&after=
 *   POST /api/games/{id}/act                {player, action, args}
 *   POST /api/games/{id}/bots               lets bot players resume (after a server restart...)
 * Protocol v1 (protocol/protocol.ts): /api/v1/cards, /api/v1/decks, /api/v1/games[/{id}[/answer]]
 * Card art: /assets/cards/{faction}/{asset}.jpg
 * Anything else is served from the client build (CLIENT_DIR) when present.
 */

require dirname(__DIR__) . '/src/bootstrap.php';

use Arena\Arena;
use Arena\HttpError;
use Arena\V1;

$uri = parse_url($_SERVER['REQUEST_URI'], PHP_URL_PATH);
$method = $_SERVER['REQUEST_METHOD'];

header('Access-Control-Allow-Origin: *');
header('Access-Control-Allow-Headers: Content-Type');
header('Access-Control-Allow-Methods: GET, POST, OPTIONS');
if ($method == 'OPTIONS') {
  http_response_code(204);
  return true;
}

if (preg_match('#^/assets/cards/([A-Z]{2})/([A-Za-z0-9_]+)\.jpg$#', $uri, $m)) {
  return serveFile(GAME_DIR . "/img/illustrations/{$m[1]}/{$m[2]}.jpg", 'image/jpeg', true);
}
if (!str_starts_with($uri, '/api/')) {
  return serveStatic($uri);
}

function respond(int $code, $data): void
{
  http_response_code($code);
  header('Content-Type: application/json');
  header('Cache-Control: no-store');
  echo json_encode($data, JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
}

function body(): array
{
  return json_decode(file_get_contents('php://input') ?: '{}', true) ?? [];
}

try {
  if ($uri == '/api/v1/cards') {
    serveFile(dirname(__DIR__, 2) . '/protocol/data/cards.json', 'application/json', true);
  } elseif ($uri == '/api/v1/decks') {
    respond(200, V1::decks());
  } elseif ($uri == '/api/v1/games' && $method == 'POST') {
    respond(200, V1::createGame(body()));
  } elseif (preg_match('#^/api/v1/games/(\d+)$#', $uri, $m)) {
    respond(200, V1::view((int) $m[1], (int) ($_GET['player'] ?? 0), isset($_GET['since']) ? (int) $_GET['since'] : null));
  } elseif (preg_match('#^/api/v1/games/(\d+)/answer$#', $uri, $m) && $method == 'POST') {
    respond(200, V1::answer((int) $m[1], (int) ($_GET['player'] ?? 0), body()));
  } elseif ($uri == '/api/health') {
    respond(200, ['ok' => true]);
  } elseif ($uri == '/api/games' && $method == 'GET') {
    respond(200, Arena::listGames());
  } elseif ($uri == '/api/games' && $method == 'POST') {
    respond(200, Arena::createGame(body()));
  } elseif (preg_match('#^/api/games/(\d+)/view$#', $uri, $m)) {
    respond(200, Arena::view((int) $m[1], (int) ($_GET['player'] ?? 0)));
  } elseif (preg_match('#^/api/games/(\d+)/notifs$#', $uri, $m)) {
    respond(200, Arena::notifications((int) $m[1], (int) ($_GET['player'] ?? 0), (int) ($_GET['after'] ?? 0)));
  } elseif (preg_match('#^/api/games/(\d+)/act$#', $uri, $m) && $method == 'POST') {
    $b = body();
    Arena::act((int) $m[1], (int) ($b['player'] ?? 0), (string) ($b['action'] ?? ''), (array) ($b['args'] ?? []));
    respond(200, Arena::view((int) $m[1], (int) $b['player']));
  } elseif (preg_match('#^/api/games/(\d+)/bots$#', $uri, $m) && $method == 'POST') {
    Arena::openGame((int) $m[1]);
    Arena::runBots((int) $m[1]);
    respond(200, ['ok' => true]);
  } else {
    respond(404, ['error' => 'Not found']);
  }
} catch (HttpError $e) {
  respond($e->getCode() ?: 400, ['error' => $e->getMessage()]);
} catch (\BgaUserException | \Bga\GameFramework\UserException | \Bga\GameFramework\VisibleSystemException $e) {
  // Errors meant for the player (illegal move...)
  respond(400, ['error' => $e->getMessage(), 'user' => true]);
} catch (\Throwable $e) {
  error_log((string) $e);
  respond(500, ['error' => $e->getMessage(), 'where' => basename($e->getFile()) . ':' . $e->getLine()]);
}
return true;

function serveFile(string $file, string $type, bool $cache): bool
{
  if (!is_file($file)) {
    http_response_code(404);
    return true;
  }
  header("Content-Type: $type");
  if ($cache) {
    header('Cache-Control: public, max-age=86400');
  }
  readfile($file);
  return true;
}

function serveStatic(string $uri)
{
  $root = getenv('CLIENT_DIR') ?: '/client';
  $file = realpath($root . $uri);
  if ($file === false || !str_starts_with($file, realpath($root) ?: '/nonexistent') || is_dir($file)) {
    $file = $root . '/index.html';
  }
  if (!is_file($file)) {
    http_response_code(404);
    echo 'Client not built';
    return true;
  }
  $types = ['html' => 'text/html', 'js' => 'text/javascript', 'css' => 'text/css', 'svg' => 'image/svg+xml', 'png' => 'image/png',
    'jpg' => 'image/jpeg', 'webp' => 'image/webp', 'json' => 'application/json', 'webmanifest' => 'application/manifest+json',
    'woff2' => 'font/woff2', 'woff' => 'font/woff', 'ttf' => 'font/ttf'];
  header('Content-Type: ' . ($types[pathinfo($file, PATHINFO_EXTENSION)] ?? 'application/octet-stream'));
  readfile($file);
  return true;
}
