// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! `vital::FormantFilter`: four `DigitalSvf` formant stages whose gain,
//! resonance and cutoff are bilinearly interpolated between four vowels.
//!
//! Ports `vital/src/synthesis/filters/formant_filter.h` and
//! `formant_filter.cpp`. Processing is the router graph built in `init`;
//! `setupFilter` (configuring the stages directly) is not called while
//! processing.
//!
//! Upstream plugs every filter's stages into shared static `cr::Value`s
//! (the vowel tables, `k12Db`, `constants::kValueOne`). Here each filter
//! owns control-rate `Value` nodes with the same constants; they are
//! sources only (never processed), so the outputs are identical.

// The vowel tables keep upstream's literals verbatim.
#![allow(clippy::excessive_precision)]

use sas_dsp_core::operators::{cr, Add, BilinearInterpolate, Interpolate};
use sas_dsp_core::sas_simd::{poly_utils as utils, PolyFloat, PolyMask};
use sas_dsp_core::{processor_boilerplate, Processor, ProcessorImpl, ProcessorRef, Value};

use super::formant_manager::FormantManager;
use crate::digital_svf::DigitalSvf;
use crate::synth_filter::{input, style, FilterState, SynthFilter};

/// `kNumFormants` (`synth_constants.h`).
pub const NUM_FORMANTS: usize = 4;

/// `FormantValues`: gain (dB), resonance, MIDI cutoff of one formant.
#[derive(Clone, Copy, Debug)]
struct FormantValues {
    gain: f32,
    resonance: f32,
    midi_cutoff: f32,
}

const fn fv(gain: f32, resonance: f32, midi_cutoff: f32) -> FormantValues {
    FormantValues {
        gain,
        resonance,
        midi_cutoff,
    }
}

const FORMANT_A: [FormantValues; NUM_FORMANTS] = [
    fv(-2.0, 0.66, 75.755_234_332_7),
    fv(-8.0, 0.75, 84.545_470_602_3),
    fv(-9.0, 1.0, 100.085_003_17),
    fv(-10.0, 1.0, 101.645_729_657),
];

const FORMANT_E: [FormantValues; NUM_FORMANTS] = [
    fv(0.0, 0.66, 67.349_957_715),
    fv(-14.0, 0.75, 92.399_511_81),
    fv(-4.0, 1.0, 99.755_234_332_7),
    fv(-14.0, 1.0, 103.349_957_715),
];

const FORMANT_I: [FormantValues; NUM_FORMANTS] = [
    fv(0.0, 0.8, 61.782_592_517_9),
    fv(-15.0, 0.75, 94.049_554_095),
    fv(-17.0, 1.0, 101.038_216_78),
    fv(-20.0, 1.0, 103.618_371_471),
];

const FORMANT_O: [FormantValues; NUM_FORMANTS] = [
    fv(-2.0, 0.7, 67.349_957_715),
    fv(-6.0, 0.75, 79.349_957_715),
    fv(-14.0, 1.0, 99.755_234_332_7),
    fv(-14.0, 1.0, 101.038_216_78),
];

const FORMANT_U: [FormantValues; NUM_FORMANTS] = [
    fv(0.0, 0.7, 65.038_216_779_7),
    fv(-20.0, 0.75, 74.369_507_723_7),
    fv(-17.0, 1.0, 100.408_607_741),
    fv(-14.0, 1.0, 101.645_729_657),
];

// FormantPosition
const BOTTOM_LEFT: usize = 0;
const BOTTOM_RIGHT: usize = 1;
const TOP_LEFT: usize = 2;
const TOP_RIGHT: usize = 3;
const NUM_FORMANT_POSITIONS: usize = 4;

type FormantStyle = [&'static [FormantValues; NUM_FORMANTS]; NUM_FORMANT_POSITIONS];

const FORMANT_STYLE1: FormantStyle = [&FORMANT_A, &FORMANT_O, &FORMANT_I, &FORMANT_E];
const FORMANT_STYLE2: FormantStyle = [&FORMANT_A, &FORMANT_I, &FORMANT_U, &FORMANT_O];

/// `formant_styles`, indexed by `FormantFilter` style.
const FORMANT_STYLES: [FormantStyle; FormantFilter::NUM_FORMANT_STYLES] =
    [FORMANT_STYLE2, FORMANT_STYLE1];

fn bilinear_interpolate(
    top_left: PolyFloat,
    top_right: PolyFloat,
    bot_left: PolyFloat,
    bot_right: PolyFloat,
    x: PolyFloat,
    y: PolyFloat,
) -> PolyFloat {
    let top = utils::interpolate(top_left, top_right, x);
    let bot = utils::interpolate(bot_left, bot_right, x);
    utils::interpolate(bot, top, y)
}

fn interpolate_formants(
    top_left: &FormantValues,
    top_right: &FormantValues,
    bot_left: &FormantValues,
    bot_right: &FormantValues,
    formant_x: PolyFloat,
    formant_y: PolyFloat,
) -> FilterState {
    let corners = |field: fn(&FormantValues) -> f32| {
        bilinear_interpolate(
            PolyFloat::splat(field(top_left)),
            PolyFloat::splat(field(top_right)),
            PolyFloat::splat(field(bot_left)),
            PolyFloat::splat(field(bot_right)),
            formant_x,
            formant_y,
        )
    };
    FilterState {
        midi_cutoff: corners(|v| v.midi_cutoff),
        resonance_percent: corners(|v| v.resonance),
        gain: corners(|v| v.gain),
        ..FilterState::default()
    }
}

/// `vital::FormantFilter`.
#[derive(Clone)]
pub struct FormantFilter {
    formant_manager: ProcessorRef,
    style: usize,
    /// The constant sources the stages are plugged into (kept alive here).
    constants: Vec<ProcessorRef>,
}

impl FormantFilter {
    // FormantStyle
    pub const AOIE: usize = 0;
    pub const AIUO: usize = 1;
    pub const NUM_FORMANT_STYLES: usize = 2;
    pub const VOCAL_TRACT: usize = Self::NUM_FORMANT_STYLES;
    pub const TOTAL_FORMANT_FILTERS: usize = 3;

    pub const CENTER_MIDI: f32 = 80.0;

    /// `new FormantFilter(style)`: `ProcessorRouter(kNumInputs, 1)`. Call
    /// `init()` before processing.
    pub fn create(style: usize) -> ProcessorRef {
        let formant_manager = FormantManager::create(NUM_FORMANTS);
        let router = Processor::new_router(
            FormantFilter {
                formant_manager: formant_manager.clone(),
                style,
                constants: Vec::new(),
            },
            input::NUM_INPUTS,
            1,
            false,
        );
        router.add_processor(&formant_manager);
        router
    }

    /// `getFormant(index)`: the global stage `index`.
    pub fn get_formant(&self, index: usize) -> ProcessorRef {
        self.formant_manager
            .with::<FormantManager, _>(|manager, _| manager.get_formant(index))
    }

    fn constant(&mut self, value: f32) -> ProcessorRef {
        let node = Value::create_control_rate(value);
        self.constants.push(node.clone());
        node
    }
}

impl ProcessorImpl for FormantFilter {
    processor_boilerplate!();

    fn init(&mut self, this: &Processor) {
        let k12_db = self.constant(style::K12_DB as f32);
        let value_one = self.constant(1.0);

        self.formant_manager.use_output(&this.output(0), 0);
        let center = Value::create(Self::CENTER_MIDI, false);
        this.add_idle_processor(&center);

        let formant_style = FORMANT_STYLES[self.style];
        for i in 0..NUM_FORMANTS {
            let formant_gain = cr::BilinearInterpolate::create();
            let formant_q = cr::BilinearInterpolate::create();
            let formant_midi = BilinearInterpolate::create();

            for (p, vowel) in formant_style.iter().enumerate() {
                let values = vowel[i];
                let gain = self.constant(values.gain);
                let resonance = self.constant(values.resonance);
                let midi_cutoff = self.constant(values.midi_cutoff);
                formant_gain.plug(&gain, cr::BilinearInterpolate::POSITION_START + p);
                formant_q.plug(&resonance, cr::BilinearInterpolate::POSITION_START + p);
                formant_midi.plug(&midi_cutoff, BilinearInterpolate::POSITION_START + p);
            }

            let interpolate_x = this.input(input::INTERPOLATE_X);
            formant_gain.use_input(&interpolate_x, cr::BilinearInterpolate::X_POSITION);
            formant_q.use_input(&interpolate_x, cr::BilinearInterpolate::X_POSITION);
            formant_midi.use_input(&interpolate_x, BilinearInterpolate::X_POSITION);

            let interpolate_y = this.input(input::INTERPOLATE_Y);
            formant_gain.use_input(&interpolate_y, cr::BilinearInterpolate::Y_POSITION);
            formant_q.use_input(&interpolate_y, cr::BilinearInterpolate::Y_POSITION);
            formant_midi.use_input(&interpolate_y, BilinearInterpolate::Y_POSITION);

            let formant_midi_spread = Interpolate::create();
            formant_midi_spread.use_input(&this.input(input::SPREAD), Interpolate::FRACTIONAL);
            formant_midi_spread.use_input(&this.input(input::RESET), Interpolate::RESET);
            formant_midi_spread.plug(&center, Interpolate::TO);
            formant_midi_spread.plug(&formant_midi, Interpolate::FROM);

            let formant_midi_adjust = Add::create();
            formant_midi_adjust.use_input(&this.input(input::TRANSPOSE), 0);
            formant_midi_adjust.plug(&formant_midi_spread, 1);

            let formant_q_adjust = cr::Multiply::create();
            formant_q_adjust.use_input(&this.input(input::RESONANCE), 0);
            formant_q_adjust.plug(&formant_q, 1);

            let formant = self.get_formant(i);
            formant.use_input(&this.input(input::AUDIO), input::AUDIO);
            formant.use_input(&this.input(input::RESET), input::RESET);
            formant.plug(&k12_db, input::STYLE);
            formant.plug(&value_one, input::PASS_BLEND);
            formant.plug(&formant_gain, input::GAIN);
            formant.plug(&formant_q_adjust, input::RESONANCE);
            formant.plug(&formant_midi_adjust, input::MIDI_CUTOFF);

            this.add_processor(&formant_gain);
            this.add_processor(&formant_q);
            this.add_processor(&formant_q_adjust);
            this.add_processor(&formant_midi);
            this.add_processor(&formant_midi_spread);
            this.add_processor(&formant_midi_adjust);
        }

        this.router_init();
    }

    fn reset(&mut self, this: &Processor, reset_mask: PolyMask) {
        if let Some(manager) = this.get_local_processor(&self.formant_manager) {
            manager.reset(reset_mask);
        }
    }

    fn hard_reset(&mut self, this: &Processor) {
        if let Some(manager) = this.get_local_processor(&self.formant_manager) {
            manager.hard_reset();
        }
    }
}

impl SynthFilter for FormantFilter {
    fn setup_filter(&mut self, _this: &Processor, filter_state: &FilterState) {
        let style_index = filter_state.style.min(Self::NUM_FORMANT_STYLES as i32 - 1) as usize;
        let formant_style = FORMANT_STYLES[style_index];
        #[allow(clippy::needless_range_loop)]
        for i in 0..NUM_FORMANTS {
            let mut formant_setting = interpolate_formants(
                &formant_style[TOP_LEFT][i],
                &formant_style[TOP_RIGHT][i],
                &formant_style[BOTTOM_LEFT][i],
                &formant_style[BOTTOM_RIGHT][i],
                filter_state.interpolate_x,
                filter_state.interpolate_y,
            );

            formant_setting.midi_cutoff = utils::interpolate(
                formant_setting.midi_cutoff,
                Self::CENTER_MIDI,
                filter_state.pass_blend,
            );
            formant_setting.midi_cutoff += filter_state.transpose;
            formant_setting.resonance_percent *= filter_state.resonance_percent;
            formant_setting.style = style::K12_DB;
            formant_setting.pass_blend = PolyFloat::splat(1.0);
            self.get_formant(i)
                .with::<DigitalSvf, _>(|svf, node| svf.setup_filter(node, &formant_setting));
        }
    }
}
