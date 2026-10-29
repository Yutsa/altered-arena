//! PHP 8.2's Mersenne Twister and the functions drawing from it, bit for bit: `mt_srand`,
//! `mt_rand()`, `mt_rand($min, $max)` (ext/random: engine_mt19937.c, random.c), `shuffle`
//! (`php_array_data_shuffle`) and `array_rand` (`php_array_pick_random`), ext/standard/array.c.
//!
//! The arena seeds the generator at every request that changes the game (Arena::seedRng), so the
//! same position gives the same draws in both engines. Checked against PHP in tests/php_rand.rs.

use serde::{Deserialize, Serialize};

const N: usize = 624;
const M: usize = 397;

/// The seed of an act: game id and number of acts before it (server/src/Arena.php `rngSeed`)
pub fn seed_of(game: i64, acts: i64) -> u32 {
    (game.wrapping_mul(1_000_003).wrapping_add(acts) & 0xFFFF_FFFF) as u32
}

/// The MT19937 state of `php_random_status_state_mt19937` (mode MT_RAND_MT19937)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhpMt {
    state: Vec<u32>,
    count: usize,
}

#[inline]
fn twist(m: u32, u: u32, v: u32) -> u32 {
    let mix = (u & 0x8000_0000) | (v & 0x7FFF_FFFF);
    m ^ (mix >> 1) ^ ((v & 1).wrapping_neg() & 0x9908_b0df)
}

impl PhpMt {
    /// `mt_srand($seed)` (`mt19937_seed_state`, then a reload)
    pub fn new(seed: u32) -> PhpMt {
        let mut state = vec![0u32; N];
        state[0] = seed;
        for i in 1..N {
            let prev = state[i - 1];
            state[i] = 1_812_433_253u32.wrapping_mul(prev ^ (prev >> 30)).wrapping_add(i as u32);
        }
        let mut mt = PhpMt { state, count: N };
        mt.reload();
        mt
    }

    /// `mt19937_reload`
    fn reload(&mut self) {
        let s = &mut self.state;
        for i in 0..N - M {
            s[i] = twist(s[i + M], s[i], s[i + 1]);
        }
        for i in N - M..N - 1 {
            s[i] = twist(s[i + M - N], s[i], s[i + 1]);
        }
        s[N - 1] = twist(s[M - 1], s[N - 1], s[0]);
        self.count = 0;
    }

    /// `php_mt_rand()`: the next 32-bit output
    pub fn next_u32(&mut self) -> u32 {
        if self.count >= N {
            self.reload();
        }
        let mut s1 = self.state[self.count];
        self.count += 1;
        s1 ^= s1 >> 11;
        s1 ^= (s1 << 7) & 0x9d2c_5680;
        s1 ^= (s1 << 15) & 0xefc6_0000;
        s1 ^ (s1 >> 18)
    }

    /// `mt_rand()` without arguments
    pub fn mt_rand(&mut self) -> i64 {
        (self.next_u32() >> 1) as i64
    }

    /// `php_random_range32`: uniform in 0..=umax, rejection sampling against modulo bias
    fn range32(&mut self, umax: u32) -> u32 {
        let mut result = self.next_u32();
        if umax == u32::MAX {
            return result;
        }
        let umax = umax + 1;
        if umax & (umax - 1) == 0 {
            return result & (umax - 1);
        }
        let limit = u32::MAX - (u32::MAX % umax) - 1;
        while result > limit {
            result = self.next_u32();
        }
        result % umax
    }

    /// `php_random_range64`: two outputs per value, the first in the low half
    fn range64(&mut self, umax: u64) -> u64 {
        let next = |mt: &mut PhpMt| (mt.next_u32() as u64) | ((mt.next_u32() as u64) << 32);
        let mut result = next(self);
        if umax == u64::MAX {
            return result;
        }
        let umax = umax + 1;
        if umax & (umax - 1) == 0 {
            return result & (umax - 1);
        }
        let limit = u64::MAX - (u64::MAX % umax) - 1;
        while result > limit {
            result = next(self);
        }
        result % umax
    }

    /// `mt_rand($min, $max)` = `php_mt_rand_range` (also what shuffle and array_rand draw with).
    /// `$max < $min` is a ValueError in PHP: the caller must not ask for it.
    pub fn range(&mut self, min: i64, max: i64) -> i64 {
        let umax = (max as u64).wrapping_sub(min as u64);
        let r = if umax > u32::MAX as u64 { self.range64(umax) } else { self.range32(umax as u32) as u64 };
        (min as u64).wrapping_add(r) as i64
    }

    /// `shuffle($a)` (`php_array_data_shuffle`): the values, in iteration order, are shuffled in
    /// place (keys and holes don't matter: the array is renumbered)
    pub fn shuffle<T>(&mut self, a: &mut [T]) {
        let n = a.len();
        if n < 1 {
            return;
        }
        let mut n_left = n - 1;
        while n_left > 0 {
            let j = self.range(0, n_left as i64) as usize;
            if j != n_left {
                a.swap(n_left, j);
            }
            n_left -= 1;
        }
    }

    /// `array_rand($a, $num_req)` (`php_array_pick_random`) on an array of this shape: the
    /// positions (in iteration order) of the keys returned, ascending. Errors (empty array,
    /// `$num_req` out of range) are the caller's to avoid.
    pub fn array_rand(&mut self, shape: &PhpArrayShape, num_req: usize) -> Vec<usize> {
        let num_avail = shape.slots.len();
        assert!(num_avail > 0 && num_req >= 1 && num_req <= num_avail, "array_rand: bad arguments");
        if num_req == 1 {
            let used = shape.used;
            if (num_avail as u64) < used - (used >> 1) {
                // Less than half of the slots used: a position among the elements
                return vec![self.range(0, num_avail as i64 - 1) as usize];
            }
            // Random slots until one is not a hole
            loop {
                let slot = self.range(0, used as i64 - 1) as u64;
                if let Some(i) = shape.position_of_slot(slot) {
                    return vec![i];
                }
            }
        }
        let (negative, mut i) = if num_req > num_avail >> 1 { (true, num_avail - num_req) } else { (false, num_req) };
        let mut bitset = vec![false; num_avail];
        while i > 0 {
            let r = self.range(0, num_avail as i64 - 1) as usize;
            if !bitset[r] {
                bitset[r] = true;
                i -= 1;
            }
        }
        (0..num_avail).filter(|&k| bitset[k] ^ negative).collect()
    }
}

/// What array_rand sees of a PHP array: packed or hash, `nNumUsed`, and the slot of each element
/// in iteration order (the other slots up to `nNumUsed` are holes)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhpArrayShape {
    pub packed: bool,
    pub used: u64,
    pub slots: Vec<u64>,
}

impl PhpArrayShape {
    /// An array without holes (a list, or any array built by appending distinct keys that ended
    /// up as a hash): n elements in slots 0..n
    pub fn compact(n: usize) -> PhpArrayShape {
        PhpArrayShape { packed: false, used: n as u64, slots: (0..n as u64).collect() }
    }

    /// A new array (`[]`, `array_filter`, `$a[$k] = $v`...) after inserting these distinct integer
    /// keys in this order (`_zend_hash_index_add_or_update_i`). A packed array keeps its holes:
    /// `$a = []; $a[3] = x;` is packed with nNumUsed 4 and its element in slot 3. A copy
    /// (`zend_array_dup`, `ArrayObject::getArrayCopy`) keeps that shape: a hash has no holes.
    pub fn from_int_keys(keys: &[i64]) -> PhpArrayShape {
        enum St {
            Uninit,
            Packed { size: u64, used: u64 },
            Hash,
        }
        let mut st = St::Uninit;
        let mut count = 0u64;
        for &k in keys {
            let h = k as u64;
            st = match st {
                St::Uninit if h < 8 => St::Packed { size: 8, used: h + 1 },
                St::Uninit => St::Hash,
                // A key below nNumUsed falls in a hole: "we have to keep the order"
                St::Packed { used, .. } if h < used => St::Hash,
                St::Packed { size, .. } if h < size => St::Packed { size, used: h + 1 },
                St::Packed { size, .. } if (h >> 1) < size && (size >> 1) < count => St::Packed { size: size * 2, used: h + 1 },
                St::Packed { .. } | St::Hash => St::Hash,
            };
            count += 1;
        }
        match st {
            St::Packed { used, .. } => PhpArrayShape { packed: true, used, slots: keys.iter().map(|&k| k as u64).collect() },
            _ => PhpArrayShape::compact(keys.len()),
        }
    }

    fn position_of_slot(&self, slot: u64) -> Option<usize> {
        if self.packed {
            self.slots.binary_search(&slot).ok()
        } else {
            (slot < self.slots.len() as u64).then_some(slot as usize)
        }
    }
}
