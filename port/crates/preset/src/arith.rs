// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The float expressions of `LoadSave::updateFromOldVersion`
//! (`vital/src/common/load_save.cpp`) whose result depends on how they are
//! compiled.
//!
//! The reference is built like upstream's release builds, with
//! `-ffast-math`, and GCC rewrites these expressions (read from the
//! disassembly of `ref-preset`). Saved preset state is compared with the
//! reference byte for byte, so by default each function computes what the
//! reference build computes. With the `source-formulas` feature each one
//! computes the expression as written in the C++ source instead. Every
//! mirrored expression is listed in `migration/LEDGER.md` (M2 entry).

/// `std::pow(x, 1.0f / 1.5f)` (load_save.cpp:327-329, the 0.2.1 envelope
/// curve). GCC emits `cbrtf(x)` squared.
pub(crate) fn pow_two_thirds(x: f32) -> f32 {
    if cfg!(feature = "source-formulas") {
        x.powf(1.0 / 1.5)
    } else {
        let root = x.cbrt();
        root * root
    }
}

/// `std::pow(x, 3.0f / 4.0f)` (load_save.cpp:358-360, the 0.2.5 envelope
/// curve). GCC emits `sqrtf(x) * sqrtf(sqrtf(x))`.
pub(crate) fn pow_three_quarters(x: f32) -> f32 {
    if cfg!(feature = "source-formulas") {
        x.powf(3.0 / 4.0)
    } else {
        let root = x.sqrt();
        root * root.sqrt()
    }
}

/// `std::pow(x, 1.0f / 2.0f)` (load_save.cpp:606, 627-628, 640 and their
/// osc 2 copies, the 0.4.7 FM curve). GCC emits `sqrtss`.
pub(crate) fn pow_half(x: f32) -> f32 {
    if cfg!(feature = "source-formulas") {
        x.powf(1.0 / 2.0)
    } else {
        x.sqrt()
    }
}

/// `sqrtf(adjustment * level * level)` (load_save.cpp:382-390, the 0.2.7
/// level curve). GCC reassociates the product to
/// `(level * level) * adjustment`.
pub(crate) fn adjusted_level(level: f32) -> f32 {
    #[allow(clippy::excessive_precision, clippy::approx_constant)]
    const ADJUSTMENT: f32 = 0.70710678119;
    if cfg!(feature = "source-formulas") {
        (ADJUSTMENT * level * level).sqrt()
    } else {
        (level * level * ADJUSTMENT).sqrt()
    }
}

/// The 0.4.7 FM remap curve (load_save.cpp:635-643 and 681-689): point `i`
/// of `resolution` is `(t, y)` with `t = i / (resolution - 1.0f)`,
/// `old = interpolate(min, max, t)` and
/// `y = 1 - (pow(old, 1/2) - min_target) / new_amount`. GCC hoists
/// `1 / new_amount` out of the loop, turns both divisions into reciprocal
/// multiplies, and computes `y` as `(min_target - sqrt(old)) * (1 / new_amount) + 1`.
pub(crate) fn fm_remap_points(
    resolution: usize,
    min: f32,
    max: f32,
    min_target: f32,
    new_amount: f32,
) -> Vec<(f32, f32)> {
    let last = resolution as f32 - 1.0;
    (0..resolution)
        .map(|i| {
            if cfg!(feature = "source-formulas") {
                let t = i as f32 / last;
                let old_value = sas_common::scalar::interpolate(min, max, t);
                (t, 1.0 - (old_value.powf(1.0 / 2.0) - min_target) / new_amount)
            } else {
                let t = i as f32 * (1.0 / last);
                let old_value = t * (max - min) + min;
                (t, (min_target - old_value.sqrt()) * (1.0 / new_amount) + 1.0)
            }
        })
        .collect()
}

/// `sub_waveform * 257.0f / 6.0f` (load_save.cpp:713, the 0.5.0 step).
/// GCC folds the constants into `sub_waveform * (257.0f / 6.0f)`.
pub(crate) fn sub_wave_frame(sub_waveform: f32) -> f32 {
    if cfg!(feature = "source-formulas") {
        sub_waveform * 257.0 / 6.0
    } else {
        sub_waveform * (257.0f32 / 6.0)
    }
}
