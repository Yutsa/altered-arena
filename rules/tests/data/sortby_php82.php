<?php
error_reporting(E_ALL & ~E_DEPRECATED);
mt_srand(42);
$alpha = ['character', 'token', 'permanent', 'hero', 'spell'];
$cases = [];
$sizes = [];
for ($n = 1; $n <= 60; $n++) { $sizes[] = $n; $sizes[] = $n; $sizes[] = $n; }
$sizes[] = 1030; $sizes[] = 2000; $sizes[] = 300;
foreach ($sizes as $n) {
  $na = mt_rand(1, 5);
  $keys = [];
  for ($i = 0; $i < $n; $i++) { $keys[] = $alpha[mt_rand(0, $na - 1)]; }
  $arr = [];
  foreach ($keys as $i => $k) { $arr[100 + $i] = $k; }
  uasort($arr, function ($a, $b) { return strcmp($a, $b) >= 0; });
  $cases[] = ['keys' => $keys, 'order' => array_keys($arr)];
}
echo json_encode($cases);
