// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `FilterResponse`: a filter's response curve from its controls.
//!
//! Ports `FilterResponse` (`vital/src/interface/editor_components/
//! filter_response.cpp`): upstream sets up the engine's own filter of the
//! chosen model at a visual sample rate (`setupFilter`), reads its
//! coefficients into the response shader's uniforms (`loadShader`), and
//! turns 512 line positions into heights. The port does the same with
//! `sas-filters`' filters and `controls::filter_response`'s evaluators.

use sas_filters::character::comb_filter::CombFilter;
use sas_filters::character::formant_filter::FormantFilter;
use sas_filters::character::phaser_filter::PhaserFilter;
use sas_filters::sas_dsp_core::processor::ProcessorRef;
use sas_filters::{
    DigitalSvf, DiodeFilter, DirtyFilter, FilterState, LadderFilter, SallenKeyFilter, SynthFilter,
};
use sas_simd::PolyFloat;

use super::filter_response::{self as eval, FilterResponseUniforms, FormantUniforms};

/// `FilterResponse::kResolution`.
pub const RESOLUTION: usize = 512;
/// `FilterResponse::kDefaultVisualSampleRate`.
const VISUAL_SAMPLE_RATE: i32 = 200_000;
/// `FilterResponse::kCombAlternatePeriod`.
const COMB_ALTERNATE_PERIOD: usize = 3;

/// `vital::constants::FilterModel`.
pub mod model {
    pub const ANALOG: usize = 0;
    pub const DIRTY: usize = 1;
    pub const LADDER: usize = 2;
    pub const DIGITAL: usize = 3;
    pub const DIODE: usize = 4;
    pub const FORMANT: usize = 5;
    pub const COMB: usize = 6;
    pub const PHASE: usize = 7;
    pub const COUNT: usize = 8;
}

/// `SynthFilter::Style::k12Db`.
const STYLE_12DB: i32 = 0;

/// `FilterResponse::FilterShader`: which response a model and style draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shader {
    Analog,
    Comb,
    PositiveFlange,
    NegativeFlange,
    Diode,
    Dirty,
    Ladder,
    Phase,
    Formant,
    Digital,
}

/// `getShaderForModel`.
pub fn shader_for(model: usize, style: i32) -> Shader {
    match model {
        model::DIRTY => Shader::Dirty,
        model::LADDER => Shader::Ladder,
        model::DIGITAL => Shader::Digital,
        model::DIODE => Shader::Diode,
        model::FORMANT => Shader::Formant,
        model::COMB => match style % CombFilter::NUM_FEEDBACK_STYLES {
            0 => Shader::Comb,
            1 => Shader::PositiveFlange,
            _ => Shader::NegativeFlange,
        },
        model::PHASE => Shader::Phase,
        _ => Shader::Analog,
    }
}

/// The controls a response is drawn from (the sliders `setFilterResponseSliders`
/// hands it), at rest.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FilterInputs {
    pub model: usize,
    pub style: i32,
    pub cutoff: f32,
    pub resonance: f32,
    pub mix: f32,
    pub blend: f32,
    pub blend_transpose: f32,
    pub formant_x: f32,
    pub formant_y: f32,
    pub formant_transpose: f32,
    pub formant_resonance: f32,
    pub formant_spread: f32,
}

/// A response's uniforms, for the left (0) and right (1) channel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Response {
    pub shader: Shader,
    pub uniforms: [FilterResponseUniforms; 2],
    pub formant: [FormantUniforms; 2],
}

/// `FilterResponse::setupFilterState`: the filter state the controls make.
fn filter_state(inputs: &FilterInputs) -> FilterState {
    let mut state = FilterState {
        midi_cutoff: PolyFloat::splat(inputs.cutoff.max(0.0)),
        resonance_percent: PolyFloat::splat(inputs.resonance),
        pass_blend: PolyFloat::splat(inputs.blend.clamp(0.0, 2.0)),
        transpose: PolyFloat::splat(inputs.blend_transpose),
        interpolate_x: PolyFloat::splat(inputs.formant_x),
        interpolate_y: PolyFloat::splat(inputs.formant_y),
        style: inputs.style,
        ..FilterState::default()
    };
    if inputs.model == model::FORMANT {
        state.transpose = PolyFloat::splat(inputs.formant_transpose);
        state.resonance_percent = PolyFloat::splat(inputs.formant_resonance);
        state.pass_blend = PolyFloat::splat(inputs.formant_spread);
    }
    state
}

/// A filter processor at the visual sample rate.
fn at_visual_rate(processor: ProcessorRef) -> ProcessorRef {
    processor.set_sample_rate(VISUAL_SAMPLE_RATE);
    processor
}

/// `FilterResponse::loadShader` for both channels: sets the model's filter
/// up and reads its coefficients.
pub fn response(inputs: &FilterInputs) -> Response {
    let state = filter_state(inputs);
    let shader = shader_for(inputs.model, inputs.style);
    let mix = inputs.mix.clamp(0.0, 1.0);
    let midi_cutoff = state.midi_cutoff;
    let db24 = if state.style != STYLE_12DB { 1.0 } else { 0.0 };
    let mut out = Response {
        shader,
        uniforms: [FilterResponseUniforms::default(); 2],
        formant: [FormantUniforms::default(); 2],
    };
    let lanes = |f: &dyn Fn(usize) -> FilterResponseUniforms| [f(0), f(1)];
    match inputs.model {
        model::ANALOG => {
            let filter = at_visual_rate(SallenKeyFilter::create());
            out.uniforms = filter.with::<SallenKeyFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: midi_cutoff.get(i),
                    resonance: f.get_resonance().get(i).clamp(0.0, 2.0),
                    drive: f.get_drive().get(i),
                    mix,
                    db24,
                    stage0: f.get_low_amount().get(i),
                    stage1: f.get_band_amount().get(i),
                    stage2: f.get_high_amount().get(i),
                    stage3: f.get_low_amount24(state.style).get(i),
                    stage4: f.get_high_amount24(state.style).get(i),
                })
            });
        }
        model::COMB => {
            let filter = at_visual_rate(CombFilter::create(CombFilter::MIN_PERIOD));
            out.uniforms = filter.with::<CombFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: midi_cutoff.get(i),
                    resonance: f.resonance().get(i).clamp(-0.99, 0.99),
                    drive: f.drive().get(i),
                    mix,
                    stage0: f.low_amount().get(i),
                    stage1: f.high_amount().get(i),
                    stage2: f.filter_midi_cutoff().get(i),
                    stage3: f.filter2_midi_cutoff().get(i),
                    ..Default::default()
                })
            });
        }
        model::DIGITAL => {
            let filter = at_visual_rate(DigitalSvf::create());
            out.uniforms = filter.with::<DigitalSvf, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: f.get_midi_cutoff().get(i),
                    resonance: f.get_resonance().get(i).clamp(0.0, 2.0),
                    drive: f.get_drive().get(i),
                    mix,
                    db24,
                    stage0: f.get_low_amount().get(i),
                    stage1: f.get_band_amount().get(i),
                    stage2: f.get_high_amount().get(i),
                    stage3: f.get_low_amount24(state.style).get(i),
                    stage4: f.get_high_amount24(state.style).get(i),
                })
            });
        }
        model::DIODE => {
            let filter = at_visual_rate(DiodeFilter::create());
            out.uniforms = filter.with::<DiodeFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: midi_cutoff.get(i),
                    resonance: f.get_resonance().get(i),
                    drive: f.get_drive().get(i),
                    mix,
                    db24: f.get_high_pass_amount().get(i),
                    stage0: f.get_high_pass_ratio().get(i),
                    ..Default::default()
                })
            });
        }
        model::DIRTY => {
            let filter = at_visual_rate(DirtyFilter::create());
            out.uniforms = filter.with::<DirtyFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: midi_cutoff.get(i),
                    resonance: f.get_resonance().get(i).clamp(0.0, 2.0),
                    drive: f.get_drive().get(i),
                    mix,
                    db24,
                    stage0: f.get_low_amount().get(i),
                    stage1: f.get_band_amount().get(i),
                    stage2: f.get_high_amount().get(i),
                    stage3: f.get_low_amount24(state.style).get(i),
                    stage4: f.get_high_amount24(state.style).get(i),
                })
            });
        }
        model::FORMANT => {
            let filter = at_visual_rate(FormantFilter::create(0));
            let formants: Vec<ProcessorRef> = filter.with::<FormantFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                (0..4).map(|i| f.get_formant(i)).collect()
            });
            let mut values = [FormantUniforms::default(); 2];
            for (index, formant) in formants.iter().enumerate() {
                formant.with::<DigitalSvf, _>(|svf, _| {
                    let drive = svf.get_drive();
                    for (lane, value) in values.iter_mut().enumerate() {
                        value.cutoff[index] = svf.get_midi_cutoff().get(lane);
                        value.resonance[index] = svf.get_resonance().get(lane);
                        value.low[index] = (svf.get_low_amount() * drive).get(lane);
                        value.band[index] = (svf.get_band_amount() * drive).get(lane);
                        value.high[index] = (svf.get_high_amount() * drive).get(lane);
                    }
                });
            }
            out.formant = values;
            out.uniforms = [FilterResponseUniforms { mix, ..Default::default() }; 2];
        }
        model::LADDER => {
            let filter = at_visual_rate(LadderFilter::create());
            out.uniforms = filter.with::<LadderFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: midi_cutoff.get(i),
                    resonance: f.get_resonance().get(i),
                    drive: f.get_drive().get(i),
                    mix,
                    stage0: f.get_stage_scale(0).get(i),
                    stage1: f.get_stage_scale(1).get(i),
                    stage2: f.get_stage_scale(2).get(i),
                    stage3: f.get_stage_scale(3).get(i),
                    stage4: f.get_stage_scale(4).get(i),
                    ..Default::default()
                })
            });
        }
        _ => {
            let filter = at_visual_rate(PhaserFilter::create(false));
            out.uniforms = filter.with::<PhaserFilter, _>(|f, this| {
                f.setup_filter(this, &state);
                lanes(&|i| FilterResponseUniforms {
                    midi_cutoff: midi_cutoff.get(i),
                    resonance: f.resonance().get(i),
                    mix,
                    db24,
                    stage0: f.peak1_amount().get(i),
                    stage1: f.peak3_amount().get(i),
                    stage2: f.peak5_amount().get(i),
                    ..Default::default()
                })
            });
        }
    }
    out
}

impl Response {
    /// The response height (device coordinates, -1 to 1) of `channel` at
    /// line point `index`.
    pub fn height_at(&self, channel: usize, index: usize) -> f32 {
        let t = index as f32 / (RESOLUTION as f32 - 1.0);
        let x = 2.0 * t - 1.0;
        let y = ((index / COMB_ALTERNATE_PERIOD) % 2) as f32;
        let u = &self.uniforms[channel];
        match self.shader {
            Shader::Analog => eval::analog(u, x),
            Shader::Comb => eval::comb(u, x, y),
            Shader::PositiveFlange => eval::positive_flange(u, x, y),
            Shader::NegativeFlange => eval::negative_flange(u, x, y),
            Shader::Diode => eval::diode(u, x),
            Shader::Dirty => eval::dirty(u, x),
            Shader::Ladder => eval::ladder(u, x),
            Shader::Phase => eval::phaser(u, x),
            Shader::Formant => eval::formant(u, &self.formant[channel], x),
            Shader::Digital => eval::digital(u, x),
        }
    }

    /// `renderLineResponse`: the line's points for `channel` in a view
    /// `width` × `height` (pixels, y down).
    pub fn line(&self, channel: usize, width: f32, height: f32) -> Vec<(f32, f32)> {
        let y_adjust = height / 2.0;
        (0..RESOLUTION)
            .map(|i| {
                let x = width * i as f32 / (RESOLUTION as f32 - 1.0);
                (x, y_adjust * (1.0 - self.height_at(channel, i)))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0 dB is height 2 · (0 + 30) / 50 − 1 = 0.2.
    const ZERO_DB: f32 = 0.2;

    #[test]
    fn an_analog_low_pass_passes_below_its_cutoff() {
        let inputs = FilterInputs {
            model: model::ANALOG,
            style: 0,
            cutoff: 60.0,
            resonance: 0.0,
            mix: 1.0,
            blend: 0.0,
            formant_x: 0.5,
            formant_y: 0.5,
            ..Default::default()
        };
        let r = response(&inputs);
        assert_eq!(r.shader, Shader::Analog);
        assert_eq!(r.uniforms[0], r.uniforms[1]);
        assert!((r.height_at(0, 0) - ZERO_DB).abs() < 0.02, "{}", r.height_at(0, 0));
        assert!(r.height_at(0, RESOLUTION - 1) < ZERO_DB - 0.5);
    }

    #[test]
    fn comb_styles_pick_their_feedback_shader() {
        assert_eq!(shader_for(model::COMB, 0), Shader::Comb);
        assert_eq!(shader_for(model::COMB, 1), Shader::PositiveFlange);
        assert_eq!(shader_for(model::COMB, 5), Shader::NegativeFlange);
        assert_eq!(shader_for(model::FORMANT, 1), Shader::Formant);
    }

    #[test]
    fn every_model_draws_a_finite_line() {
        for model in 0..model::COUNT {
            let inputs = FilterInputs {
                model,
                cutoff: 60.0,
                resonance: 0.5,
                mix: 1.0,
                blend: 1.0,
                formant_x: 0.5,
                formant_y: 0.5,
                formant_resonance: 0.5,
                ..Default::default()
            };
            let r = response(&inputs);
            let line = r.line(0, 300.0, 100.0);
            assert_eq!(line.len(), RESOLUTION);
            assert!(line.iter().all(|(x, y)| x.is_finite() && y.is_finite()), "model {model}");
        }
    }
}
