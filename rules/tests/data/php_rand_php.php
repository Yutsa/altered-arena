<?php
// Reference values for rules/src/php_rand.rs (tests/php_rand.rs), from PHP 8.2 in the server container:
//   docker compose exec -T server php < rules/tests/data/php_rand_php.php > rules/tests/data/php_rand_php.json
error_reporting(E_ALL & ~E_DEPRECATED);
$out = ['php' => PHP_VERSION, 'mt' => [], 'range' => [], 'shuffle' => [], 'rand' => []];

// mt_rand(): past two reloads of the state
foreach ([0, 1, 42, 5489, 12345, 0x7FFFFFFF, 0xFFFFFFFF, -1, 1000003 * 7 + 3] as $seed) {
  mt_srand($seed);
  $v = [];
  for ($i = 0; $i < 1300; $i++) {
    $v[] = mt_rand();
  }
  $out['mt'][] = ['seed' => $seed, 'values' => $v];
}

// mt_rand($min, $max): small ranges, powers of two, ranges with frequent rejections, 64-bit ranges
$ranges = [[0, 0], [0, 1], [1, 6], [-5, 5], [0, 7], [0, 255], [3, 1002], [0, 0x7FFFFFFF], [0, 0xFFFFFFFF],
  [0, 3000000000], [0, 2147483648], [-2147483648, 2147483647], [0, 0x100000000], [0, 1 << 40], [0, 3 << 61],
  [0, PHP_INT_MAX], [PHP_INT_MIN, PHP_INT_MAX], [PHP_INT_MIN, 0], [-1000, 1 << 33]];
foreach ($ranges as $k => [$min, $max]) {
  $seed = 7000 + $k;
  mt_srand($seed);
  $v = [];
  for ($i = 0; $i < 300; $i++) {
    $v[] = mt_rand($min, $max);
  }
  $out['range'][] = ['seed' => $seed, 'min' => $min, 'max' => $max, 'values' => $v];
}

// shuffle
$sizes = array_merge(range(0, 70), [100, 257, 1000]);
foreach ($sizes as $k => $n) {
  $seed = 9000 + $k;
  mt_srand($seed);
  $a = $n ? range(0, $n - 1) : [];
  shuffle($a);
  // the draws after it: the shuffle used exactly as many values
  $out['shuffle'][] = ['seed' => $seed, 'n' => $n, 'out' => $a, 'next' => mt_rand()];
}

// array_rand, on arrays of several shapes
function pick(array $arr, int $num)
{
  $r = array_rand($arr, $num);
  return ['picked' => is_array($r) ? $r : [$r], 'next' => mt_rand()];
}
$intKeySets = [[0], [1], [3], [7], [8], [100], [0, 1], [1, 2], [3, 5], [3, 4], [6, 7], [1, 7], [2, 9], [12, 13],
  [5, 9, 15], [1, 2, 3], [2, 4, 6, 7], [1, 3, 5, 7, 9, 11, 13, 15, 17], [0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 30],
  [9, 3], [5, 1, 2], [40, 12, 7, 3], [101, 102, 103], [150, 3, 12], [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]];
mt_srand(123);
for ($i = 0; $i < 60; $i++) {
  // random subsets of 0..m, ascending or shuffled
  $m = mt_rand(1, 40);
  $keys = [];
  for ($k = 0; $k <= $m; $k++) {
    if (mt_rand(0, 2) == 0) {
      $keys[] = $k;
    }
  }
  if (!$keys) {
    $keys = [$m];
  }
  if ($i % 3 == 2) {
    shuffle($keys);
  }
  $intKeySets[] = $keys;
}
$case = 0;
foreach ($intKeySets as $keys) {
  $src = [];
  foreach ($keys as $k) {
    $src[$k] = "v$k";
  }
  foreach (array_unique([1, 2, count($keys) - 1, count($keys)]) as $num) {
    if ($num < 1 || $num > count($keys)) {
      continue;
    }
    foreach (['insert', 'filter', 'collection'] as $via) {
      $seed = 11000 + $case++;
      $arr = match ($via) {
        'insert' => $src,
        'filter' => array_filter($src, fn($v) => true),
        // Collection::rand: an ArrayObject over an array_filter result, copied
        'collection' => (new ArrayObject(array_filter($src, fn($v) => true)))->getArrayCopy(),
      };
      mt_srand($seed);
      $out['rand'][] = ['seed' => $seed, 'keys' => $keys, 'num' => $num, 'via' => $via] + pick($arr, $num);
    }
  }
}
// Lists and string keys (no holes)
foreach ([1, 2, 3, 5, 8, 13, 40, 200] as $n) {
  foreach (array_unique([1, 2, intdiv($n, 2), $n - 1, $n]) as $num) {
    if ($num < 1 || $num > $n) {
      continue;
    }
    $seed = 11000 + $case++;
    mt_srand($seed);
    $out['rand'][] = ['seed' => $seed, 'keys' => range(0, $n - 1), 'num' => $num, 'via' => 'list'] + pick(range(0, $n - 1), $num);
    $seed = 11000 + $case++;
    $assoc = [];
    for ($k = 0; $k < $n; $k++) {
      $assoc["k$k"] = $k;
    }
    mt_srand($seed);
    $out['rand'][] = ['seed' => $seed, 'keys' => array_keys($assoc), 'num' => $num, 'via' => 'assoc'] + pick($assoc, $num);
  }
}

echo json_encode($out);
