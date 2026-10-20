//! The engine's random number generator: `vital::utils::RandomGenerator`
//! (`vital/src/synthesis/framework/utils.h`, Copyright 2013-2019 Matt Tytel,
//! GPL-3.0-or-later), a `std::mt19937` drawn through libstdc++'s
//! `std::uniform_real_distribution<float>`.
//!
//! The distribution matches libstdc++ (GCC 13) exactly: `generate_canonical<float, 24>`
//! takes one 32-bit draw, converts it to `float` (round to nearest), divides
//! by 2^32, and maps a result of 1.0 to the largest float below 1. The value
//! is then `canonical * (max - min) + min` in `float`.
//!
//! Upstream seeds each generator from a global counter
//! (`RandomGenerator::next_seed_`) in construction order. [`take_next_seed`]
//! and [`RandomGenerator::from_global_counter`] mirror it; reproducing a
//! reference stream also needs the reference's construction order (and the
//! seeds its static initialisation consumes). [`RandomGenerator::new`] takes
//! an explicit seed instead.

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

/// libstdc++ `std::generate_canonical<float, 24>(mt19937&)`.
#[inline]
pub fn generate_canonical_f32(engine: &mut Mt19937) -> f32 {
    const TWO_POW_32: f32 = 4_294_967_296.0;
    let sum = engine.next_u32() as f32;
    let ret = sum / TWO_POW_32;
    if ret >= 1.0 {
        // std::nextafter(1.0f, 0.0f)
        f32::from_bits(1.0f32.to_bits() - 1)
    } else {
        ret
    }
}

static NEXT_SEED: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Takes the next value of the global seed counter
/// (`RandomGenerator::next_seed_++`).
pub fn take_next_seed() -> i32 {
    NEXT_SEED.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    /// A generator returning values in `[min, max)`, seeded with `seed`
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

    #[test]
    fn canonical_never_reaches_one() {
        assert!(f32::from_bits(1.0f32.to_bits() - 1) < 1.0);
        let mut generator = RandomGenerator::new(-1.0, 1.0, 0);
        for _ in 0..10_000 {
            let v = generator.next();
            assert!((-1.0..1.0).contains(&v));
        }
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
