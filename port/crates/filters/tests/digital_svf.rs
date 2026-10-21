// SPDX-License-Identifier: GPL-3.0-or-later

//! Parity for `DigitalSvf`: `filters/digital_svf` (through
//! `SynthFilter::createFilter`, every style) and `filters/digital_svf_settings`
//! (the equalizer's and formant manager's per-instance settings).

mod support;

use sas_filters::synth_filter::input;
use sas_filters::DigitalSvf;
use support::{
    check_file, filter_module, param_bool, param_f32, Audio, Control, ModuleDef, Trigger,
};

/// `filters/digital_svf.cpp`: module "digital_svf".
fn digital_svf_module() -> ModuleDef {
    ModuleDef {
        create: |params| {
            let mut svf = DigitalSvf::new();
            svf.set_basic(param_bool(params, "basic", false));
            svf.set_drive_compensation(param_bool(params, "drive_compensation", true));
            svf.set_resonance_bounds(
                param_f32(params, "min_resonance", DigitalSvf::DEFAULT_MIN_RESONANCE),
                param_f32(params, "max_resonance", DigitalSvf::DEFAULT_MAX_RESONANCE),
            );
            DigitalSvf::create_with(svf)
        },
        inputs: vec![
            ("audio", input::AUDIO, Audio),
            ("reset", input::RESET, Trigger),
            ("midi_cutoff", input::MIDI_CUTOFF, Audio),
            ("resonance", input::RESONANCE, Control),
            ("drive_gain", input::DRIVE_GAIN, Control),
            ("gain", input::GAIN, Control),
            ("style", input::STYLE, Control),
            ("pass_blend", input::PASS_BLEND, Control),
        ],
        outputs: vec![("audio", 0)],
    }
}

#[test]
fn digital_svf_matches_reference() {
    check_file("digital_svf", &filter_module());
}

#[test]
fn digital_svf_settings_match_reference() {
    check_file("digital_svf_settings", &digital_svf_module());
}
