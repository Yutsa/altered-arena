<?php

/**
 * Altered Arena game server: runs the unchanged AlteredBGA engine (GAME_DIR) on the local
 * framework replacement and exposes it as a small JSON API.
 *
 * Each game lives in its own MariaDB database (arena_g<id>). A request is handled like a BGA
 * request: fresh PHP state, one transaction, notifications stored for every client to fetch.
 */

namespace Arena;

use ALT\Core\Globals;

class HttpError extends \Exception {}

class Arena
{
  public const PLAYER_IDS = [2001, 2002];
  private const REGISTRY = 'arena';

  /**
   * Methods a client may call, with the name of each argument (as in altered.action.php).
   * The engine actions themselves (actPlay, actTarget...) go through actTakeAtomicAction.
   */
  private const ACTIONS = [
    'actSelectPrecoDeck' => ['choice'],
    'actSelectRandomDeck' => ['faction'],
    'actConfirmAPIDeck' => ['deckContent'],
    'actCancelPrecoDeckSelection' => [],
    'actFirstDayManaSelection' => ['cardIds'],
    'actCancelFirstDayManaSelection' => [],
    'actNewDayManaSelection' => ['cardIds'],
    'actPassNewDayManaSelection' => [],
    'actConfirmTurn' => [],
    'actConfirmPartialTurn' => [],
    'actRestart' => [],
    'actUndoToStep' => ['stepId'],
    'actTakeAtomicAction' => ['actionName', 'actionArgs'],
    'actChooseAction' => ['id'],
    'actPassOptionalAction' => [],
    'actAnytimeAction' => ['id'],
    'actChangePreference' => ['pref', 'value'],
  ];

  /** Actions that don't require the player to be active */
  private const ANYTIME = ['actChangePreference'];

  // ---------------------------------------------------------------------------
  // Registry
  // ---------------------------------------------------------------------------

  private static function registry(): \mysqli
  {
    $db = \BgaDb::conn();
    $db->query('CREATE DATABASE IF NOT EXISTS `' . self::REGISTRY . '`');
    $db->query('CREATE TABLE IF NOT EXISTS `' . self::REGISTRY . '`.`games` (
      `id` int unsigned NOT NULL AUTO_INCREMENT,
      `created_at` timestamp NOT NULL DEFAULT CURRENT_TIMESTAMP,
      `config` text NOT NULL,
      PRIMARY KEY (`id`)
    )');
    return $db;
  }

  public static function listGames(): array
  {
    $db = self::registry();
    $rows = $db->query('SELECT id, created_at, config FROM `' . self::REGISTRY . '`.`games` ORDER BY id DESC LIMIT 50')->fetch_all(MYSQLI_ASSOC);
    return array_map(fn($r) => ['id' => (int) $r['id'], 'createdAt' => $r['created_at']] + json_decode($r['config'], true), $rows);
  }

  public static function gameConfig(int $gameId): array
  {
    $db = self::registry();
    $row = $db->query('SELECT config FROM `' . self::REGISTRY . "`.`games` WHERE id = $gameId")->fetch_row();
    if (!$row) {
      throw new HttpError("Unknown game $gameId", 404);
    }
    return json_decode($row[0], true);
  }

  /**
   * @param array $config ['players' => [['name' => .., 'bot' => bool], ...]]
   */
  public static function createGame(array $config): array
  {
    $players = [];
    foreach (array_values($config['players'] ?? []) as $i => $p) {
      $players[] = [
        'id' => self::PLAYER_IDS[$i],
        'name' => substr(preg_replace('/[^\p{L}\p{N} _-]/u', '', (string) ($p['name'] ?? '')) ?: 'Player ' . ($i + 1), 0, 32),
        'bot' => (bool) ($p['bot'] ?? false),
      ];
    }
    if (count($players) != 2) {
      throw new HttpError('A game needs 2 players', 400);
    }
    $config = ['players' => $players];

    $db = self::registry();
    $db->query('INSERT INTO `' . self::REGISTRY . "`.`games` (config) VALUES ('" . $db->real_escape_string(json_encode($config)) . "')");
    $gameId = (int) $db->insert_id;

    self::useGame($gameId);
    \BgaDb::resetDatabase(GAME_DIR);
    \BgaDb::conn()->query('CREATE TABLE `arena_notif` (
      `id` int unsigned NOT NULL AUTO_INCREMENT,
      `packet` int unsigned NOT NULL,
      `target` int unsigned DEFAULT NULL,
      `name` varchar(64) NOT NULL,
      `msg` text NOT NULL,
      `args` mediumtext NOT NULL,
      PRIMARY KEY (`id`),
      KEY (`packet`)
    ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4');

    self::request(null, function () use ($players) {
      $names = [];
      foreach ($players as $p) {
        $names[$p['id']] = $p['name'];
      }
      \altered::harnessCreate($names, [151 => 1]);
    });
    self::runBots($gameId);
    return ['id' => $gameId] + $config;
  }

  private static function useGame(int $gameId): void
  {
    putenv("DB_NAME=arena_g$gameId");
    \BgaDb::conn()->query("CREATE DATABASE IF NOT EXISTS `arena_g$gameId`");
    \BgaDb::conn()->select_db("arena_g$gameId");
  }

  public static function openGame(int $gameId): array
  {
    $config = self::gameConfig($gameId);
    self::useGame($gameId);
    return $config;
  }

  // ---------------------------------------------------------------------------
  // Requests
  // ---------------------------------------------------------------------------

  /** Drop every static cache of the engine, as a new BGA request would start */
  private static function freshState(): void
  {
    foreach ([\ALT\Managers\Cards::class, \ALT\Managers\Meeples::class, \ALT\Managers\Players::class] as $manager) {
      if (method_exists($manager, 'invalidate')) {
        $manager::invalidate();
      }
    }
    $reset = function ($class, $prop, $value) {
      if (property_exists($class, $prop)) {
        (new \ReflectionProperty($class, $prop))->setValue(null, $value);
      }
    };
    $reset(Globals::class, 'initialized', false);
    $reset(Globals::class, 'data', []);
    $reset(\ALT\Core\Notifications::class, 'cachedValues', []);
    \Table::harnessResetGlobalsCache();
    \Table::$notifications = [];
  }

  /**
   * Runs $fn as one request of $playerId inside a transaction and stores its notifications.
   * @return mixed what $fn returns
   */
  private static function request(?int $playerId, callable $fn)
  {
    self::freshState();
    \Table::$currentPlayerId = $playerId;
    $db = \BgaDb::conn();
    $db->begin_transaction();
    try {
      $result = $fn();
      self::storeNotifications();
      $db->commit();
      return $result;
    } catch (\Throwable $e) {
      $db->rollback();
      throw $e;
    } finally {
      \Table::$currentPlayerId = null;
    }
  }

  private static function storeNotifications(): void
  {
    if (empty(\Table::$notifications)) {
      return;
    }
    $db = \BgaDb::conn();
    $packet = (int) $db->query('SELECT COALESCE(MAX(packet), 0) + 1 FROM arena_notif')->fetch_row()[0];
    $values = [];
    foreach (\Table::$notifications as $n) {
      $args = json_encode($n['args'], JSON_PARTIAL_OUTPUT_ON_ERROR | JSON_INVALID_UTF8_SUBSTITUTE);
      $values[] = sprintf(
        "(%d, %s, '%s', '%s', '%s')",
        $packet,
        $n['target'] === null ? 'NULL' : (int) $n['target'],
        $db->real_escape_string($n['name']),
        $db->real_escape_string((string) $n['msg']),
        $db->real_escape_string($args)
      );
    }
    $db->query('INSERT INTO arena_notif (packet, target, name, msg, args) VALUES ' . implode(',', $values));
  }

  private static function lock(int $gameId): void
  {
    $got = \BgaDb::conn()->query("SELECT GET_LOCK('arena_g$gameId', 15)")->fetch_row()[0];
    if ((int) $got !== 1) {
      throw new HttpError('Game is busy', 409);
    }
  }

  private static function unlock(int $gameId): void
  {
    \BgaDb::conn()->query("DO RELEASE_LOCK('arena_g$gameId')");
  }

  /**
   * A player's action: same checks as BGA (the player must be active), then the game method.
   */
  public static function act(int $gameId, int $playerId, string $action, array $args): void
  {
    $config = self::openGame($gameId);
    if (!in_array($playerId, array_column($config['players'], 'id'), true)) {
      throw new HttpError('Unknown player', 403);
    }
    if (!isset(self::ACTIONS[$action])) {
      throw new HttpError("Unknown action $action", 400);
    }
    $expected = self::ACTIONS[$action];
    if (array_is_list($args)) {
      $args = array_slice($args, 0, count($expected));
    } else {
      $args = array_map(fn($name) => $args[$name] ?? null, $expected);
    }
    if ($action == 'actTakeAtomicAction') {
      if (!preg_match('/^act[A-Za-z]+$/', (string) $args[0])) {
        throw new HttpError('Bad atomic action', 400);
      }
      $args[1] = is_array($args[1]) ? $args[1] : [];
      self::validateAlphaNum($args[1]);
    }

    self::lock($gameId);
    try {
      self::request($playerId, function () use ($playerId, $action, $args) {
        $game = new \altered();
        if (!in_array($action, self::ANYTIME, true) && !$game->gamestate->isPlayerActive($playerId)) {
          throw new \BgaUserException('It is not your turn');
        }
        $game->$action(...$args);
      });
      self::runBots($gameId);
    } finally {
      self::unlock($gameId);
    }
  }

  private static function validateAlphaNum($value): void
  {
    if (is_array($value)) {
      foreach ($value as $k => $v) {
        self::validateAlphaNum($k);
        self::validateAlphaNum($v);
      }
      return;
    }
    if (is_int($value) || is_bool($value) || $value === null) {
      return;
    }
    if (!preg_match('/^[_0-9a-zA-Z- @.-]*$/', (string) $value)) {
      throw new HttpError('Bad argument value', 400);
    }
  }

  /** Lets bot players take every decision that is theirs, one request per decision */
  public static function runBots(int $gameId): void
  {
    $config = self::gameConfig($gameId);
    $bots = array_column(array_filter($config['players'], fn($p) => $p['bot']), 'id');
    if (empty($bots)) {
      return;
    }
    for ($i = 0; $i < 200; $i++) {
      $decision = self::request(null, function () use ($bots) {
        $game = new \altered();
        foreach ($bots as $botId) {
          if ($game->gamestate->isPlayerActive($botId)) {
            return [$botId, Bot::decide($game, $botId)];
          }
        }
        return null;
      });
      if ($decision === null || $decision[1] === null) {
        return;
      }
      [$botId, [$action, $args]] = $decision;
      try {
        self::request($botId, function () use ($action, $args) {
          $game = new \altered();
          $game->$action(...$args);
        });
      } catch (\Throwable $e) {
        // A bad bot choice must not block the game: try the fallback once, else stop
        error_log("bot $botId $action " . json_encode($args) . ': ' . $e->getMessage());
        $fallback = self::request(null, fn() => Bot::fallback(new \altered(), $botId));
        if ($fallback === null) {
          return;
        }
        try {
          self::request($botId, function () use ($fallback) {
            $game = new \altered();
            $game->{$fallback[0]}(...$fallback[1]);
          });
        } catch (\Throwable $e2) {
          error_log("bot $botId fallback failed: " . $e2->getMessage());
          return;
        }
      }
    }
  }

  // ---------------------------------------------------------------------------
  // Views
  // ---------------------------------------------------------------------------

  /** Everything a client needs to draw the table for $playerId (getAllDatas + state) */
  public static function view(int $gameId, int $playerId): array
  {
    $config = self::openGame($gameId);
    if (!in_array($playerId, array_column($config['players'], 'id'), true)) {
      throw new HttpError('Unknown player', 403);
    }
    return self::request($playerId, function () use ($gameId, $playerId, $config) {
      $game = new \altered();
      $state = $game->gamestate->state();
      $args = self::privateArgs($state['args'] ?? null, $playerId);
      $cursor = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM arena_notif')->fetch_row()[0];
      return [
        'gameId' => $gameId,
        'playerId' => $playerId,
        'config' => $config,
        'cursor' => $cursor,
        'gamedatas' => $game->getAllDatas(),
        'state' => [
          'id' => (int) $state['id'],
          'name' => $state['name'] ?? '',
          'type' => $state['type'] ?? '',
          'description' => $state['description'] ?? '',
          'descriptionmyturn' => $state['descriptionmyturn'] ?? '',
          'args' => $args,
          'activePlayer' => (int) $game->getActivePlayerId(),
          'multiactive' => $game->gamestate->getActivePlayerList(),
          'isActive' => $game->gamestate->isPlayerActive($playerId),
        ],
      ];
    });
  }

  /** BGA sends each player only their own entry of args['_private'] */
  private static function privateArgs($args, int $playerId)
  {
    if (!is_array($args)) {
      return $args;
    }
    if (array_key_exists('_private', $args)) {
      $private = $args['_private'];
      if (is_array($private) && array_key_exists($playerId, $private)) {
        $args['_private'] = $private[$playerId];
      } else {
        unset($args['_private']);
      }
    }
    return $args;
  }

  /** Notifications visible to $playerId after notification id $after, grouped by request */
  public static function notifications(int $gameId, int $playerId, int $after): array
  {
    self::openGame($gameId);
    $rows = \BgaDb::conn()->query(
      "SELECT id, packet, name, msg, args FROM arena_notif WHERE id > $after AND (target IS NULL OR target = $playerId) ORDER BY id LIMIT 2000"
    )->fetch_all(MYSQLI_ASSOC);
    $cursor = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM arena_notif')->fetch_row()[0];
    return [
      'cursor' => $cursor,
      'notifications' => array_map(fn($r) => [
        'id' => (int) $r['id'],
        'packet' => (int) $r['packet'],
        'name' => $r['name'],
        'msg' => $r['msg'],
        'args' => json_decode($r['args'], true),
      ], $rows),
    ];
  }

  /**
   * Full state (hidden information included), in the snapshot format of the Rust engine
   * (Game::from_snapshot): used to start the new engine from the same position and compare them.
   */
  public static function oracleState(int $gameId): array
  {
    $config = self::openGame($gameId);
    return self::request(null, function () use ($config) {
      $game = new \altered();
      $tokensOn = [];
      $positions = [];
      foreach (\ALT\Managers\Meeples::getAll() as $m) {
        $loc = $m->getLocation();
        if (str_starts_with($loc, 'card-')) {
          $tokensOn[(int) substr($loc, 5)][] = $m->getType();
        } elseif (str_starts_with($loc, 'storm-') && in_array($m->getType(), ['hero', 'companion'])) {
          $positions[(int) $m->getPId()][$m->getType()] = (int) substr($loc, 6);
        }
      }
      $cards = [];
      foreach (\ALT\Managers\Cards::getAll() as $id => $c) {
        $loc = $c->getLocation();
        $zone = match (true) {
          str_starts_with($loc, 'deck-') => 'deck',
          str_starts_with($loc, 'board-hero') => 'hero',
          $loc == 'stormLeft' => 'heroExpedition',
          $loc == 'stormRight' => 'companionExpedition',
          default => $loc,
        };
        $tokens = $tokensOn[(int) $id] ?? [];
        $cards[] = [
          'id' => (int) $id,
          'ref' => (new \ReflectionClass($c))->getShortName(),
          'owner' => (int) $c->getPId(),
          'zone' => $zone,
          'order' => (int) $c->getState(),
          'exhausted' => (bool) $c->isTapped(),
          'boosts' => count(array_filter($tokens, fn($t) => $t == 'boost')),
          'statuses' => array_values(array_filter($tokens, fn($t) => in_array($t, ['fleeting', 'anchored', 'asleep']))),
          'counter' => (int) ($c->getExtraDatas()['counter'] ?? 0),
        ];
      }
      usort($cards, fn($a, $b) => [$a['owner'], $a['zone'], $a['order'], $a['id']] <=> [$b['owner'], $b['zone'], $b['order'], $b['id']]);
      $players = [];
      foreach (\ALT\Managers\Players::getAll() as $pId => $p) {
        $cfg = current(array_filter($config['players'], fn($x) => $x['id'] == $pId));
        $players[] = [
          'id' => (int) $pId,
          'name' => $p->getName(),
          'bot' => (bool) ($cfg['bot'] ?? false),
          'position' => [$positions[$pId]['hero'] ?? 0, $positions[$pId]['companion'] ?? 7],
        ];
      }
      $storm = array_map(fn($t) => ['card' => (int) $t['cardId'], 'rotated' => (bool) ($t['rotated'] ?? false), 'revealed' => (bool) ($t['visible'] ?? false)], Globals::getStorm());
      $state = $game->gamestate->state(true);
      return [
        'players' => $players,
        'first' => (int) Globals::getFirstPlayer(),
        'day' => (int) Globals::getDay(),
        'phase' => (int) Globals::getPhase(),
        'state' => $state['name'] ?? '',
        'storm' => $storm,
        'cards' => $cards,
        // Id of the next card created (tokens): deck selection creates and deletes cards
        'nextCardId' => (int) \BgaDb::conn()->query("SELECT AUTO_INCREMENT FROM information_schema.TABLES WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = 'cards'")->fetch_row()[0],
      ];
    });
  }
}
