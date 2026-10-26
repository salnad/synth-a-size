// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Filter response curves: upstream's filter response vertex shaders
//! (`vital/src/interface/look_and_feel/shaders.cpp`), evaluated on the CPU.
//!
//! Upstream runs these shaders with transform feedback to turn a line's x
//! positions into response heights, then draws the line. Each evaluator here
//! ports one shader line by line, in its f32 arithmetic order and with its
//! constants (`FILTER_RESPONSE_CONSTANTS`, `RESPONSE_TOOLS`). Positions and
//! results are in the component's device coordinates (-1 to 1, y up);
//! callers write them into a [`LineRenderer`](crate::primitives::line::LineRenderer)
//! and keep their own fill, boost and colours.

/// `FILTER_RESPONSE_UNIFORMS`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FilterResponseUniforms {
    pub midi_cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    pub mix: f32,
    pub db24: f32,
    pub stage0: f32,
    pub stage1: f32,
    pub stage2: f32,
    pub stage3: f32,
    pub stage4: f32,
}

// FILTER_RESPONSE_CONSTANTS.
const MIN_MIDI_NOTE: f32 = 8.0;
#[allow(clippy::approx_constant, clippy::excessive_precision)]
const PI: f32 = 3.141_592_653_59;
const MAX_MIDI_NOTE: f32 = 137.0;
const MIN_DB: f32 = -30.0;
const MAX_DB: f32 = 20.0;

/// A GLSL `vec2` used as a complex number.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Complex(f32, f32);

impl std::ops::Add for Complex {
    type Output = Complex;
    fn add(self, other: Complex) -> Complex {
        Complex(self.0 + other.0, self.1 + other.1)
    }
}

impl std::ops::Sub for Complex {
    type Output = Complex;
    fn sub(self, other: Complex) -> Complex {
        Complex(self.0 - other.0, self.1 - other.1)
    }
}

impl std::ops::Neg for Complex {
    type Output = Complex;
    fn neg(self) -> Complex {
        Complex(-self.0, -self.1)
    }
}

/// `float * vec2`.
fn scale(s: f32, v: Complex) -> Complex {
    Complex(s * v.0, s * v.1)
}

const ONE: Complex = Complex(1.0, 0.0);

// RESPONSE_TOOLS.
fn complex_multiply(a: Complex, b: Complex) -> Complex {
    Complex(a.0 * b.0 - a.1 * b.1, a.1 * b.0 + a.0 * b.1)
}

fn complex_reciprocal(num: Complex) -> Complex {
    let conjugate = Complex(num.0, -num.1);
    let denominator = complex_multiply(num, conjugate);
    Complex(conjugate.0 / denominator.0, conjugate.1 / denominator.0)
}

fn complex_divide(a: Complex, b: Complex) -> Complex {
    let conjugate = Complex(b.0, -b.1);
    let num = complex_multiply(a, conjugate);
    let den = complex_multiply(b, conjugate);
    Complex(num.0 / den.0, num.1 / den.0)
}

fn one_pole_response(cutoff: f32) -> Complex {
    complex_reciprocal(Complex(1.0, cutoff))
}

#[allow(clippy::excessive_precision)]
fn magnitude_to_db(magnitude: f32) -> f32 {
    8.685_889_638_065_037 * magnitude.ln()
}

fn get_cutoff_ratio(x: f32, midi_cutoff: f32) -> f32 {
    let percent = 0.5 * (x + 1.0);
    let midi_note = MIN_MIDI_NOTE + percent * (MAX_MIDI_NOTE - MIN_MIDI_NOTE);
    2.0f32.powf(((midi_note - midi_cutoff) / 12.0).min(8.0))
}

/// GLSL `length`.
fn length(v: Complex) -> f32 {
    (v.0 * v.0 + v.1 * v.1).sqrt()
}

/// GLSL `step(edge, x)`.
fn step(edge: f32, x: f32) -> f32 {
    if x < edge {
        0.0
    } else {
        1.0
    }
}

fn get_y_for_response(response: Complex) -> f32 {
    let magnitude_response = length(response);
    let db = magnitude_to_db(magnitude_response);
    2.0 * (db - MIN_DB) / (MAX_DB - MIN_DB) - 1.0
}

/// `kDigitalFilterResponseVertexShader` at `x`.
pub fn digital(u: &FilterResponseUniforms, x: f32) -> f32 {
    let g = get_cutoff_ratio(x, u.midi_cutoff);
    let g2 = Complex(g * g, 0.0);
    let denominator = g2 + Complex(0.0, g * u.resonance) + Complex(-1.0, 0.0);
    let numerator = scale(-u.stage0, ONE) + scale(u.stage1, Complex(0.0, g)) + scale(u.stage2, g2);
    let numerator_pre =
        scale(-u.stage3, ONE) + scale(u.stage1, Complex(0.0, g)) + scale(u.stage4, g2);
    let mut response = complex_divide(numerator, denominator);
    let pre_denominator = g2 + Complex(0.0, g) + Complex(-1.0, 0.0);
    let pre_response = complex_divide(numerator_pre, pre_denominator);
    response = response + scale(u.db24, complex_multiply(response, pre_response) - response);
    response = scale(u.drive, response);
    response = scale(u.mix, response) + Complex(1.0 - u.mix, 0.0);
    get_y_for_response(response)
}

/// `kCombFilterResponseVertexShader` at the line position (`x`, `y`): `y`
/// picks how the response rounds off past its sixth cycle. Callers feed
/// point `i` the y `(i / kCombAlternatePeriod) % 2` (period 2), as
/// `FilterResponse` and `FlangerResponse` set up their lines, so the
/// rounded-off part alternates between its two values and fills solid.
pub fn comb(u: &FilterResponseUniforms, x: f32, y: f32) -> f32 {
    const MAX_CYCLES: f32 = 6.0;
    let ratio = get_cutoff_ratio(x, u.midi_cutoff);
    let ratio_diff = get_cutoff_ratio(x + 0.02, u.midi_cutoff) - ratio;
    let max_step = step(MAX_CYCLES, ratio);
    let tick = scale(
        u.resonance,
        Complex((2.0 * PI * ratio).cos(), -(2.0 * PI * ratio).sin()),
    );
    let low_pass = one_pole_response(get_cutoff_ratio(x, u.stage2));
    let high_pass = ONE - low_pass;
    let one_pole = scale(u.stage0, low_pass) + scale(u.stage1, high_pass);
    let high_pass2 = ONE - one_pole_response(get_cutoff_ratio(x, u.stage3));
    let mut filter_input = Complex(1.0 - 0.5 * u.resonance.abs(), 0.0);
    filter_input = complex_multiply(complex_multiply(filter_input, one_pole), high_pass2);
    let mut denominator = ONE - complex_multiply(complex_multiply(tick, one_pole), high_pass2);
    let round_value = complex_multiply(one_pole, high_pass2).0 * u.resonance.abs();
    let denominator_round = Complex(1.0 - round_value, 0.0);
    let denominator_round_down = Complex(1.0 + round_value, 0.0);
    let max_step_mult = 1.0 - y;
    let max_step_denominator = scale(max_step_mult, denominator_round)
        + scale(1.0 - max_step_mult, denominator_round_down);
    denominator = scale(max_step, max_step_denominator) + scale(1.0 - max_step, denominator);
    let denominator_round_step = 1.0 - max_step.max(step(ratio_diff, length(denominator)));
    denominator = scale(denominator_round_step, denominator_round)
        + scale(1.0 - denominator_round_step, denominator);
    let mut response = complex_divide(scale(u.drive, filter_input), denominator);
    response = scale(u.mix, response) + Complex(1.0 - u.mix, 0.0);
    get_y_for_response(response)
}

/// `kPhaserFilterResponseVertexShader` at `x`.
pub fn phaser(u: &FilterResponseUniforms, x: f32) -> f32 {
    let g = get_cutoff_ratio(x, u.midi_cutoff);
    let one_pole = one_pole_response(g);
    let all_pass = ONE - scale(2.0, one_pole);
    let half_peak = complex_multiply(all_pass, all_pass);
    let peak1 = complex_multiply(half_peak, half_peak);
    let peak3 = complex_multiply(peak1, peak1);
    let peak5 = complex_multiply(peak3, peak1);
    let chain = scale(u.stage0, peak1) + scale(u.stage1, peak3) + scale(u.stage2, peak5);
    let invert_mult = 1.0 - 2.0 * u.db24;
    let mut feedback_chain = complex_multiply(chain, one_pole_response(0.05 * g));
    feedback_chain = complex_multiply(feedback_chain, ONE - one_pole_response(20.0 * g));
    let denominator = ONE - scale(invert_mult * u.resonance, feedback_chain);
    let phase_response = complex_divide(chain, denominator);
    let mut response = Complex(0.5, 0.0) + scale(0.5 * invert_mult, phase_response);
    response = scale(u.mix, response) + Complex(1.0 - u.mix, 0.0);
    get_y_for_response(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 1e-4,
            "{actual} is not {expected}"
        );
    }

    /// At x = -1 the note is kMinMidiNote; at the cutoff the ratio is 1.
    #[test]
    fn cutoff_ratios_span_the_midi_range() {
        close(get_cutoff_ratio(-1.0, 8.0), 1.0);
        close(get_cutoff_ratio(1.0, 137.0), 1.0);
        // Capped at eight octaves above the cutoff.
        close(get_cutoff_ratio(1.0, 8.0), 256.0);
    }

    /// The x position of `midi_note` on the line.
    fn x_at(midi_note: f32) -> f32 {
        2.0 * (midi_note - MIN_MIDI_NOTE) / (MAX_MIDI_NOTE - MIN_MIDI_NOTE) - 1.0
    }

    /// The y position of `db` on the line.
    fn y_at(db: f32) -> f32 {
        2.0 * (db - MIN_DB) / (MAX_DB - MIN_DB) - 1.0
    }

    /// A low pass (stage2 = 1) at its cutoff: g = 1, so the numerator is
    /// g² = 1 and the denominator g² + i·g·res − 1 = i·res, and
    /// |response| = 1 / res: 2 (+6.02 dB) at res 0.5.
    #[test]
    fn digital_low_pass_at_its_cutoff() {
        let u = FilterResponseUniforms {
            midi_cutoff: 72.5,
            resonance: 0.5,
            drive: 1.0,
            mix: 1.0,
            stage2: 1.0,
            ..Default::default()
        };
        close(digital(&u, x_at(72.5)), y_at(20.0 * 2.0f32.log10()));
        // Mix 0 is the dry signal: 0 dB everywhere.
        let dry = FilterResponseUniforms { mix: 0.0, ..u };
        close(digital(&dry, 0.3), y_at(0.0));
    }

    /// With its filters open (the low pass far above the range, the high
    /// pass at the lowest cutoff, capped eight octaves below), the comb is
    /// (1 − ½|r|) / (1 − r·e^(−2πi·ratio)). At r = 0.5 that peaks at
    /// 0.75 / 0.5 = 1.5 (+3.52 dB) where the ratio is 1 (the cutoff) and
    /// dips to 0.75 / 1.5 = 0.5 (−6.02 dB) where it is ½ (an octave below).
    #[test]
    fn comb_peaks_and_dips_with_feedback() {
        let u = FilterResponseUniforms {
            midi_cutoff: 60.0,
            resonance: 0.5,
            drive: 1.0,
            mix: 1.0,
            stage0: 1.0,
            stage2: 1000.0,
            stage3: -1000.0,
            ..Default::default()
        };
        close(comb(&u, x_at(60.0), 0.0), y_at(20.0 * 1.5f32.log10()));
        close(comb(&u, x_at(48.0), 0.0), y_at(20.0 * 0.5f32.log10()));
        // Without feedback it is flat.
        let open = FilterResponseUniforms {
            resonance: 0.0,
            ..u
        };
        close(comb(&open, x_at(54.0), 0.0), y_at(0.0));
    }

    /// One stage (stage0 = 1, peak1 = all_pass⁴) with no feedback and the
    /// phase not inverted (db24 = 0): response = 0.5 + 0.5·all_pass⁴. At
    /// the cutoff g = 1: one_pole = (1 − i)/2, all_pass = i, all_pass⁴ = 1,
    /// so |response| = 1, 0 dB. Inverted (db24 = 1): 0.5 − 0.5 = 0, −∞ dB.
    #[test]
    fn phaser_notch_and_peak_at_the_cutoff() {
        let u = FilterResponseUniforms {
            midi_cutoff: 60.0,
            resonance: 0.0,
            mix: 1.0,
            stage0: 1.0,
            ..Default::default()
        };
        let x = x_at(60.0);
        close(phaser(&u, x), y_at(0.0));
        let inverted = FilterResponseUniforms { db24: 1.0, ..u };
        assert!(phaser(&inverted, x) < -1.0);
    }
}
