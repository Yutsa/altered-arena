<?php

namespace Arena;

use ALT\Core\Globals;
use ALT\Managers\Cards;
use ALT\Managers\Meeples;

/**
 * Protocol v1 (protocol/protocol.ts) on top of the PHP engine: builds GameViews from the
 * engine state and turns Answers into the engine's actions. This is what the BGA client does
 * in its onEnteringState* handlers, in data instead of DOM.
 */
class V1
{
  private const ZONES = [
    'hand' => 'hand', 'reserve' => 'reserve', 'landmark' => 'landmark', 'stormLeft' => 'heroExpedition',
    'stormRight' => 'companionExpedition', 'discard' => 'discard', 'mana' => 'mana', 'limbo' => 'limbo', 'deck' => 'deck',
  ];
  private const DESTINATIONS = [
    'stormLeft' => 'heroExpedition', 'stormRight' => 'companionExpedition', 'landmark' => 'landmark', 'reserve' => 'reserve',
    'limbo' => 'spell', 'stormLeft_scout' => 'heroExpeditionScout', 'stormRight_scout' => 'companionExpeditionScout',
  ];
  private const PHASES = [0 => 'morning', 1 => 'noon', 2 => 'afternoon', 3 => 'dusk', 4 => 'night'];
  private const STORM_CARDS = [
    [[], ['forest', 'mountain', 'ocean']],
    [['forest'], ['mountain', 'ocean']],
    [['mountain'], ['forest', 'ocean']],
    [['ocean'], ['mountain', 'forest']],
    [['forest', 'mountain', 'ocean'], []],
  ];

  public static function decks(): array
  {
    return json_decode(file_get_contents(dirname(__DIR__, 2) . '/protocol/data/decks.json'), true);
  }

  // ---------------------------------------------------------------------------
  // Games
  // ---------------------------------------------------------------------------

  public static function createGame(array $body): array
  {
    $players = array_map(fn($p) => ['name' => $p['name'] ?? '', 'bot' => (bool) ($p['bot'] ?? false)], $body['players'] ?? []);
    $game = Arena::createGame(['players' => $players, 'dice' => $body['dice'] ?? null]);
    // Decks chosen in the lobby are selected right away; a custom deck (apiDeck: the content
    // actConfirmAPIDeck receives) is confirmed
    foreach (array_values($body['players'] ?? []) as $i => $p) {
      if (!empty($p['apiDeck']) && empty($p['bot'])) {
        $pId = $game['players'][$i]['id'];
        Arena::view($game['id'], $pId);
        Arena::act($game['id'], $pId, 'actConfirmAPIDeck', [$p['apiDeck']]);
        continue;
      }
      if (!empty($p['deck']) && empty($p['bot'])) {
        $pId = $game['players'][$i]['id'];
        $view = Arena::view($game['id'], $pId);
        foreach ($view['state']['args']['_private']['decks'] ?? [] as $deck) {
          if ($deck['deckId'] == $p['deck']) {
            Arena::act($game['id'], $pId, 'actSelectPrecoDeck', [$deck['deckNumber']]);
          }
        }
      }
    }
    return ['gameId' => (string) $game['id'], 'players' => $game['players']];
  }

  /** Long polling: waits until the game changes past $since (or 20 s) */
  public static function view(int $gameId, int $playerId, ?int $since = null): array
  {
    if ($since !== null) {
      $deadline = microtime(true) + 20;
      while (microtime(true) < $deadline) {
        Arena::openGame($gameId);
        $seq = (int) \BgaDb::conn()->query('SELECT COALESCE(MAX(id), 0) FROM arena_notif')->fetch_row()[0];
        if ($seq > $since) {
          break;
        }
        usleep(250000);
      }
    }
    return self::build(Arena::view($gameId, $playerId), $gameId);
  }

  public static function answer(int $gameId, int $playerId, array $answer): array
  {
    $raw = json_decode(json_encode(Arena::view($gameId, $playerId), JSON_PARTIAL_OUTPUT_ON_ERROR), true);
    $current = self::build($raw, $gameId)['decision'];
    if ($current === null) {
      throw new HttpError('Nothing to decide', 409);
    }
    if (($answer['decisionId'] ?? null) !== $current['id']) {
      throw new HttpError('This decision is no longer current', 409);
    }
    [$action, $args] = self::toAction($raw['state'], $answer);
    Arena::act($gameId, $playerId, $action, $args);
    return self::build(Arena::view($gameId, $playerId), $gameId);
  }

  // ---------------------------------------------------------------------------
  // View
  // ---------------------------------------------------------------------------

  private static function build(array $raw, int $gameId): array
  {
    // Engine objects (cards, meeples, collections) as plain arrays, as a BGA client receives them
    $raw = json_decode(json_encode($raw, JSON_PARTIAL_OUTPUT_ON_ERROR), true);
    $g = $raw['gamedatas'];
    $state = $raw['state'];
    $you = $raw['playerId'];
    $bots = array_column($raw['config']['players'], 'bot', 'id');

    $phase = self::PHASES[$g['phase']] ?? 'setup';
    if (in_array($state['name'], ['selectPrecoDeck', 'selectDeck', 'firstDayManaSelection', 'gameSetup', 'deckSetup', 'firstDay'])) {
      $phase = 'setup';
    }
    if (!empty($g['tieBreaker'])) {
      $phase = 'tiebreaker';
    }
    if ($state['name'] == 'gameEnd') {
      $phase = 'ended';
    }

    // Cards (with the engine class name as ref)
    $cards = [];
    $tokensOn = [];
    foreach ($g['meeples'] as $m) {
      if (str_starts_with($m['location'], 'card-')) {
        $tokensOn[(int) substr($m['location'], 5)][] = $m['type'];
      }
    }
    $rows = $g['cards'];
    foreach ($g['players'] as $pId => $p) {
      foreach ($p['manaCards'] ?? [] as $c) {
        $rows[] = $c;
      }
    }
    $seen = [];
    $positions = [];
    foreach ($rows as $c) {
      $id = (int) $c['id'];
      if (isset($seen[$id])) {
        continue;
      }
      $seen[$id] = true;
      $cards[] = self::card($c, $tokensOn[$id] ?? [], $positions);
    }

    // Players
    $tokens = [];
    $ascended = [];
    foreach ($g['meeples'] as $m) {
      if (in_array($m['type'], ['hero', 'companion']) && str_starts_with($m['location'], 'storm-')) {
        $tokens[(int) $m['pId']][$m['type']] = (int) substr($m['location'], 6);
      }
      if ($m['type'] == 'ascend') {
        $ascended[(int) $m['pId']][$m['location']] = true;
      }
    }
    $players = [];
    foreach ($g['players'] as $pId => $p) {
      $pId = (int) $pId;
      $hero = null;
      foreach ($cards as $c) {
        if ($c['zone'] == 'hero' && $c['owner'] == $pId) {
          $hero = $c['id'];
        }
      }
      $expeditions = [];
      foreach (['hero' => 'stormLeft', 'companion' => 'stormRight'] as $side => $loc) {
        $expeditions[$side] = [
          'position' => $tokens[$pId][$side] ?? ($side == 'hero' ? 0 : 7),
          'stats' => self::biomes($g['biomes'][$pId][$loc] ?? []),
          'blocked' => (bool) ($g['blockedExpeditions'][$pId][$loc] ?? false),
          'ascended' => $ascended[$pId][$loc] ?? false,
        ];
      }
      $players[] = [
        'id' => $pId,
        'name' => $p['name'],
        'bot' => (bool) ($bots[$pId] ?? false),
        'faction' => $p['faction'] ?? null,
        'heroCardId' => $hero,
        'handCount' => (int) ($p['handCount'] ?? 0),
        'deckCount' => (int) ($p['deckCount'] ?? 0),
        'discardCount' => (int) ($p['discardCount'] ?? 0),
        'manaTotal' => (int) ($p['totalMana'] ?? 0),
        'manaAvailable' => (int) ($p['mana'] ?? 0),
        'passed' => in_array($pId, $g['passedPlayers'] ?? []),
        'reserveSlots' => (int) ($g['reserveSlots'][$pId] ?? 2),
        'landmarkSlots' => (int) ($g['landmarkSlots'][$pId] ?? 2),
        'expeditions' => $expeditions,
      ];
    }

    // Regions of the Tumult
    $regions = [];
    $storm = $g['storm'];
    for ($i = 0; $i <= 7; $i++) {
      $card = $storm[intdiv($i + 1, 2)] ?? null;
      $biomes = null;
      if ($i == 0 || $i == 7) {
        $biomes = ['forest', 'mountain', 'ocean'];
      } elseif ($card !== null && $card['cardId'] != 6) {
        $sides = self::STORM_CARDS[$card['cardId']];
        if (!empty($card['rotated'])) {
          $sides = array_reverse($sides);
        }
        $biomes = $sides[($i - 1) % 2];
      }
      $regions[] = ['index' => $i, 'biomes' => $biomes, 'kind' => $i == 0 ? 'heroRegion' : ($i == 7 ? 'companionRegion' : 'tumult')];
    }

    $winner = null;
    if ($phase == 'ended') {
      foreach ($g['players'] as $pId => $p) {
        if ((int) $p['score'] > 0) {
          $winner = (int) $pId;
        }
      }
    }

    $waiting = $state['type'] == 'multipleactiveplayer' ? $state['multiactive'] : ($state['type'] == 'activeplayer' ? [$state['activePlayer']] : []);
    $seq = (int) $raw['cursor'];

    return [
      'gameId' => (string) $gameId,
      'you' => $you,
      'seq' => $seq,
      'day' => (int) $g['day'],
      'phase' => $phase,
      'firstPlayer' => (int) $g['firstPlayer'] ?: null,
      'activePlayer' => $state['type'] == 'activeplayer' ? $state['activePlayer'] : null,
      'winner' => $winner,
      'players' => $players,
      'cards' => $cards,
      'regions' => $regions,
      'decision' => $state['isActive'] ? self::decision($state, $g, $you, $seq) : null,
      'waitingFor' => array_values(array_map('intval', $waiting)),
      'log' => self::log($gameId, $you, $g),
    ];
  }

  private static function biomes(array $b): array
  {
    return ['forest' => (int) ($b['forest'] ?? 0), 'mountain' => (int) ($b['mountain'] ?? 0), 'ocean' => (int) ($b['ocean'] ?? 0)];
  }

  private static function card(array $c, array $tokens, array &$positions): array
  {
    $p = $c['properties'] ?? [];
    $location = $c['location'];
    $zone = self::ZONES[$location] ?? (str_starts_with($location, 'board-hero') ? 'hero' : (str_starts_with($location, 'reveal') ? 'limbo' : $location));
    $owner = (int) $c['pId'];
    $key = "$owner|$zone";
    $positions[$key] = ($positions[$key] ?? -1) + 1;

    $ref = null;
    $card = Cards::get((int) $c['id']);
    if ($card !== null) {
      $ref = (new \ReflectionClass($card))->getShortName();
    }
    $boosts = count(array_filter($tokens, fn($t) => $t == 'boost'));
    $statuses = array_values(array_unique(array_filter($tokens, fn($t) => !in_array($t, ['boost', 'forest', 'mountain', 'ocean']))));
    foreach (['gigantic', 'tough', 'defender', 'eternal', 'seasoned'] as $kw) {
      if (!empty($p[$kw])) {
        $statuses[] = $kw;
      }
    }
    $view = [
      'id' => (int) $c['id'],
      'ref' => $ref,
      'owner' => $owner,
      'zone' => $zone,
      'position' => $positions[$key],
      'exhausted' => (bool) ($p['tapped'] ?? false),
      'boosts' => $boosts,
      'counters' => new \stdClass(),
      'statuses' => array_values(array_unique($statuses)),
    ];
    if (!empty($p['token'])) {
      $view['token'] = true;
    }
    if (($p['type'] ?? '') == 'character' || !empty($p['token'])) {
      if (isset($p['forest'])) {
        $view['stats'] = [
          'forest' => (int) $p['forest'] + $boosts,
          'mountain' => (int) $p['mountain'] + $boosts,
          'ocean' => (int) $p['ocean'] + $boosts,
        ];
      }
    }
    foreach (['costHand', 'costReserve'] as $k) {
      if (isset($p[$k]) && is_numeric($p[$k])) {
        $view[$k] = (int) $p[$k];
      }
    }
    $extra = $p['extraDatas'] ?? [];
    if (!empty($extra['counter'])) {
      $view['counters'] = [$extra['counterName'] ?? 'counter' => (int) $extra['counter']];
    }
    return $view;
  }

  // ---------------------------------------------------------------------------
  // Decisions
  // ---------------------------------------------------------------------------

  private static function decision(array $state, array $g, int $you, int $seq): ?array
  {
    $args = $state['args'] ?? [];
    $private = $args['_private'] ?? [];
    $name = $state['name'];
    $base = [
      'id' => "$seq:$name",
      'prompt' => self::prompt($state, $args, $g),
    ];
    if (!empty($args['sourceId'])) {
      $base['sourceCardId'] = (int) $args['sourceId'];
    }
    if (($args['optionalAction'] ?? false) === true) {
      $base['canPass'] = true;
    }
    if (($args['previousEngineChoices'] ?? 0) > 0 && !empty($g['undo'])) {
      $base['canUndo'] = true;
    }

    switch ($name) {
      case 'selectPrecoDeck':
        if (($private['selection'] ?? null) !== null) {
          return null;
        }
        $known = array_column(self::decks(), null, 'id');
        $decks = array_map(fn($d) => $known[$d['deckId']] ?? [
          'id' => $d['deckId'], 'name' => $d['hero']['properties']['name'] ?? $d['deckId'], 'faction' => $d['faction'], 'heroRef' => null, 'size' => 40,
        ], $private['decks'] ?? []);
        return $base + ['kind' => 'deck', 'decks' => array_values($decks), 'prompt' => 'Choose your deck'];

      case 'firstDayManaSelection':
        if (!empty($private['selection'])) {
          return null;
        }
        $n = (int) ($private['n'] ?? 3);
        return $base + ['kind' => 'select', 'cardIds' => $private['cards'] ?? [], 'min' => $n, 'max' => $n,
          'prompt' => "Choose $n cards to put in your Mana"];

      case 'chooseAssignment':
        $play = [];
        foreach ($private['play'] ?? [] as $cardId => $locations) {
          $play[] = ['cardId' => (int) $cardId, 'destinations' => array_values(array_map(fn($l) => self::DESTINATIONS[$l] ?? $l, $locations))];
        }
        return $base + [
          'kind' => 'turn',
          'play' => $play,
          'support' => array_map('intval', $private['support'] ?? []),
          'tap' => array_map('intval', $private['tap'] ?? []),
          'canEndTurn' => ($args['additionalAction'] ?? false) == false,
        ];

      case 'playCard':
        $cardId = (int) $args['cardId'];
        $locations = $private['play'][$cardId] ?? [];
        return $base + ['kind' => 'turn', 'play' => [['cardId' => $cardId, 'destinations' => array_map(fn($l) => self::DESTINATIONS[$l] ?? $l, $locations)]],
          'support' => [], 'tap' => [], 'canEndTurn' => false];

      case 'confirmTurn':
      case 'confirmPartialTurn':
        return $base + ['kind' => 'confirm', 'prompt' => 'Confirm your turn'];

      case 'resolveChoice':
        $choices = [];
        foreach ($args['allChoices'] ?? [] as $id => $choice) {
          $choices[(int) $id] = [
            'id' => (int) $id,
            'label' => self::label($choice),
            'enabled' => isset($args['choices'][$id]),
          ] + (!empty($choice['sourceId']) ? ['sourceCardId' => (int) $choice['sourceId']] : []);
        }
        foreach ($args['choices'] ?? [] as $id => $choice) {
          $choices[(int) $id] ??= ['id' => (int) $id, 'label' => self::label($choice), 'enabled' => true];
        }
        return $base + ['kind' => 'choice', 'choices' => array_values($choices)];

      case 'target':
      case 'discardDo':
        $ids = array_map('intval', $args['cardIds'] ?? []);
        $n = is_numeric($args['n'] ?? null) ? (int) $args['n'] : 1;
        $max = min($n, count($ids));
        return $base + ['kind' => 'select', 'cardIds' => $ids, 'min' => ($args['upTo'] ?? false) ? 0 : $max, 'max' => $max,
          'costs' => (object) array_map('intval', $args['targetCosts'] ?? []), 'showInPicker' => (bool) ($args['manaOrbs'] ?? false)];

      case 'discard':
        $ids = array_map('intval', $private['cards'] ?? []);
        $n = is_numeric($args['n'] ?? null) ? (int) $args['n'] : count($ids);
        $max = min($n, count($ids));
        return $base + ['kind' => 'select', 'cardIds' => $ids, 'min' => ($args['upTo'] ?? false) ? 0 : $max, 'max' => $max,
          'showInPicker' => ($args['source'] ?? null) === 'mana'];

      case 'nightCleanup':
        return $base + ['kind' => 'cleanup',
          'reserve' => ['cardIds' => $private['reserveCards'] ?? [], 'n' => (int) $args['nReserve']],
          'landmarks' => ['cardIds' => $private['landmarkCards'] ?? [], 'n' => (int) $args['nLandmarks']]];

      case 'moveExpedition':
        $options = array_map(fn($e) => ['key' => json_encode($e), 'player' => (int) $e[0], 'side' => $e[1] == 'stormLeft' ? 'hero' : 'companion'],
          self::expeditionPairs($args['expeditions'] ?? []));
        return $base + ['kind' => 'expedition', 'expeditions' => $options, 'min' => 1, 'max' => 1];

      case 'targetExpedition':
        $options = array_map(function ($ex) {
          [$pId, $loc] = explode('-', $ex);
          return ['key' => $ex, 'player' => (int) $pId, 'side' => $loc == 'stormLeft' ? 'hero' : 'companion'];
        }, $args['expeditions'] ?? []);
        $n = (int) ($args['n'] ?? 1);
        return $base + ['kind' => 'expedition', 'expeditions' => $options, 'min' => min($n, count($options)), 'max' => $n];

      case 'blockExpedition':
        $options = [];
        foreach (array_keys($g['players']) as $pId) {
          foreach (['stormLeft' => 'hero', 'stormRight' => 'companion'] as $loc => $side) {
            $options[] = ['key' => "board-$loc-$pId", 'player' => (int) $pId, 'side' => $side];
          }
        }
        return $base + ['kind' => 'expedition', 'expeditions' => $options, 'min' => 1, 'max' => 1];

      case 'targetPlayer':
        $players = array_map('intval', array_keys($g['players']));
        if (!empty($args['opponentsOnly'])) {
          $players = array_values(array_filter($players, fn($p) => $p != $you));
        }
        return $base + ['kind' => 'player', 'players' => $players];

      case 'invokeToken':
        $names = ['stormLeft' => 'Hero expedition', 'stormRight' => 'Companion expedition', 'source' => 'Source expedition',
          'initialSource' => 'Source expedition', 'oppositeSource' => 'Opposite expedition', 'landmark' => 'Landmarks'];
        $locations = [];
        $owner = $args['invokePlayerId'] ?? $you;
        if (!empty($args['allPlayers'])) {
          foreach ($g['players'] as $pId => $p) {
            foreach ($args['locations'] ?? [] as $l) {
              $locations[] = ['key' => "$l-$pId", 'label' => $p['name'] . ' — ' . ($names[$l] ?? $l), 'player' => (int) $pId, 'zone' => self::DESTINATIONS[$l] ?? null];
            }
          }
        } else {
          foreach ($args['locations'] ?? [] as $l) {
            $key = in_array($l, ['stormLeft', 'stormRight']) ? "$l-$owner" : $l;
            $locations[] = ['key' => $key, 'label' => $names[$l] ?? $l, 'player' => (int) $owner, 'zone' => self::DESTINATIONS[$l] ?? null];
          }
        }
        return $base + ['kind' => 'location', 'locations' => array_map(fn($l) => array_filter($l, fn($v) => $v !== null), $locations)];

      case 'rollDie':
        return $base + ['kind' => 'number', 'values' => array_map('intval', $args['rolls'] ?? [])];

      case 'pay':
        $max = min((int) ($args['mana'] ?? 0), (int) ($args['maximum'] ?? 0));
        return $base + ['kind' => 'number', 'values' => range(0, max(0, $max))];

      case 'spend':
        return $base + ['kind' => 'number', 'values' => range(1, max(1, (int) ($args['n'] ?? 1)))];

      case 'exchange':
        return $base + ['kind' => 'exchange', 'hand' => $args['handIds'] ?? [], 'reserve' => $args['reserveIds'] ?? []];

      case 'markRegion':
        $buttons = [];
        foreach ($args['markers'] ?? [] as $m) {
          foreach ($args['regions'] ?? [] as $rid => $region) {
            $buttons[] = ['key' => $m['id'] . ':' . $rid, 'label' => ucfirst($m['type']) . ' marker → region ' . $rid];
          }
        }
        return $base + ['kind' => 'buttons', 'buttons' => $buttons];

      case 'moveRegionMarker':
        $buttons = array_map(fn($m) => ['key' => (string) $m['id'], 'label' => 'Move ' . $m['type'] . ' marker'], array_values($args['markers'] ?? []));
        return $base + ['kind' => 'buttons', 'buttons' => $buttons];

      case 'interuptReveal':
        return $base + ['kind' => 'buttons', 'buttons' => [['key' => 'ok', 'label' => 'Confirm']]];
    }

    return $base + ['kind' => 'buttons', 'buttons' => []];
  }

  /** moveExpedition args hold [pId, side] pairs */
  private static function expeditionPairs($expeditions): array
  {
    return array_values(array_filter($expeditions, fn($e) => is_array($e) && count($e) == 2));
  }

  private static function toAction(array $state, array $a): array
  {
    $type = $a['type'] ?? '';
    $name = $state['name'];
    $atomic = fn($action, ...$args) => ['actTakeAtomicAction', [$action, $args]];
    switch ($type) {
      case 'pass':
        return ['actPassOptionalAction', []];
      case 'undo':
        return ['actRestart', []];
      case 'confirm':
        return [$name == 'confirmPartialTurn' ? 'actConfirmPartialTurn' : 'actConfirmTurn', []];
      case 'deck':
        foreach ($state['args']['_private']['decks'] ?? [] as $deck) {
          if ($deck['deckId'] == $a['deck']) {
            return ['actSelectPrecoDeck', [$deck['deckNumber']]];
          }
        }
        throw new HttpError('Unknown deck', 400);
      case 'play':
        $loc = array_search($a['destination'], self::DESTINATIONS, true);
        if ($loc === false) {
          throw new HttpError('Unknown destination', 400);
        }
        return $atomic($name == 'playCard' ? 'actPlayCard' : 'actPlay', (int) $a['cardId'], $loc);
      case 'support':
        return $atomic('actSupport', (int) $a['cardId']);
      case 'tap':
        return $atomic('actTap', (int) $a['cardId']);
      case 'endTurn':
        return $atomic('actPass');
      case 'choice':
        return ['actChooseAction', [(int) $a['id']]];
      case 'select':
        $ids = array_map('intval', $a['cardIds'] ?? []);
        if ($name == 'firstDayManaSelection') {
          return ['actFirstDayManaSelection', [$ids]];
        }
        if ($name == 'discard') {
          return $atomic('actDiscard', $ids);
        }
        return $atomic('actTarget', $ids);
      case 'cleanup':
        return $atomic('actNightCleanup', array_map('intval', $a['reserve'] ?? []), array_map('intval', $a['landmarks'] ?? []));
      case 'expedition':
        $keys = $a['keys'] ?? [];
        if ($name == 'moveExpedition') {
          return $atomic('actMoveExpedition', json_decode($keys[0], true));
        }
        if ($name == 'blockExpedition') {
          return $atomic('actBlockExpedition', $keys[0]);
        }
        return $atomic('actTargetExpedition', $keys);
      case 'player':
        return $atomic('actTargetPlayer', (int) $a['player']);
      case 'location':
        return $atomic('actInvokeToken', (string) $a['key']);
      case 'number':
        $method = ['rollDie' => 'actRollDie', 'pay' => 'actPay', 'spend' => 'actSpend'][$name] ?? null;
        if ($method === null) {
          throw new HttpError('Unexpected number', 400);
        }
        return $atomic($method, (int) $a['value']);
      case 'exchange':
        return $atomic('actExchange', (int) $a['reserve'], (int) $a['hand']);
      case 'button':
        if ($name == 'markRegion') {
          [$marker, $region] = explode(':', $a['key']);
          return $atomic('actMarkRegion', (int) $marker, (int) $region);
        }
        if ($name == 'moveRegionMarker') {
          return $atomic('actMoveRegionMarker', (int) $a['key']);
        }
        if ($name == 'interuptReveal') {
          return $atomic('actInteruptReveal');
        }
    }
    throw new HttpError("Answer '$type' does not fit decision '$name'", 400);
  }

  // ---------------------------------------------------------------------------
  // Text
  // ---------------------------------------------------------------------------

  private static function prompt(array $state, array $args, array $g): string
  {
    $desc = $state['descriptionmyturn'] ?: $state['description'];
    if (is_array($desc)) {
      $desc = $desc['log'] ?? '';
    }
    $suffix = $args['descSuffix'] ?? '';
    $text = self::format((string) $desc, $args + ['you' => 'You', 'actplayer' => 'Opponent']);
    if (isset($args['description']) && in_array($state['name'], ['target', 'discardDo'])) {
      $text = self::format(is_array($args['description']) ? ($args['description']['log'] ?? '') : (string) $args['description'], $args['description']['args'] ?? []) ?: $text;
      $text = 'Choose target: ' . $text;
    }
    return trim(preg_replace('/\s+/', ' ', $text)) ?: ucfirst(preg_replace('/([A-Z])/', ' $1', $state['name']));
  }

  private static function label(array $choice): string
  {
    $d = $choice['description'] ?? '';
    $text = is_array($d) ? self::format($d['log'] ?? '', $d['args'] ?? []) : self::format((string) $d, []);
    if (!empty($choice['source']) && is_string($choice['source'])) {
      $text .= ' (' . $choice['source'] . ')';
    }
    return trim($text) ?: 'Option ' . ($choice['id'] ?? '');
  }

  /** BGA log substitution: ${key} by args[key], recursively for nested {log, args} */
  public static function format(string $log, array $args, int $depth = 0): string
  {
    if ($depth > 6) {
      return $log;
    }
    return preg_replace_callback('/\$\{([a-zA-Z0-9_]+)\}/', function ($m) use ($args, $depth) {
      $v = $args[$m[1]] ?? '';
      if (is_array($v)) {
        if (isset($v['log'])) {
          return self::format((string) $v['log'], $v['args'] ?? [], $depth + 1);
        }
        return implode(', ', array_map(fn($x) => is_scalar($x) ? (string) $x : '', $v));
      }
      if (in_array($m[1], ['card_name', 'card_name2', 'source', 'cardName', 'eventCardName'])) {
        return "[[$v]]";
      }
      return (string) $v;
    }, $log);
  }

  private static function log(int $gameId, int $you, array $g): array
  {
    $rows = \BgaDb::conn()->query(
      "SELECT id, msg, args FROM arena_notif WHERE msg <> '' AND (target IS NULL OR target = $you) ORDER BY id DESC LIMIT 100"
    )->fetch_all(MYSQLI_ASSOC);
    $out = [];
    foreach (array_reverse($rows) as $r) {
      $args = json_decode($r['args'], true) ?? [];
      $args['you'] = 'You';
      $entry = ['seq' => (int) $r['id'], 'text' => trim(self::format($r['msg'], $args))];
      if (isset($args['player_id'])) {
        $entry['player'] = (int) $args['player_id'];
      }
      if ($entry['text'] !== '') {
        $out[] = $entry;
      }
    }
    return $out;
  }
}
