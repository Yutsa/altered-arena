//! Helpers/Collection.php on collections of pieces: a collection is the ordered list of the ids
//! it holds (the PHP Collection is keyed by id)

use serde_json::{json, Value};

/// `$a->merge($b)`: `$a + $b` (ids of `$b` already in `$a` are dropped)
pub fn merge(a: &[i64], b: &[i64]) -> Vec<i64> {
    let mut out = a.to_vec();
    for x in b {
        if !out.contains(x) {
            out.push(*x);
        }
    }
    out
}

/// `$c->limit($n)`
pub fn limit(a: &[i64], n: usize) -> Vec<i64> {
    a.iter().take(n).copied().collect()
}

/// `$c->first()`
pub fn first(a: &[i64]) -> Option<i64> {
    a.first().copied()
}

/// `$c->getIds()` as a PHP list
pub fn ids(a: &[i64]) -> Value {
    json!(a)
}

/// `$c->includes($t)` / `$c->has($id)`
pub fn has(a: &[i64], id: i64) -> bool {
    a.contains(&id)
}

/// `$c->sortBy($field)` (ascending): `uasort` with a comparator returning `strcmp(a, b) >= 0`,
/// a bool. PHP 8 turns a `true` into 1 and retries a `false` with swapped operands (-1 if that
/// call is true, else 0, then the original positions), so equal keys compare as 1 both ways and
/// their final order is whatever `zend_sort` makes of it. `keys[i]` is the field of `ids[i]`.
// PHP: Helpers/Collection.php:185, ext/standard/array.c php_array_user_compare
pub fn sort_by(ids: &[i64], keys: &[String]) -> Vec<i64> {
    let f = |a: &str, b: &str| a.as_bytes().cmp(b.as_bytes()) != std::cmp::Ordering::Less;
    let mut items: Vec<(usize, i64, &str)> = ids.iter().enumerate().map(|(i, id)| (i, *id, keys[i].as_str())).collect();
    let mut cmp = |a: &(usize, i64, &str), b: &(usize, i64, &str)| -> i32 {
        let r = if f(a.2, b.2) {
            1
        } else if f(b.2, a.2) {
            -1
        } else {
            0
        };
        if r != 0 {
            return r;
        }
        // stable_sort_fallback
        match a.0.cmp(&b.0) {
            std::cmp::Ordering::Greater => 1,
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
        }
    };
    zend_sort(&mut items, &mut cmp);
    items.into_iter().map(|x| x.1).collect()
}

/// `zend_sort` (Zend/zend_sort.c, PHP 8.2): hybrid insertion sort / quick sort, reproduced step
/// by step since an inconsistent comparator makes the result depend on the algorithm
pub fn zend_sort<T>(v: &mut [T], cmp: &mut dyn FnMut(&T, &T) -> i32) {
    let n = v.len();
    zs_sort(v, 0, n, cmp);
}

type Cmp<'a, T> = &'a mut dyn FnMut(&T, &T) -> i32;

fn zs_c<T>(v: &[T], a: usize, b: usize, cmp: Cmp<T>) -> i32 {
    cmp(&v[a], &v[b])
}

fn zs_sort2<T>(v: &mut [T], a: usize, b: usize, cmp: Cmp<T>) {
    if zs_c(v, a, b, cmp) > 0 {
        v.swap(a, b);
    }
}

fn zs_sort3<T>(v: &mut [T], a: usize, b: usize, c: usize, cmp: Cmp<T>) {
    if zs_c(v, a, b, cmp) <= 0 {
        if zs_c(v, b, c, cmp) <= 0 {
            return;
        }
        v.swap(b, c);
        if zs_c(v, a, b, cmp) > 0 {
            v.swap(a, b);
        }
        return;
    }
    if zs_c(v, c, b, cmp) <= 0 {
        v.swap(a, c);
        return;
    }
    v.swap(a, b);
    if zs_c(v, b, c, cmp) > 0 {
        v.swap(b, c);
    }
}

fn zs_sort4<T>(v: &mut [T], a: usize, b: usize, c: usize, d: usize, cmp: Cmp<T>) {
    zs_sort3(v, a, b, c, cmp);
    if zs_c(v, c, d, cmp) > 0 {
        v.swap(c, d);
        if zs_c(v, b, c, cmp) > 0 {
            v.swap(b, c);
            if zs_c(v, a, b, cmp) > 0 {
                v.swap(a, b);
            }
        }
    }
}

fn zs_sort5<T>(v: &mut [T], a: usize, b: usize, c: usize, d: usize, e: usize, cmp: Cmp<T>) {
    zs_sort4(v, a, b, c, d, cmp);
    if zs_c(v, d, e, cmp) > 0 {
        v.swap(d, e);
        if zs_c(v, c, d, cmp) > 0 {
            v.swap(c, d);
            if zs_c(v, b, c, cmp) > 0 {
                v.swap(b, c);
                if zs_c(v, a, b, cmp) > 0 {
                    v.swap(a, b);
                }
            }
        }
    }
}

/// `for (k = i; k > j; k -= siz) swp(k, k - siz);`
fn zs_shift<T>(v: &mut [T], i: usize, j: usize) {
    let mut k = i;
    while k > j {
        v.swap(k, k - 1);
        k -= 1;
    }
}

/// `zend_insert_sort`
fn zs_insert_sort<T>(v: &mut [T], base: usize, n: usize, cmp: Cmp<T>) {
    match n {
        0 | 1 => {}
        2 => zs_sort2(v, base, base + 1, cmp),
        3 => zs_sort3(v, base, base + 1, base + 2, cmp),
        4 => zs_sort4(v, base, base + 1, base + 2, base + 3, cmp),
        5 => zs_sort5(v, base, base + 1, base + 2, base + 3, base + 4, cmp),
        _ => {
            let start = base;
            let end = base + n;
            let sentry = start + 6;
            for i in start + 1..sentry {
                let mut j = i - 1;
                if zs_c(v, j, i, cmp) <= 0 {
                    continue;
                }
                while j != start {
                    j -= 1;
                    if zs_c(v, j, i, cmp) <= 0 {
                        j += 1;
                        break;
                    }
                }
                zs_shift(v, i, j);
            }
            for i in sentry..end {
                let mut j = i - 1;
                if zs_c(v, j, i, cmp) <= 0 {
                    continue;
                }
                loop {
                    j -= 2;
                    if zs_c(v, j, i, cmp) <= 0 {
                        j += 1;
                        if zs_c(v, j, i, cmp) <= 0 {
                            j += 1;
                        }
                        break;
                    }
                    if j == start {
                        break;
                    }
                    if j == start + 1 {
                        j -= 1;
                        if zs_c(v, i, j, cmp) > 0 {
                            j += 1;
                        }
                        break;
                    }
                }
                zs_shift(v, i, j);
            }
        }
    }
}

/// `zend_sort` on `v[base..base + n]`
fn zs_sort<T>(v: &mut [T], base: usize, n: usize, cmp: Cmp<T>) {
    let mut base = base;
    let mut n = n;
    loop {
        if n <= 16 {
            zs_insert_sort(v, base, n, cmp);
            return;
        }
        let start = base;
        let end = start + n;
        let offset = n >> 1;
        let pivot0 = start + offset;
        if (n >> 10) != 0 {
            let delta = offset >> 1;
            zs_sort5(v, start, start + delta, pivot0, pivot0 + delta, end - 1, cmp);
        } else {
            zs_sort3(v, start, pivot0, end - 1, cmp);
        }
        v.swap(start + 1, pivot0);
        let pivot = start + 1;
        let mut i = pivot + 1;
        let mut j = end - 1;
        'done: loop {
            while zs_c(v, pivot, i, cmp) > 0 {
                i += 1;
                if i == j {
                    break 'done;
                }
            }
            j -= 1;
            if j == i {
                break 'done;
            }
            while zs_c(v, j, pivot, cmp) > 0 {
                j -= 1;
                if j == i {
                    break 'done;
                }
            }
            v.swap(i, j);
            i += 1;
            if i == j {
                break 'done;
            }
        }
        v.swap(pivot, i - 1);
        if (i - 1) - start < end - i {
            zs_sort(v, start, (i - start) - 1, cmp);
            base = i;
            n = end - i;
        } else {
            zs_sort(v, i, end - i, cmp);
            n = (i - start) - 1;
        }
    }
}
