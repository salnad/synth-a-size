// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The engine's random number generator: `vital::utils::RandomGenerator`
//! (`vital/src/synthesis/framework/utils.h`, Copyright 2013-2019 Matt Tytel,
//! GPL-3.0-or-later), a `std::mt19937` drawn through libstdc++'s
//! `std::uniform_real_distribution<float>`.
//!
//! The distribution matches the reference's C++ library exactly.
//! `generate_canonical<float, 24>` takes one 32-bit draw, converts it to
//! `float` (round to nearest) and divides by 2^32. The value is then
//! `canonical * (max - min) + min` in `float`. The libraries differ only for
//! draws u >= 2^32 - 128, which round to 2^32:
//!
//! - libstdc++ (GCC 13, the x86_64 Linux reference) maps the resulting 1.0 to
//!   the largest float below 1, so values stay below `max`;
//! - libc++ (the macOS arm64 reference) returns 1.0, so the value is `max`.
//!
//! A value of exactly `max` can change downstream integer or index results,
//! so both are ported. The libc++ column is used on aarch64, and on other
//! targets with the `aarch64-semantics` feature (the switch `sas-simd` uses;
//! see [`crate::AARCH64_SEMANTICS`] and `migration/decisions/reference-arm.md`).
//!
//! Upstream seeds each generator from a global counter
//! (`RandomGenerator::next_seed_`) in construction order. [`take_next_seed`]
//! and [`RandomGenerator::from_global_counter`] mirror it; reproducing a
//! reference stream also needs the reference's construction order (and the
//! seeds its static initialisation consumes). [`RandomGenerator::new`] takes
//! an explicit seed instead.
//!
//! A thread can opt into a counter of its own ([`use_thread_seed_counter`]):
//! the LLM layer's preview engine builds a second synth on its own thread,
//! and must not move the process's counter, which would change the seeds of
//! every instance and wavetable built after it. Threads that don't opt in
//! use the global counter exactly as before.

/// `std::mt19937`.
#[derive(Clone)]
pub struct Mt19937 {
    state: [u32; Self::N],
    index: usize,
}

impl std::fmt::Debug for Mt19937 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mt19937")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl Mt19937 {
    const N: usize = 624;
    const M: usize = 397;
    const MATRIX_A: u32 = 0x9908_b0df;
    const UPPER_MASK: u32 = 0x8000_0000;
    const LOWER_MASK: u32 = 0x7fff_ffff;

    /// The default seed of `std::mt19937`.
    pub const DEFAULT_SEED: u32 = 5489;

    pub fn new(seed: u32) -> Self {
        let mut engine = Self {
            state: [0; Self::N],
            index: Self::N,
        };
        engine.seed(seed);
        engine
    }

    /// `engine.seed(value)`.
    pub fn seed(&mut self, seed: u32) {
        self.state[0] = seed;
        for i in 1..Self::N {
            let prev = self.state[i - 1];
            self.state[i] = 1_812_433_253u32
                .wrapping_mul(prev ^ (prev >> 30))
                .wrapping_add(i as u32);
        }
        self.index = Self::N;
    }

    fn twist(&mut self) {
        for i in 0..Self::N {
            let y = (self.state[i] & Self::UPPER_MASK)
                | (self.state[(i + 1) % Self::N] & Self::LOWER_MASK);
            let mut next = self.state[(i + Self::M) % Self::N] ^ (y >> 1);
            if y & 1 != 0 {
                next ^= Self::MATRIX_A;
            }
            self.state[i] = next;
        }
        self.index = 0;
    }

    /// `engine()`.
    pub fn next_u32(&mut self) -> u32 {
        if self.index >= Self::N {
            self.twist();
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `engine.discard(count)`.
    pub fn discard(&mut self, count: u64) {
        for _ in 0..count {
            self.next_u32();
        }
    }
}

const TWO_POW_32: f32 = 4_294_967_296.0;

/// libstdc++ `std::generate_canonical<float, 24>` for one mt19937 draw.
#[inline]
pub fn canonical_from_draw_libstdcxx(draw: u32) -> f32 {
    let ret = draw as f32 / TWO_POW_32;
    if ret >= 1.0 {
        // std::nextafter(1.0f, 0.0f)
        f32::from_bits(1.0f32.to_bits() - 1)
    } else {
        ret
    }
}

/// libc++ `std::generate_canonical<float, 24>` for one mt19937 draw: no
/// clamp, so draws u >= 2^32 - 128 give exactly 1.0.
#[inline]
pub fn canonical_from_draw_libcxx(draw: u32) -> f32 {
    draw as f32 / TWO_POW_32
}

/// `std::generate_canonical<float, 24>` for one draw, in this build's column
/// ([`crate::AARCH64_SEMANTICS`]).
#[inline]
pub fn canonical_from_draw(draw: u32) -> f32 {
    if crate::AARCH64_SEMANTICS {
        canonical_from_draw_libcxx(draw)
    } else {
        canonical_from_draw_libstdcxx(draw)
    }
}

/// `std::generate_canonical<float, 24>(mt19937&)`.
#[inline]
pub fn generate_canonical_f32(engine: &mut Mt19937) -> f32 {
    canonical_from_draw(engine.next_u32())
}

static NEXT_SEED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

thread_local! {
    /// This thread's own seed counter, if it has one.
    static THREAD_SEEDS: std::cell::Cell<Option<i32>> = const { std::cell::Cell::new(None) };
}

/// Takes the next value of the seed counter
/// (`RandomGenerator::next_seed_++`): the calling thread's own, if it has
/// one ([`use_thread_seed_counter`]), else the global one.
pub fn take_next_seed() -> i32 {
    THREAD_SEEDS.with(|seeds| match seeds.get() {
        Some(seed) => {
            seeds.set(Some(seed.wrapping_add(1)));
            seed
        }
        None => NEXT_SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    })
}

/// Gives the calling thread a seed counter of its own, starting at `seed`,
/// for the rest of its life (calling it again restarts it). The global
/// counter is neither read nor changed, by this or by the thread's later
/// draws.
pub fn use_thread_seed_counter(seed: i32) {
    THREAD_SEEDS.with(|seeds| seeds.set(Some(seed)));
}

/// Sets the global seed counter (`ref-render --seed N` does this before
/// constructing the synth).
pub fn set_next_seed(seed: i32) {
    NEXT_SEED.store(seed, std::sync::atomic::Ordering::Relaxed);
}

/// `vital::utils::RandomGenerator`.
#[derive(Clone, Debug)]
pub struct RandomGenerator {
    engine: Mt19937,
    min: f32,
    max: f32,
}

impl RandomGenerator {
    /// A generator returning values in `[min, max)` (`[min, max]` in the
    /// libc++ column, see the module docs), seeded with `seed`
    /// (upstream passes `next_seed_++`, an `int`, to `std::mt19937`).
    pub fn new(min: f32, max: f32, seed: i32) -> Self {
        Self {
            engine: Mt19937::new(seed as u32),
            min,
            max,
        }
    }

    /// `RandomGenerator(min, max)`: seeded from the global counter, as
    /// upstream constructs every generator.
    pub fn from_global_counter(min: f32, max: f32) -> Self {
        Self::new(min, max, take_next_seed())
    }

    /// `next()`: one value from `uniform_real_distribution<float>(min, max)`.
    #[inline]
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f32 {
        generate_canonical_f32(&mut self.engine) * (self.max - self.min) + self.min
    }

    /// One raw `std::mt19937` draw, the `u` that [`next`](Self::next)
    /// maps into `[min, max)`.
    #[inline]
    pub fn next_draw(&mut self) -> u32 {
        self.engine.next_u32()
    }

    /// `seed(new_seed)`: reseeds the engine.
    pub fn seed(&mut self, new_seed: i32) {
        self.engine.seed(new_seed as u32);
    }

    /// `polyNext()`: four consecutive values, one per lane.
    pub fn poly_next(&mut self) -> [f32; 4] {
        [self.next(), self.next(), self.next(), self.next()]
    }

    /// `polyVoiceNext()`: two values, each written to both lanes of a voice.
    pub fn poly_voice_next(&mut self) -> [f32; 4] {
        let a = self.next();
        let b = self.next();
        [a, a, b, b]
    }

    /// `polyNext(mask)`: a value for each set lane, 0 elsewhere.
    pub fn poly_next_masked(&mut self, mask: [bool; 4]) -> [f32; 4] {
        let mut result = [0.0; 4];
        for (value, set) in result.iter_mut().zip(mask) {
            if set {
                *value = self.next();
            }
        }
        result
    }

    pub fn min(&self) -> f32 {
        self.min
    }

    pub fn max(&self) -> f32 {
        self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mt19937_reference_values() {
        let mut engine = Mt19937::new(Mt19937::DEFAULT_SEED);
        assert_eq!(engine.next_u32(), 3_499_211_612);
        assert_eq!(engine.next_u32(), 581_869_302);
        // C++11 [rand.predef]: the 10000th value of a default-constructed mt19937.
        let mut engine = Mt19937::new(Mt19937::DEFAULT_SEED);
        engine.discard(9999);
        assert_eq!(engine.next_u32(), 4_123_659_995);
    }

    const EDGE: u32 = u32::MAX - 127;
    const BELOW_ONE: f32 = 1.0 - 1.0 / 16_777_216.0;

    #[test]
    fn edge_draws_differ_between_the_columns() {
        for draw in [EDGE, EDGE + 1, u32::MAX - 1, u32::MAX] {
            assert_eq!(canonical_from_draw_libstdcxx(draw), BELOW_ONE);
            assert_eq!(canonical_from_draw_libcxx(draw), 1.0);
        }
        let draw = EDGE - 1;
        assert_eq!(canonical_from_draw_libstdcxx(draw), BELOW_ONE);
        assert_eq!(canonical_from_draw_libcxx(draw), BELOW_ONE);
    }

    #[test]
    fn edge_draw_reaches_max_only_in_the_libcxx_column() {
        let map = |canonical: f32| canonical * (1.0f32 - -1.0) + -1.0;
        assert_eq!(
            map(canonical_from_draw_libstdcxx(EDGE)),
            1.0 - 2.0 / 16_777_216.0
        );
        assert_eq!(map(canonical_from_draw_libcxx(EDGE)), 1.0);
    }

    #[test]
    fn columns_agree_below_the_edge() {
        let mut engine = Mt19937::new(1);
        for _ in 0..100_000 {
            let draw = engine.next_u32().min(EDGE - 1);
            assert_eq!(
                canonical_from_draw_libstdcxx(draw),
                canonical_from_draw_libcxx(draw)
            );
        }
        for draw in [0, 1, 1 << 31, EDGE - 1] {
            assert_eq!(
                canonical_from_draw_libstdcxx(draw),
                canonical_from_draw_libcxx(draw)
            );
        }
    }

    #[test]
    fn the_build_uses_its_column() {
        let expected = if cfg!(any(target_arch = "aarch64", feature = "aarch64-semantics")) {
            1.0
        } else {
            BELOW_ONE
        };
        assert_eq!(canonical_from_draw(EDGE), expected);
        assert_eq!(crate::AARCH64_SEMANTICS, expected == 1.0);
    }

    #[test]
    fn a_thread_counter_leaves_the_global_counter_alone() {
        // Only this test sets or draws the global counter from these
        // threads, but other tests in the binary may draw it concurrently,
        // so the check is that the thread draws its own sequence.
        let own = std::thread::spawn(|| {
            use_thread_seed_counter(0);
            let first: Vec<i32> = (0..3).map(|_| take_next_seed()).collect();
            use_thread_seed_counter(0);
            let again = take_next_seed();
            (first, again)
        })
        .join()
        .unwrap();
        assert_eq!(own, (vec![0, 1, 2], 0));
        let global = std::thread::spawn(take_next_seed).join().unwrap();
        let next = std::thread::spawn(take_next_seed).join().unwrap();
        assert!(
            next > global,
            "other threads still share the global counter"
        );
    }

    #[test]
    fn reseeding_restarts_the_stream() {
        let mut a = RandomGenerator::new(0.0, 1.0, 7);
        let first: Vec<f32> = (0..5).map(|_| a.next()).collect();
        a.seed(7);
        let again: Vec<f32> = (0..5).map(|_| a.next()).collect();
        assert_eq!(first, again);
    }
}
