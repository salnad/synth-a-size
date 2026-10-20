// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Upgrading presets saved by older upstream versions.
//!
//! Ports `LoadSave::updateFromOldVersion` (`vital/src/common/load_save.cpp`)
//! and `utils::encodeOrderToFloat` / `decodeFloatToOrder`
//! (`vital/src/synthesis/framework/utils.cpp`). Each step runs when the
//! preset's `synth_version` is older than the step's version, in upstream
//! order. Reading a value the preset lacks fails the load, as it does
//! upstream (nlohmann throws on converting `null` to a number).
//!
//! Float expressions whose result depends on the reference build's
//! `-ffast-math` code generation go through [`crate::arith`].

use sas_common::json::Value;
use sas_common::load_save::convert_buffer_to_pcm;
use sas_common::LineGenerator;
use sas_wavetable::creator::WavetableCreator;

use crate::arith;
use crate::constants::*;
use crate::json::{ensure_object, read_f32, read_i32, read_string};
use crate::version::older_than;
use crate::Error;

/// `utils::encodeOrderToFloat`.
pub fn encode_order_to_float(order: &[i32]) -> f32 {
    let mut code: u32 = 0;
    for i in 1..order.len() {
        let index = (0..i).filter(|&j| order[i] < order[j]).count() as u32;
        code = code.wrapping_mul(i as u32 + 1).wrapping_add(index);
    }
    code as f32
}

/// `utils::decodeFloatToOrder` into the first `size` entries of `order`.
pub fn decode_float_to_order(order: &mut [i32], float_code: f32, size: usize) {
    let mut code = float_code as i32;
    for (i, slot) in order.iter_mut().take(size).enumerate() {
        *slot = i as i32;
    }
    for i in 0..size {
        let remaining = (size - i) as i32;
        let index = remaining - 1;
        let inversions = code % remaining;
        code /= remaining;

        let start = (index - inversions) as usize;
        let placement = order[start];
        for j in start..index as usize {
            order[j] = order[j + 1];
        }
        order[index as usize] = placement;
    }
}

/// The entries a range-for over `value` visits, mutably.
fn elements_mut(value: &mut Value) -> Vec<&mut Value> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(items) => items.iter_mut().collect(),
        Value::Object(map) => map.values_mut().collect(),
        other => vec![other],
    }
}

/// `modulation["destination"] == name`.
fn destination_is(modulation: &Value, name: &str) -> Result<bool, Error> {
    Ok(matches!(modulation.get("destination")?, Value::String(text) if text == name))
}

/// Copies the entries into a new list (`json new_modulations; … push_back`),
/// which stays `null` when there are none.
fn rebuilt(modulations: &mut Value) -> Value {
    let mut new_modulations = Value::Null;
    for modulation in elements_mut(modulations) {
        new_modulations.push(modulation.clone());
    }
    new_modulations
}

/// Renames every modulation destination `from` to `to`.
fn rename_destination(modulations: &mut Value, from: &str, to: &str) -> Result<Value, Error> {
    for modulation in elements_mut(modulations) {
        if destination_is(modulation, from)? {
            modulation.set("destination", to);
        }
    }
    Ok(rebuilt(modulations))
}

/// Multiplies the amount of every modulation targeting `destination`
/// (`int index = 1; for (json& modulation : modulations) { …; index++; }`).
fn scale_amounts(
    settings: &mut Value,
    modulations: &mut Value,
    destination: &str,
    scale: f32,
) -> Result<(), Error> {
    for (i, modulation) in elements_mut(modulations).into_iter().enumerate() {
        if destination_is(modulation, destination)? {
            let name = format!("modulation_{}_amount", i + 1);
            let amount = read_f32(settings, &name)?;
            settings.set(&name, amount * scale);
        }
    }
    Ok(())
}

/// The `0.4.3` / `0.4.4` amount remap for one oscillator.
fn remap_distortion_amount(
    settings: &mut Value,
    modulations: &mut Value,
    osc: usize,
    distortion_type: f32,
) -> Result<(), Error> {
    let amount_name = format!("osc_{osc}_distortion_amount");
    let osc_type = read_f32(settings, &format!("osc_{osc}_distortion_type"))?;
    let amount = read_f32(settings, &amount_name)?;
    if osc_type == distortion_type {
        settings.set(&amount_name, 0.5f32 + 0.5 * amount);
        scale_amounts(settings, modulations, &amount_name, 0.5)?;
    }
    Ok(())
}

/// `std::min(a, b)`.
fn std_min(a: f32, b: f32) -> f32 {
    if b < a {
        b
    } else {
        a
    }
}

/// `std::max(a, b)`.
fn std_max(a: f32, b: f32) -> f32 {
    if a < b {
        b
    } else {
        a
    }
}

/// `LoadSave::updateFromOldVersion`.
pub fn update_from_old_version(mut state: Value) -> Result<Value, Error> {
    let mut settings = state.get("settings")?.clone();
    ensure_object(&settings)?;
    let mut modulations = settings.get("modulations")?.clone();
    let mut sample = settings.get("sample")?.clone();
    let version = read_string(&state, "synth_version")?;
    let older = |other: &str| older_than(&version, other);

    if older("0.2.0") || settings.count("sub_octave") != 0 {
        let mut sub_waveform = read_i32(&settings, "sub_waveform")?;
        if sub_waveform == 4 {
            sub_waveform = 5;
        } else if sub_waveform == 5 {
            sub_waveform = 4;
        }
        settings.set("sub_waveform", sub_waveform);

        let sub_octave = read_i32(&settings, "sub_octave")?;
        settings.set("sub_transpose", NOTES_PER_OCTAVE.wrapping_mul(sub_octave));

        let osc_1_filter_routing = read_i32(&settings, "osc_1_filter_routing")?;
        let osc_2_filter_routing = read_i32(&settings, "osc_2_filter_routing")?;
        let sample_filter_routing = read_i32(&settings, "sample_filter_routing")?;
        let sub_filter_routing = read_i32(&settings, "sub_filter_routing")?;
        settings.set("filter_1_osc1_input", 1 - osc_1_filter_routing);
        settings.set("filter_1_osc2_input", 1 - osc_2_filter_routing);
        settings.set("filter_1_sample_input", 1 - sample_filter_routing);
        settings.set("filter_1_sub_input", 1 - sub_filter_routing);
        settings.set("filter_2_osc1_input", osc_1_filter_routing);
        settings.set("filter_2_osc2_input", osc_2_filter_routing);
        settings.set("filter_2_sample_input", sample_filter_routing);
        settings.set("filter_2_sub_input", sub_filter_routing);

        for filter in ["filter_1_style", "filter_2_style"] {
            let mut style = read_i32(&settings, filter)?;
            if style == 2 {
                style = 3;
            } else if style == 3 {
                style = 2;
            }
            settings.set(filter, style);
        }
    }

    if older("0.2.1") {
        adjust_envelope_times(&mut settings, arith::pow_two_thirds)?;
        if settings.count("wave_tables") != 0 {
            let wave_tables = settings.get("wave_tables")?.clone();
            settings.set("wavetables", wave_tables);
        }
    }

    if older("0.2.4") {
        let portamento_type = read_i32(&settings, "portamento_type")?;
        settings.set("portamento_force", 0.max(portamento_type.wrapping_sub(1)));
        if portamento_type == 0 {
            settings.set("portamento_time", -10.0f32);
        }
    }

    if older("0.2.5") {
        adjust_envelope_times(&mut settings, arith::pow_three_quarters)?;
    }

    if older("0.2.6") {
        for i in 1..=NUM_LFOS {
            settings.set(&format!("lfo_{i}_fade_time"), 0.0f32);
            settings.set(&format!("lfo_{i}_delay_time"), 0.0f32);
        }
    }

    if older("0.2.7") {
        let names = ["osc_1_level", "osc_2_level", "sub_level", "sample_level"];
        let mut levels = [0.0f32; 4];
        for (level, name) in levels.iter_mut().zip(names) {
            *level = read_f32(&settings, name)?;
        }
        for (level, name) in levels.iter().zip(names) {
            settings.set(name, arith::adjusted_level(*level));
        }
    }

    if older("0.3.0") {
        let reverb_damping = read_f32(&settings, "reverb_damping")?;
        let reverb_feedback = read_f32(&settings, "reverb_feedback")?;
        // Upstream writes "decay_time", which is not a parameter.
        settings.set("decay_time", (reverb_feedback - 0.8f32) * 10.0);
        settings.set("reverb_high_shelf_gain", -reverb_damping * 4.0);
        settings.set("reverb_pre_high_cutoff", 128.0f32);

        for modulation in elements_mut(&mut modulations) {
            if destination_is(modulation, "reverb_damping")? {
                modulation.set("destination", "reverb_high_shelf_gain");
            }
            if destination_is(modulation, "reverb_feedback")? {
                modulation.set("destination", "reverb_decay_time");
            }
        }
        modulations = rebuilt(&mut modulations);
    }

    if older("0.3.1") {
        let sample_transpose = read_f32(&settings, "sample_transpose")?;
        let sample_keytrack = read_f32(&settings, "sample_keytrack")?;
        if sample_keytrack != 0.0 {
            settings.set("sample_transpose", sample_transpose + 28.0);
        }
    }

    if older("0.3.2") {
        for osc in 1..=2 {
            let transpose_name = format!("osc_{osc}_transpose");
            let transpose = read_f32(&settings, &transpose_name)?;
            let midi_track = read_f32(&settings, &format!("osc_{osc}_midi_track"))?;
            if midi_track == 0.0 {
                settings.set(&transpose_name, transpose - 48.0);
            }
        }
    }

    if older("0.3.4") {
        let float_order = read_f32(&settings, "effect_chain_order")?;
        let mut effect_order = [0i32; NUM_EFFECTS];
        decode_float_to_order(&mut effect_order, float_order, NUM_EFFECTS - 1);
        for order in effect_order.iter_mut().take(NUM_EFFECTS - 1) {
            if *order >= EFFECT_FILTER_FX {
                *order += 1;
            }
        }
        effect_order[NUM_EFFECTS - 1] = EFFECT_FILTER_FX;
        settings.set("effect_chain_order", encode_order_to_float(&effect_order));
    }

    if older("0.3.5") {
        let osc_1_distortion_type = read_f32(&settings, "osc_1_distortion_type")?;
        let osc_2_distortion_type = read_f32(&settings, "osc_2_distortion_type")?;
        if osc_1_distortion_type >= 10.0 {
            settings.set("osc_1_distortion_type", osc_1_distortion_type + 1.0);
        }
        if osc_2_distortion_type >= 10.0 {
            settings.set("osc_2_distortion_type", osc_2_distortion_type + 1.0);
        }
    }

    if older("0.3.6") {
        for i in 1..=NUM_LFOS {
            let name = format!("lfo_{i}_sync_type");
            if settings.count(&name) != 0 {
                let value = read_f32(&settings, &name)?;
                if value >= 2.0 {
                    settings.set(&name, value - 1.0);
                }
            }
        }
    }

    if older("0.3.7") {
        ensure_object(&sample)?;
        convert_buffer_to_pcm(&mut sample, "samples")?;
        convert_buffer_to_pcm(&mut sample, "samples_stereo")?;
    }

    if older("0.4.1") {
        let mut update = false;
        for modulation in elements_mut(&mut modulations) {
            if matches!(modulation.get("source")?, Value::String(text) if text == "perlin") {
                update = true;
                modulation.set("source", "random_1");
            }
        }
        let new_modulations = rebuilt(&mut modulations);
        if update {
            modulations = new_modulations;
            settings.set("random_1_sync", 0.0f32);
            #[allow(clippy::excessive_precision)]
            settings.set("random_1_frequency", 1.65149612947f32);
            settings.set("random_1_stereo", 1.0f32);
        }
    }

    if older("0.4.3") {
        remap_distortion_amount(&mut settings, &mut modulations, 1, DISTORTION_FORMANT)?;
        remap_distortion_amount(&mut settings, &mut modulations, 2, DISTORTION_FORMANT)?;
    }

    if older("0.4.4") {
        remap_distortion_amount(&mut settings, &mut modulations, 1, DISTORTION_SYNC)?;
        remap_distortion_amount(&mut settings, &mut modulations, 2, DISTORTION_SYNC)?;
    }

    if older("0.4.5") {
        let low_band = read_f32(&settings, "compressor_low_band")?;
        let high_band = read_f32(&settings, "compressor_high_band")?;
        let bands = match (low_band != 0.0, high_band != 0.0) {
            (true, true) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (false, false) => 3,
        };
        settings.set("compressor_enabled_bands", bands);
    }

    if older("0.4.7") && settings.count("osc_1_distortion_type") != 0 {
        upgrade_0_4_7(&mut settings, &mut modulations)?;
    }

    if older("0.5.0") && settings.count("sub_on") != 0 {
        modulations = upgrade_0_5_0(&mut settings, &mut modulations)?;
    }

    if older("0.5.5") {
        for name in [
            "flanger_tempo",
            "phaser_tempo",
            "chorus_tempo",
            "delay_tempo",
        ] {
            let tempo = read_f32(&settings, name)?;
            settings.set(name, tempo + 1.0);
        }
        let tempo_names = (1..=NUM_LFOS)
            .map(|i| format!("lfo_{i}_tempo"))
            .chain((1..=NUM_RANDOM_LFOS).map(|i| format!("random_{i}_tempo")));
        for name in tempo_names {
            if settings.count(&name) != 0 {
                let tempo = read_f32(&settings, &name)?;
                settings.set(&name, tempo + 1.0);
            }
        }
    }

    if older("0.5.7") {
        for (to, from) in [
            ("delay_aux_sync", "delay_sync"),
            ("delay_aux_frequency", "delay_frequency"),
            ("delay_aux_tempo", "delay_tempo"),
        ] {
            let value = settings.get(from)?.clone();
            settings.set(to, value);
        }
        let style = read_f32(&settings, "delay_style")?;
        if style != 0.0 {
            settings.set("delay_style", style + 1.0);
        }
    }

    if older("0.5.8") {
        settings.set("chorus_damping", 1.0f32);
    }

    if older("0.6.5") {
        settings.set("stereo_mode", 1.0f32);
        let routing = read_f32(&settings, "stereo_routing")? * 0.125;
        settings.set(
            "stereo_routing",
            if routing < 0.0 {
                1.0 - routing
            } else {
                routing
            },
        );
    }

    if older("0.6.6") {
        let stereo_mode = read_f32(&settings, "stereo_mode")?;
        let routing = read_f32(&settings, "stereo_routing")?;
        if stereo_mode == 0.0 {
            settings.set("stereo_routing", 1.0 - routing);
        }
    }

    if older("0.6.7") {
        let chorus_damping = read_f32(&settings, "chorus_damping")?;
        settings.set("chorus_cutoff", 20.0f32);
        settings.set("chorus_spread", chorus_damping);
        modulations = rename_destination(&mut modulations, "chorus_damping", "chorus_spread")?;
    }

    if older("0.7.1") {
        let mut morph_types = [0.0f32; 2];
        for (osc, morph_type) in morph_types.iter_mut().enumerate() {
            let name = format!("osc_{}_spectral_morph_type", osc + 1);
            if settings.count(&name) != 0 {
                *morph_type = read_f32(&settings, &name)?;
            }
        }
        for (osc, morph_type) in morph_types.iter().enumerate() {
            if *morph_type == 9.0 {
                let name = format!("osc_{}_spectral_morph_amount", osc + 1);
                let amount = read_f32(&settings, &name)?;
                settings.set(&name, -0.5f32 * amount + 0.5);
                scale_amounts(&mut settings, &mut modulations, &name, -0.5)?;
            }
        }
    }

    if older("0.7.5") {
        const FLANGER_CENTER_MULTIPLY: f32 = 48.0 / 128.0;
        const FLANGER_CENTER_OFFSET: f32 = 53.69;
        const PHASER_CENTER_MULTIPLY: f32 = 48.0 / 128.0;

        if settings.count("flanger_center") != 0 {
            let center = read_f32(&settings, "flanger_center")?;
            settings.set("flanger_center", center + FLANGER_CENTER_OFFSET);
        }
        for (i, modulation) in elements_mut(&mut modulations).into_iter().enumerate() {
            let name = format!("modulation_{}_amount", i + 1);
            if destination_is(modulation, "flanger_center")? {
                let amount = read_f32(&settings, &name)?;
                settings.set(&name, amount * FLANGER_CENTER_MULTIPLY);
            }
            if destination_is(modulation, "phaser_center")? {
                let amount = read_f32(&settings, &name)?;
                settings.set(&name, amount * PHASER_CENTER_MULTIPLY);
            }
        }
    }

    if older("0.7.6") {
        let filter_1_model = read_i32(&settings, "filter_1_model")?;
        let filter_2_model = read_i32(&settings, "filter_2_model")?;
        for (model, style_name) in [
            (filter_1_model, "filter_1_style"),
            (filter_2_model, "filter_2_style"),
        ] {
            if model == 6 && read_i32(&settings, style_name)? == 1 {
                settings.set(style_name, 3);
            }
        }
        if settings.count("filter_fx_model") != 0
            && read_i32(&settings, "filter_fx_model")? == 6
            && read_i32(&settings, "filter_fx_style")? == 1
        {
            settings.set("filter_fx_style", 3);
        }

        for osc in 1..=NUM_OSCILLATORS {
            let name = format!("osc_{osc}_spectral_morph_type");
            if settings.count(&name) != 0 && read_f32(&settings, &name)? == 10.0 {
                settings.set(&name, 7.0f32);
            }
        }
    }

    if older("0.8.1") {
        // Upstream numbers these from 0, so it writes "lfo_0_smooth_mode"
        // (not a parameter) and leaves lfo_8 alone.
        for i in 0..NUM_LFOS {
            settings.set(&format!("lfo_{i}_smooth_mode"), 0.0f32);
        }
    }

    if older("0.9.0") {
        for filter in 1..=2 {
            let model = read_f32(&settings, &format!("filter_{filter}_model"))?;
            if model == 4.0 {
                let blend_name = format!("filter_{filter}_blend");
                settings.set(&blend_name, 0.0f32);
                settings.set(&format!("filter_{filter}_style"), 0.0f32);
                for (i, modulation) in elements_mut(&mut modulations).into_iter().enumerate() {
                    if destination_is(modulation, &blend_name)? {
                        settings.set(&format!("modulation_{}_amount", i + 1), 0.0f32);
                    }
                }
            }
        }
    }

    settings.set("modulations", modulations);
    settings.set("sample", sample);
    ensure_object(&state)?;
    state.set("settings", settings);
    Ok(state)
}

/// The `0.5.0` step: the sub oscillator becomes oscillator 3 (with a
/// wavetable of the predefined shapes), and filter routing switches become
/// per-source destinations.
fn upgrade_0_5_0(settings: &mut Value, modulations: &mut Value) -> Result<Value, Error> {
    for (to, from) in [
        ("osc_3_on", "sub_on"),
        ("osc_3_level", "sub_level"),
        ("osc_3_pan", "sub_pan"),
        ("osc_3_transpose", "sub_transpose"),
    ] {
        let value = settings.get(from)?.clone();
        settings.set(to, value);
    }
    if settings.count("sub_transpose_quantize") != 0 {
        let value = settings.get("sub_transpose_quantize")?.clone();
        settings.set("osc_3_transpose_quantize", value);
    }
    let sub_tune = settings.get("sub_tune")?.clone();
    settings.set("osc_3_tune", sub_tune);
    settings.set("osc_3_phase", 0.25f32);
    settings.set("osc_3_random_phase", 0.0f32);

    let sub_waveform = read_f32(settings, "sub_waveform")?;
    settings.set("osc_3_wave_frame", arith::sub_wave_frame(sub_waveform));

    let sub_filter1 = read_f32(settings, "filter_1_sub_input")?;
    let sub_filter2 = read_f32(settings, "filter_2_sub_input")?;
    let sub_direct_out = read_f32(settings, "sub_direct_out")?;
    let sub_destination = if sub_direct_out != 0.0 {
        4.0f32
    } else {
        destination(sub_filter1, sub_filter2)
    };
    settings.set("osc_3_destination", sub_destination);

    for (inputs, name) in [
        (
            ["filter_1_osc1_input", "filter_2_osc1_input"],
            "osc_1_destination",
        ),
        (
            ["filter_1_osc2_input", "filter_2_osc2_input"],
            "osc_2_destination",
        ),
        (
            ["filter_1_sample_input", "filter_2_sample_input"],
            "sample_destination",
        ),
    ] {
        let filter1 = read_f32(settings, inputs[0])?;
        let filter2 = read_f32(settings, inputs[1])?;
        settings.set(name, destination(filter1, filter2));
    }

    // Older presets list the wavetables in reverse; the sub's predefined
    // shapes become the last one.
    let wavetables = settings.get("wavetables")?.clone();
    let mut new_wavetables = Value::Null;
    for i in (0..wavetables.size()).rev() {
        new_wavetables.push(wavetables.at(i)?.clone());
    }
    let mut sub = WavetableCreator::new();
    sub.init_predefined_waves();
    sub.name = "Sub".to_string();
    new_wavetables.push(sub.to_json());
    settings.set("wavetables", new_wavetables);

    for modulation in elements_mut(modulations) {
        for (from, to) in [
            ("sub_transpose", "osc_3_transpose"),
            ("sub_tune", "osc_3_tune"),
            ("sub_level", "osc_3_level"),
            ("sub_pan", "osc_3_pan"),
        ] {
            if destination_is(modulation, from)? {
                modulation.set("destination", to);
                break;
            }
        }
    }
    Ok(rebuilt(modulations))
}

/// The 0.5.0 mapping from the two filter routing switches to a
/// `SourceDestination` (filter 1, filter 2, both, effects).
fn destination(filter1: f32, filter2: f32) -> f32 {
    match (filter1 != 0.0, filter2 != 0.0) {
        (true, true) => 2.0,
        (false, true) => 1.0,
        (true, false) => 0.0,
        (false, false) => 3.0,
    }
}

/// The `0.2.1` / `0.2.5` envelope time curve changes:
/// `settings[x] = std::pow(settings[x], power)` for the attack, decay and
/// release of each envelope, stopping at the first envelope without an
/// attack.
fn adjust_envelope_times(settings: &mut Value, power: fn(f32) -> f32) -> Result<(), Error> {
    for i in 1..=NUM_ENVELOPES {
        let attack = format!("env_{i}_attack");
        if settings.count(&attack) == 0 {
            break;
        }
        for name in [attack, format!("env_{i}_decay"), format!("env_{i}_release")] {
            let value = read_f32(settings, &name)?;
            settings.set(&name, power(value));
        }
    }
    Ok(())
}

/// The `0.4.7` step: spectral morph split out of the distortion types, and
/// FM amounts moved to a square-root curve, with a remapping line on each
/// modulation of the amount so the modulated range sounds the same.
fn upgrade_0_4_7(settings: &mut Value, modulations: &mut Value) -> Result<(), Error> {
    const REMAP_RESOLUTION: usize = 32;

    let osc_1_distortion_type = read_f32(settings, "osc_1_distortion_type")?;
    if osc_1_distortion_type != 0.0 {
        settings.set("osc_1_distortion_type", osc_1_distortion_type - 1.0);
    }
    let osc_2_distortion_type = read_f32(settings, "osc_2_distortion_type")?;
    if osc_2_distortion_type != 0.0 {
        settings.set("osc_2_distortion_type", osc_2_distortion_type - 1.0);
    }
    if osc_1_distortion_type == 1.0 {
        settings.set("osc_1_spectral_morph_type", SPECTRAL_MORPH_LOW_PASS);
    }
    if osc_2_distortion_type == 1.0 {
        settings.set("osc_2_spectral_morph_type", SPECTRAL_MORPH_LOW_PASS);
    }

    for modulation in elements_mut(modulations) {
        if osc_1_distortion_type == 1.0 && destination_is(modulation, "osc_1_distortion_amount")? {
            modulation.set("destination", "osc_1_spectral_morph_amount");
        } else if osc_2_distortion_type == 1.0
            && destination_is(modulation, "osc_2_distortion_amount")?
        {
            modulation.set("destination", "osc_2_spectral_morph_amount");
        }
    }
    let mut new_modulations = rebuilt(modulations);

    for osc in 1..=2 {
        let distortion_type = read_f32(settings, &format!("osc_{osc}_distortion_type"))?;
        if !(distortion_type == 7.0 || distortion_type == 8.0 || distortion_type == 9.0) {
            continue;
        }
        let amount_name = format!("osc_{osc}_distortion_amount");
        let original_fm_amount = read_f32(settings, &amount_name)?;
        let new_fm_amount = arith::pow_half(original_fm_amount);
        settings.set(&amount_name, new_fm_amount);

        let mut index = 1;
        for modulation in elements_mut(&mut new_modulations) {
            if !destination_is(modulation, &amount_name)? {
                index += 1;
                continue;
            }
            let number = index.to_string();
            let amount_string = format!("modulation_{number}_amount");
            let last_amount = read_f32(settings, &amount_string)?;
            if last_amount == 0.0 {
                // Upstream's `continue` here skips its `index++`, so later
                // entries are matched against shifted slot numbers.
                continue;
            }

            // `settings[...] != 0.0f` converts the value to float (and throws
            // on `null`) rather than comparing json values.
            let bipolar = read_f32(settings, &format!("modulation_{number}_bipolar"))? != 0.0;
            let mut min = std_min(original_fm_amount, original_fm_amount + last_amount);
            let mut max = std_max(original_fm_amount, original_fm_amount + last_amount);
            if bipolar {
                let up = original_fm_amount + last_amount * 0.5;
                let down = original_fm_amount - last_amount * 0.5;
                min = std_min(up, down);
                max = std_max(up, down);
            }

            let min_target = arith::pow_half(min);
            let max_target = arith::pow_half(max);
            let mut new_amount = max_target - min_target;
            if bipolar {
                new_amount = 2.0 * std_max(new_fm_amount - min_target, max_target - new_fm_amount);
            }
            settings.set(&amount_string, new_amount);

            let mut scale = LineGenerator::new(LineGenerator::DEFAULT_RESOLUTION);
            scale.init_linear();
            scale.set_num_points(REMAP_RESOLUTION);
            let points = arith::fm_remap_points(REMAP_RESOLUTION, min, max, min_target, new_amount);
            for (i, point) in points.into_iter().enumerate() {
                scale.set_point(i, point);
            }
            modulation.set("line_mapping", scale.to_json());
            index += 1;
        }
    }

    *modulations = new_modulations;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_codes_round_trip() {
        let mut order = [0i32; 9];
        for code in [0.0f32, 1.0, 17.0, 5039.0, 362_879.0] {
            decode_float_to_order(&mut order, code, 9);
            assert_eq!(encode_order_to_float(&order), code);
        }
    }
}
