<?php

namespace Arena;

use ALT\Managers\Cards;
use ALT\Managers\Players;

/**
 * A simple opponent for solo games. It only answers with what the state args allow, through
 * the same game methods as a human client, so it cannot break a rule: at worst it plays badly.
 *
 * decide() returns [method, args] for the game object, or null when it has nothing to do.
 */
class Bot
{
  public static function decide(\altered $game, int $botId): ?array
  {
    $state = $game->gamestate->state();
    $name = $state['name'];
    $args = json_decode(json_encode($state['args'] ?? []), true) ?? [];
    $private = $args['_private'][$botId] ?? $args['_private'] ?? [];

    switch ($name) {
      case 'selectPrecoDeck':
        if (($private['selection'] ?? null) !== null) {
          return null;
        }
        $decks = array_values($private['decks'] ?? []);
        if (empty($decks)) {
          return null;
        }
        $deck = $decks[array_rand($decks)];
        return ['actSelectPrecoDeck', [$deck['deckNumber']]];

      case 'firstDayManaSelection':
        if (!empty($private['selection'])) {
          return null;
        }
        $cards = self::sortByCost($private['cards'] ?? [], true);
        return ['actFirstDayManaSelection', [array_slice($cards, 0, $private['n'] ?? 3)]];

      case 'chooseAssignment':
        return self::chooseAssignment($args, $private, $botId);

      case 'confirmTurn':
        return ['actConfirmTurn', []];

      case 'confirmPartialTurn':
        return ['actConfirmPartialTurn', []];

      case 'resolveChoice':
        $choices = array_values($args['choices'] ?? []);
        if (empty($choices)) {
          return self::passOrNull($args);
        }
        return ['actChooseAction', [$choices[array_rand($choices)]['id']]];

      case 'target':
      case 'discardDo':
        $ids = $args['cardIds'] ?? [];
        $n = min((int) ($args['n'] ?? 1), count($ids));
        // Prefer opponent cards for effects, own cheap cards otherwise: a coin flip keeps it simple
        $ids = self::preferOwner($ids, $botId, mt_rand(0, 1) == 1);
        return ['actTakeAtomicAction', ['actTarget', [array_slice($ids, 0, $n)]]];

      case 'discard':
        $ids = self::sortByCost($private['cards'] ?? [], false);
        $n = is_numeric($args['n'] ?? null) ? (int) $args['n'] : 1;
        if (($args['upTo'] ?? false) && ($args['optionalAction'] ?? false)) {
          return ['actPassOptionalAction', []];
        }
        return ['actTakeAtomicAction', ['actDiscard', [array_slice($ids, 0, min($n, count($ids)))]]];

      case 'nightCleanup':
        $reserve = self::sortByCost($private['reserveCards'] ?? [], false);
        $landmarks = self::sortByCost($private['landmarkCards'] ?? [], false);
        return ['actTakeAtomicAction', ['actNightCleanup', [
          array_slice($reserve, 0, (int) ($args['nReserve'] ?? 0)),
          array_slice($landmarks, 0, (int) ($args['nLandmarks'] ?? 0)),
        ]]];

      case 'moveExpedition':
        $expeditions = $args['expeditions'] ?? [];
        if (empty($expeditions)) {
          return self::passOrNull($args);
        }
        $mine = array_values(array_filter($expeditions, fn($e) => (int) $e[0] == $botId));
        $pick = $mine[0] ?? $expeditions[0];
        return ['actTakeAtomicAction', ['actMoveExpedition', [$pick]]];

      case 'invokeToken':
        $locations = $args['locations'] ?? [];
        if (empty($locations)) {
          return self::passOrNull($args);
        }
        $location = $locations[array_rand($locations)];
        $owner = $args['invokePlayerId'] ?? $botId;
        if (($args['allPlayers'] ?? false) || in_array($location, ['stormLeft', 'stormRight'])) {
          $location .= '-' . $owner;
        }
        return ['actTakeAtomicAction', ['actInvokeToken', [$location]]];

      case 'targetPlayer':
        $opponent = Players::getNextId(Players::get($botId));
        return ['actTakeAtomicAction', ['actTargetPlayer', [$opponent]]];

      case 'targetExpedition':
        $expeditions = $args['expeditions'] ?? [];
        return ['actTakeAtomicAction', ['actTargetExpedition', [array_slice($expeditions, 0, (int) ($args['n'] ?? 1))]]];

      case 'blockExpedition':
        $opponent = Players::getNextId(Players::get($botId));
        return ['actTakeAtomicAction', ['actBlockExpedition', ["board-stormLeft-$opponent"]]];

      case 'rollDie':
        $rolls = $args['rolls'] ?? [];
        if (empty($rolls)) {
          return self::passOrNull($args);
        }
        return ['actTakeAtomicAction', ['actRollDie', [max($rolls)]]];

      case 'pay':
        $max = min((int) ($args['mana'] ?? 0), (int) ($args['maximum'] ?? 0));
        return ['actTakeAtomicAction', ['actPay', [$max]]];

      case 'spend':
        return ['actTakeAtomicAction', ['actSpend', [(int) ($args['n'] ?? 1)]]];

      case 'exchange':
        if ($args['optionalAction'] ?? false) {
          return ['actPassOptionalAction', []];
        }
        $reserve = $args['reserveIds'] ?? [];
        $hand = $args['handIds'] ?? [];
        if (empty($reserve) || empty($hand)) {
          return self::passOrNull($args);
        }
        return ['actTakeAtomicAction', ['actExchange', [$reserve[0], $hand[0]]]];

      case 'interuptReveal':
        return ['actTakeAtomicAction', ['actInteruptReveal', []]];

      case 'markRegion':
        $markers = array_values($args['markers'] ?? []);
        $regions = array_values($args['regions'] ?? []);
        if (empty($markers) || empty($regions)) {
          return self::passOrNull($args);
        }
        $region = $regions[array_rand($regions)];
        return ['actTakeAtomicAction', ['actMarkRegion', [$markers[0]['id'], is_array($region) ? ($region['id'] ?? 0) : $region]]];

      case 'moveRegionMarker':
        $markers = array_values($args['markers'] ?? []);
        if (empty($markers)) {
          return self::passOrNull($args);
        }
        return ['actTakeAtomicAction', ['actMoveRegionMarker', [$markers[0]['id']]]];

      case 'playCard':
        $play = $private['play'] ?? [];
        $cardId = $args['cardId'] ?? array_key_first($play);
        $locations = $play[$cardId] ?? [];
        if (empty($locations)) {
          return self::passOrNull($args);
        }
        return ['actTakeAtomicAction', ['actPlayCard', [$cardId, $locations[array_rand($locations)]]]];

      case 'impossibleAction':
        return self::passOrNull($args);
    }

    return self::passOrNull($args);
  }

  /** Second chance after a refused decision: pass if possible, else confirm */
  public static function fallback(\altered $game, int $botId): ?array
  {
    $state = $game->gamestate->state();
    $args = json_decode(json_encode($state['args'] ?? []), true) ?? [];
    $possible = $state['possibleactions'] ?? [];
    if (($args['optionalAction'] ?? false) && in_array('actPassOptionalAction', $possible)) {
      return ['actPassOptionalAction', []];
    }
    if ($state['name'] == 'chooseAssignment') {
      return ['actTakeAtomicAction', ['actPass', []]];
    }
    if ($state['name'] == 'target' || $state['name'] == 'discardDo') {
      $ids = $args['cardIds'] ?? [];
      return ['actTakeAtomicAction', ['actTarget', [array_slice($ids, 0, min((int) ($args['n'] ?? 1), count($ids)))]]];
    }
    return null;
  }

  private static function passOrNull(array $args): ?array
  {
    return ($args['optionalAction'] ?? false) ? ['actPassOptionalAction', []] : null;
  }

  /**
   * Afternoon decision: play the best affordable card, else pass.
   * A card's value is its total statistics (spells and permanents count as 4) minus its cost.
   */
  private static function chooseAssignment(array $args, array $private, int $botId): ?array
  {
    $play = $private['play'] ?? [];
    $best = null;
    foreach ($play as $cardId => $locations) {
      $card = Cards::get((int) $cardId);
      if ($card === null || empty($locations)) {
        continue;
      }
      $fromReserve = $card->getLocation() == RESERVE;
      $cost = $fromReserve ? (int) $card->getCostReserve() : (int) $card->getCostHand();
      $stats = (int) ($card->getForest() ?? 0) + (int) ($card->getMountain() ?? 0) + (int) ($card->getOcean() ?? 0);
      $value = ($stats > 0 ? $stats : 4) - 0.6 * $cost + mt_rand(0, 100) / 100;
      foreach ($locations as $location) {
        $v = $value;
        if (str_contains($location, 'scout')) {
          $v -= 1.5;
        }
        if ($location == RESERVE) {
          $v -= 3;
        }
        if ($best === null || $v > $best[0]) {
          $best = [$v, (int) $cardId, $location];
        }
      }
    }
    if ($best !== null) {
      return ['actTakeAtomicAction', ['actPlay', [$best[1], $best[2]]]];
    }
    if (!($args['additionalAction'] ?? false)) {
      return ['actTakeAtomicAction', ['actPass', []]];
    }
    return ['actPassOptionalAction', []];
  }

  private static function sortByCost(array $ids, bool $desc): array
  {
    $cost = function ($id) {
      $card = Cards::get((int) $id);
      return $card ? (int) $card->getCostHand() : 0;
    };
    usort($ids, fn($a, $b) => $desc ? $cost($b) <=> $cost($a) : $cost($a) <=> $cost($b));
    return $ids;
  }

  private static function preferOwner(array $ids, int $botId, bool $opponentFirst): array
  {
    usort($ids, function ($a, $b) use ($botId, $opponentFirst) {
      $oa = (int) (Cards::get((int) $a)?->getPId() == $botId);
      $ob = (int) (Cards::get((int) $b)?->getPId() == $botId);
      return $opponentFirst ? $oa <=> $ob : $ob <=> $oa;
    });
    return $ids;
  }
}
