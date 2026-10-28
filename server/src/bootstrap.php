<?php

// The game code uses relative includes (material.inc.php, modules/php/constants.inc.php)
$gameDir = getenv('GAME_DIR') ?: '/game';
chdir($gameDir);
define('GAME_DIR', $gameDir);
define('APP_GAMEMODULE_PATH', dirname(__DIR__) . '/framework/');
require_once $gameDir . '/modules/php/constants.inc.php';
require_once $gameDir . '/altered.game.php';
require_once __DIR__ . '/Arena.php';
require_once __DIR__ . '/Bot.php';
require_once __DIR__ . '/V1.php';

// BGA hides PHP notices from players; the engine relies on that in places
error_reporting(E_ALL & ~E_DEPRECATED & ~E_NOTICE & ~E_WARNING);
ini_set('display_errors', '0');
