# Changelog

## [0.6.0](https://github.com/Yutsa/altered-arena/compare/v0.5.0...v0.6.0) (2026-11-06)


### Features

* stage die rolls and card effects, faster Dusk moves, Tumult card flip, printed card text ([#19](https://github.com/Yutsa/altered-arena/issues/19)) ([ad23f12](https://github.com/Yutsa/altered-arena/commit/ad23f125fce885748235532b21baef85a04bec7c))


### Bug Fixes

* hide the front of cards turned face down for the Mana zone on Safari and Firefox ([#17](https://github.com/Yutsa/altered-arena/issues/17)) ([1cf5bb0](https://github.com/Yutsa/altered-arena/commit/1cf5bb0a888fcbc0ba07716f52264b15d2cefef7))

## [0.5.0](https://github.com/Yutsa/altered-arena/compare/v0.4.0...v0.5.0) (2026-11-05)


### Features

* **client:** end of the day in steps (Dusk, hero, companion, Night) ([#14](https://github.com/Yutsa/altered-arena/issues/14)) ([3f2135b](https://github.com/Yutsa/altered-arena/commit/3f2135bf3832bee27b0decd62f6c1daf8b2e30fe))
* **client:** taller desktop track, a line of tokens per player, larger totals ([#15](https://github.com/Yutsa/altered-arena/issues/15)) ([da9af6c](https://github.com/Yutsa/altered-arena/commit/da9af6c502ea9a0f6c1f006db4f3303e399aa85a))

## [0.4.0](https://github.com/Yutsa/altered-arena/compare/v0.3.0...v0.4.0) (2026-11-05)


### Features

* **client:** biome totals as a post at each end of the track ([#9](https://github.com/Yutsa/altered-arena/issues/9)) ([f9f0325](https://github.com/Yutsa/altered-arena/commit/f9f03252e093523bea3d11dc4dab6fc400b78a9b))
* **client:** main button under the track, larger Reserve and Landmarks ([#12](https://github.com/Yutsa/altered-arena/issues/12)) ([f404064](https://github.com/Yutsa/altered-arena/commit/f404064a8ca7f43552ac8dd8846515148ed67440))


### Bug Fixes

* **client:** clicking a card in the target zone plays the selected card there ([#13](https://github.com/Yutsa/altered-arena/issues/13)) ([8541553](https://github.com/Yutsa/altered-arena/commit/854155321274e0eed7cecca73ff5672990e648c3))
* **server:** show the hero of every deck AI ([#11](https://github.com/Yutsa/altered-arena/issues/11)) ([2b397dc](https://github.com/Yutsa/altered-arena/commit/2b397dc6ef1738a0e2efc3f1e454c90da06c328f))

## [0.3.0](https://github.com/Yutsa/altered-arena/compare/v0.2.0...v0.3.0) (2026-11-05)


### Features

* play by touch on phones, and menus made for phones ([#7](https://github.com/Yutsa/altered-arena/issues/7)) ([20f2655](https://github.com/Yutsa/altered-arena/commit/20f2655d74efffbaf812fb7161ee114f69f0981b))


### Bug Fixes

* keep the sorted hand without gaps between groups ([#4](https://github.com/Yutsa/altered-arena/issues/4)) ([426d1ce](https://github.com/Yutsa/altered-arena/commit/426d1ce98b1a7b2bb20800757d9387965fe72da2))


### Performance Improvements

* **client:** fine-grained game state with zustand, structural sharing and React Compiler ([#8](https://github.com/Yutsa/altered-arena/issues/8)) ([ae14d2f](https://github.com/Yutsa/altered-arena/commit/ae14d2f96a6837da6067a9616fbdf897ad0a37a4))

## [0.2.0](https://github.com/Yutsa/altered-arena/compare/altered-arena-v0.1.0...altered-arena-v0.2.0) (2026-11-05)


### Features

* center the decision prompt and the effects queue above the hand ([1975e38](https://github.com/Yutsa/altered-arena/commit/1975e38d53de2eabc1054ea41a66ef091312dd53))
* drop the hover tooltips of the client ([f58632b](https://github.com/Yutsa/altered-arena/commit/f58632ba15d76e03b0445c83eb97cff7a43aa281))
* minimize the effects queue with a button instead of hovering the hand ([fc37ac3](https://github.com/Yutsa/altered-arena/commit/fc37ac3dad3c345541a631924cfec3c85890949d))
* resolve identical effects of copies of a card without asking ([949f0a2](https://github.com/Yutsa/altered-arena/commit/949f0a2328d4ba08cab25757601d77a509639a9b))
* show information prompts on the track and fit crowded zones and mana ([2cb292b](https://github.com/Yutsa/altered-arena/commit/2cb292b8d6c623d1bbb520410ed3ee300d5f7d0b))
* show the opponent's spells beside the mat, then their targets, before their result ([d3b17ee](https://github.com/Yutsa/altered-arena/commit/d3b17eef82529bb28776c54715e2f49b377f8287))
* take single-button decisions on the main action button ([0c8f423](https://github.com/Yutsa/altered-arena/commit/0c8f423aeefe935357c5a0513fca20cb14e8c98b))


### Bug Fixes

* offer the engine undo only when it takes something back ([9b09a89](https://github.com/Yutsa/altered-arena/commit/9b09a895bf4d9ecccec081d4861aa2d294b85156))
* undo against bots goes back to the player's last choice ([dbf039b](https://github.com/Yutsa/altered-arena/commit/dbf039b2d50b643b8b620ca66f5d4a9647cc5954))
