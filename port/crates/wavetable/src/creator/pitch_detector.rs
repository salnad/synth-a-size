// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ports `vital/src/common/wavetable/pitch_detector.{h,cpp}` (Copyright
//! 2013-2019 Matt Tytel, GPL-3.0-or-later): finds the period of a pitched
//! recording by comparing consecutive cycles.

use sas_common::scalar;

/// `PitchDetector`.
#[derive(Clone, Debug, Default)]
pub struct PitchDetector {
    signal: Vec<f32>,
}

impl PitchDetector {
    /// `kNumPoints`.
    pub const NUM_POINTS: i32 = 2520;

    pub fn new() -> Self {
        Self::default()
    }

    /// `loadSignal(signal, size)`.
    pub fn load_signal(&mut self, signal: &[f32]) {
        self.signal = signal.to_vec();
    }

    fn sample(&self, index: i32) -> f32 {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.signal.get(i))
            .copied()
            .unwrap_or(0.0)
    }

    /// `getPeriodError(period)`.
    pub fn period_error(&self, period: f32) -> f32 {
        const DC_DELTA_ERROR_MULTIPLIER: f32 = 0.015;
        let size = self.signal.len() as i32;
        let mut error = 0.0f32;
        let waves = (size as f32 / period - 1.0) as i32;
        if waves <= 0 {
            return error;
        }
        let points = Self::NUM_POINTS / waves;
        for w in 0..waves {
            let mut total_from = 0.0f32;
            let mut total_to = 0.0f32;
            for i in 0..points {
                let first_position = w as f32 * period + i as f32 * period / points as f32;
                let second_position = (w + 1) as f32 * period + i as f32 * period / points as f32;

                let first_index = first_position as i32;
                let first_t = first_position - first_index as f32;
                let first_value = scalar::interpolate(
                    self.sample(first_index),
                    self.sample(first_index + 1),
                    first_t,
                );
                total_from += first_value;

                let second_index = second_position as i32;
                let second_t = second_position - second_index as f32;
                let second_value = scalar::interpolate(
                    self.sample(second_index),
                    self.sample(second_index + 1),
                    second_t,
                );
                total_to += second_value;

                let delta = first_value - second_value;
                error += delta * delta;
            }

            let total_diff = total_from - total_to;
            error += total_diff * total_diff * DC_DELTA_ERROR_MULTIPLIER;
        }
        error
    }

    /// `findYinPeriod(max_period)`: the best whole-sample period from 300
    /// up, refined in steps of 0.1 around it.
    pub fn find_yin_period(&self, max_period: i32) -> f32 {
        const MIN_LENGTH: f32 = 300.0;
        let size = self.signal.len() as i32;
        let max_length = (size as f32 / 2.0).min(max_period as f32);

        let mut best_error = i32::MAX as f32;
        let mut matched = MIN_LENGTH;
        let mut length = MIN_LENGTH;
        while length < max_length {
            let error = self.period_error(length);
            if error < best_error {
                best_error = error;
                matched = length;
            }
            length += 1.0;
        }

        let mut best_match = matched;
        let mut length = matched - 1.0;
        while length <= matched + 1.0 {
            let error = self.period_error(length);
            if error < best_error {
                best_error = error;
                best_match = length;
            }
            length += 0.1;
        }
        best_match
    }

    /// `matchPeriod(max_period)`.
    pub fn match_period(&self, max_period: i32) -> f32 {
        self.find_yin_period(max_period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_period_of_a_saw() {
        let period = 523.0f32;
        let signal: Vec<f32> = (0..8096)
            .map(|i| (i as f32 % period) / period * 2.0 - 1.0)
            .collect();
        let mut detector = PitchDetector::new();
        detector.load_signal(&signal);
        let found = detector.match_period(2048);
        assert!((found - period).abs() < 0.2, "{found}");
    }
}
