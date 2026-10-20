// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `vital::constants` values the framework itself uses.
//!
//! Copied from `vital/src/common/synth_constants.h` (the rest of that file
//! belongs to the parameter registry, `sas-preset`).

use sas_simd::{PolyFloat, PolyMask};

/// `kMaxModulationConnections` (initial router queue capacity upstream).
pub const MAX_MODULATION_CONNECTIONS: usize = 64;

/// `constants::kNumSyncedFrequencyRatios`.
pub const NUM_SYNCED_FREQUENCY_RATIOS: usize = 13;

/// `constants::kSyncedFrequencyRatios`: tempo-sync multipliers of the beat
/// frequency.
pub const SYNCED_FREQUENCY_RATIOS: [f32; NUM_SYNCED_FREQUENCY_RATIOS] = [
    0.0,
    1.0 / 128.0,
    1.0 / 64.0,
    1.0 / 32.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 2.0,
    1.0,
    2.0,
    4.0,
    8.0,
    16.0,
];

/// `constants::kLeftOne`: `(1, 0, 1, 0)`.
pub fn left_one() -> PolyFloat {
    PolyFloat::stereo(1.0, 0.0)
}

/// `constants::kRightOne`: `(0, 1, 0, 1)`.
pub fn right_one() -> PolyFloat {
    PolyFloat::stereo(0.0, 1.0)
}

/// `constants::kFirstVoiceOne`: `(1, 1, 0, 0)`.
pub fn first_voice_one() -> PolyFloat {
    PolyFloat::new(1.0, 1.0, 0.0, 0.0)
}

/// `constants::kSecondVoiceOne`: `(0, 0, 1, 1)`.
pub fn second_voice_one() -> PolyFloat {
    PolyFloat::new(0.0, 0.0, 1.0, 1.0)
}

/// `constants::kFullMask`.
pub fn full_mask() -> PolyMask {
    PolyMask::full()
}

/// `constants::kLeftMask`: lanes 0 and 2.
pub fn left_mask() -> PolyMask {
    PolyFloat::equal(left_one(), PolyFloat::splat(1.0))
}

/// `constants::kRightMask`: lanes 1 and 3.
pub fn right_mask() -> PolyMask {
    PolyFloat::equal(right_one(), PolyFloat::splat(1.0))
}

/// `constants::kFirstMask`: voice 0 (lanes 0 and 1).
pub fn first_mask() -> PolyMask {
    PolyFloat::equal(first_voice_one(), PolyFloat::splat(1.0))
}

/// `constants::kSecondMask`: voice 1 (lanes 2 and 3).
pub fn second_mask() -> PolyMask {
    PolyFloat::equal(second_voice_one(), PolyFloat::splat(1.0))
}
