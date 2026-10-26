// SPDX-License-Identifier: GPL-3.0-or-later

//! The editor's telemetry (`sas_engine::telemetry`): the engine registers
//! exactly upstream's status outputs, every destination has its modulation
//! totals, and a published snapshot holds what the engine computed.

use std::collections::BTreeSet;
use std::sync::Arc;

use sas_dsp_core::ProcessorRef;
use sas_engine::sas_effects::reorderable_effect_chain::ReorderableEffectChain;
use sas_engine::sound_engine::SoundEngine;
use sas_engine::synth::Synth;
use sas_engine::telemetry::{
    destination_id, destination_names, status_id, status_names, Telemetry, OSCILLOSCOPE_SAMPLES,
};
use sas_oscillators::RandomValues;
use sas_preset::{InitContent, PresetState};

/// Every status output name registered anywhere in the module tree.
fn registered_status_outputs(module: &ProcessorRef, names: &mut Vec<String>) {
    let data = module.module_data();
    let data = data.borrow();
    names.extend(data.status_outputs.keys().cloned());
    for sub_module in &data.sub_modules {
        registered_status_outputs(sub_module, names);
    }
}

/// The audit: upstream's 32 `createStatusOutput` call sites (11 in
/// `SoundEngine::init`, 21 in `SynthVoiceHandler`) register 198 names once
/// their loops expand; the port registers exactly those, each once. (The
/// port has 20 call sites, because it folds the six compressor bands and
/// the eight expression sources into loops.)
#[test]
fn the_engine_registers_exactly_upstreams_status_outputs() {
    let synth = Synth::new();
    let mut registered = Vec::new();
    registered_status_outputs(synth.engine(), &mut registered);
    let unique: BTreeSet<&str> = registered.iter().map(String::as_str).collect();
    assert_eq!(
        unique.len(),
        registered.len(),
        "a status output is registered twice"
    );
    let expected: BTreeSet<&str> = status_names().iter().copied().collect();
    assert_eq!(status_names().len(), 198);
    assert_eq!(unique, expected);
}

#[test]
fn every_destination_has_modulation_totals() {
    let synth = Synth::new();
    let mono = synth.engine().get_mono_modulations();
    let poly = synth.engine().get_poly_modulations();
    let destinations: BTreeSet<&str> = destination_names().iter().copied().collect();
    assert_eq!(destinations.len(), destination_names().len());
    assert_eq!(
        mono.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        destinations
    );
    assert!(poly.keys().all(|name| destinations.contains(name.as_str())));
    assert!(poly.len() > 300, "{} per-voice destinations", poly.len());
}

fn render_blocks(synth: &mut Synth, blocks: usize, seconds: &mut f64) -> Vec<[f32; 2]> {
    let (mut left, mut right) = (vec![0.0f32; 64], vec![0.0f32; 64]);
    let mut out = Vec::new();
    for _ in 0..blocks {
        synth.render_block(&mut left, &mut right, &[], seconds, 44100);
        out.extend(left.iter().zip(&right).map(|(l, r)| [*l, *r]));
    }
    out
}

#[test]
fn a_snapshot_holds_what_the_engine_computed() {
    sas_common::random::set_next_seed(2);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    synth.load_preset(&PresetState::new(&InitContent { sample_seed: 0 }));
    synth.prepare(44100, 120.0);
    synth.connect_modulation("lfo_1", "filter_1_cutoff");
    for (name, value) in [
        ("filter_1_on", 1.0),
        ("modulation_1_amount", 0.5),
        ("lfo_1_frequency", 2.0),
    ] {
        synth.set_control(name, value);
    }
    let (mut writer, telemetry) = Telemetry::new(synth.audio_memory());

    let (mut left, mut right) = (vec![0.0f32; 64], vec![0.0f32; 64]);
    let mut seconds = 0.0;
    synth.render_block(
        &mut left,
        &mut right,
        &[(0, [0x90, 57, 100])],
        &mut seconds,
        44100,
    );
    let rendered = render_blocks(&mut synth, 400, &mut seconds);
    synth.write_telemetry(&mut writer);

    telemetry.read(|snapshot| {
        for name in [
            "lfo_1",
            "lfo_1_phase",
            "env_1",
            "num_voices",
            "peak_meter",
            "note",
        ] {
            let id = status_id(name).unwrap();
            let engine = synth
                .engine()
                .get_status_output(name)
                .unwrap()
                .value()
                .to_array();
            assert_eq!(snapshot.status(id), engine, "{name}");
        }
        assert_eq!(snapshot.status_by_name("num_voices").unwrap()[0], 1.0);
        assert!(snapshot.status_by_name("peak_meter").unwrap()[0] > 0.0);

        let cutoff = snapshot.modulation_total(destination_id("filter_1_cutoff").unwrap());
        assert!(cutoff.enabled);
        assert!(cutoff
            .mono
            .iter()
            .chain(cutoff.poly.as_ref().unwrap())
            .any(|v| *v != 0.0));
        let mono = synth.engine().get_mono_modulations();
        let poly = synth.engine().get_poly_modulations();
        for (index, name) in destination_names().iter().enumerate() {
            let total = snapshot.modulation_total(sas_engine::telemetry::DestinationId(index));
            assert_eq!(total.mono, mono[*name].trigger_value().to_array(), "{name}");
            assert_eq!(
                total.poly,
                poly.get(*name).map(|p| p.trigger_value().to_array()),
                "{name}"
            );
        }

        // A held A3 fills the oscilloscope window with audio.
        assert_eq!(snapshot.oscilloscope().len(), OSCILLOSCOPE_SAMPLES);
        assert!(snapshot.oscilloscope().iter().any(|frame| frame[0] != 0.0));
    });

    let mut latest = vec![[0.0f32; 2]; 256];
    telemetry.audio_memory().read_latest(&mut latest);
    assert_eq!(latest[..], rendered[rendered.len() - 256..]);
}

/// The oscilloscope window is the output sampled every
/// `sample_rate / kOscilloscopeMemorySampleRate` frames (every other frame
/// at 44.1 kHz) from the start of a period of the note played.
#[test]
fn the_oscilloscope_window_samples_the_output() {
    sas_common::random::set_next_seed(4);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    synth.load_preset(&PresetState::new(&InitContent { sample_seed: 0 }));
    synth.prepare(44100, 120.0);
    let (mut writer, telemetry) = Telemetry::new(Arc::clone(&synth.audio_memory()));

    let (mut left, mut right) = (vec![0.0f32; 64], vec![0.0f32; 64]);
    let mut seconds = 0.0;
    synth.render_block(
        &mut left,
        &mut right,
        &[(0, [0x90, 57, 100])],
        &mut seconds,
        44100,
    );
    let rendered = render_blocks(&mut synth, 300, &mut seconds);
    synth.write_telemetry(&mut writer);
    let window = telemetry.read(|snapshot| snapshot.oscilloscope().to_vec());

    // A3's period (200.45 frames) doubled up to the 1024-frame window is
    // 1603.6 frames: 802 entries at every other frame.
    let written = 802;
    assert!(window[..written].iter().any(|frame| frame[0] != 0.0));
    let found = (0..rendered.len() - 2 * written)
        .any(|start| (0..written).all(|i| rendered[start + 2 * i] == window[i]));
    assert!(found, "the window is not the output at every other frame");
}

/// The equalizer's memory (`SynthBase::getEqualizerMemory`) reaches the
/// editor: while the EQ is on, each block adds the samples it processed (at
/// the oversampled rate, as upstream's memory holds them), holding a note's
/// sound; with the EQ off it adds none. The snapshot carries the sample rate
/// and oversampling the spectrogram scales by.
#[test]
fn the_equalizer_memory_follows_the_eq() {
    sas_common::random::set_next_seed(2);
    RandomValues::reset_process_state();
    let mut synth = Synth::new();
    synth.load_preset(&PresetState::new(&InitContent { sample_seed: 0 }));
    synth.prepare(44100, 120.0);
    synth.set_control("eq_on", 1.0);
    let (mut writer, telemetry) =
        Telemetry::with_equalizer_memory(synth.audio_memory(), synth.equalizer_memory());

    let (mut left, mut right) = (vec![0.0f32; 64], vec![0.0f32; 64]);
    let mut seconds = 0.0;
    synth.render_block(
        &mut left,
        &mut right,
        &[(0, [0x90, 57, 100])],
        &mut seconds,
        44100,
    );
    let before = telemetry.equalizer_memory().written();
    assert_eq!(before, 64 * 2, "the first block, at 2x oversampling");
    render_blocks(&mut synth, 10, &mut seconds);
    synth.write_telemetry(&mut writer);
    let oversampling = telemetry.read(|snapshot| {
        assert_eq!(snapshot.sample_rate, 44100);
        snapshot.oversampling_amount
    });
    assert_eq!(oversampling, 2, "the init preset's oversampling");
    let written = telemetry.equalizer_memory().written() - before;
    assert_eq!(written, 10 * 64 * oversampling as usize);
    let mut frames = vec![[0.0f32; 2]; 256];
    telemetry.equalizer_memory().read_latest(&mut frames);
    assert!(
        frames.iter().any(|frame| frame[0] != 0.0),
        "the note's sound"
    );
    // The copy's latest frames are the equalizer memory's, oldest first.
    let source = synth
        .engine()
        .with::<SoundEngine, _>(|engine, _| {
            engine
                .effect_chain()
                .with::<ReorderableEffectChain, _>(|chain, _| chain.get_equalizer_memory())
        })
        .expect("the effect chain has an equalizer");
    let source = source.borrow();
    let mask = source.size() as u32 - 1;
    let offset = source.offset();
    for (i, frame) in frames.iter().enumerate() {
        let age = (frames.len() - 1 - i) as u32;
        let index = (offset.wrapping_sub(age) & mask) as usize;
        assert_eq!(
            *frame,
            [source.buffer(0)[index], source.buffer(1)[index]],
            "frame {i}"
        );
    }

    synth.set_control("eq_on", 0.0);
    render_blocks(&mut synth, 1, &mut seconds);
    let held = telemetry.equalizer_memory().written();
    render_blocks(&mut synth, 10, &mut seconds);
    assert_eq!(telemetry.equalizer_memory().written(), held);
}
