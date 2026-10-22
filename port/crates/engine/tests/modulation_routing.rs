// SPDX-License-Identifier: GPL-3.0-or-later

//! What `SoundEngine::disconnectModulation` restores that audio cannot
//! show: disconnect zeroes the destination scale first, so leaving the
//! processor plugged, a switch on, the processor enabled or the poly output
//! active would not change a render. These are asserted directly against the
//! state before the connection; the processor's return to control rate is
//! asserted too.
//!
//! Upstream resets a destination's switches only when no other input is
//! connected (`mono connectedInputs() == 1`, `poly connectedInputs() == 0`).
//! An audio-rate poly destination is a `ModulationSum` whose reset input is
//! connected from the start, so its switches stay on, as
//! `updateAllModulationSwitches` leaves them, and its poly output stays in
//! the active non-accumulated set (upstream deactivates it only when the
//! switches reset). Restoring means returning to the state before the
//! connection, which for a mono-only destination is off.

use sas_dsp_core::ProcessorRef;
use sas_engine::sound_engine::SoundEngine;
use sas_engine::synth::Synth;
use sas_engine::synth_voice_handler::SynthVoiceHandler;
use sas_engine::voice_handler::with_voice_handler;
use sas_modulation::ModulationConnectionProcessor;

struct Snapshot {
    mono_inputs: usize,
    poly_inputs: Option<usize>,
    mono_switch: f32,
    poly_switch: Option<f32>,
    poly_active: Option<bool>,
}

fn voice_handler(synth: &Synth) -> ProcessorRef {
    synth
        .engine()
        .with::<SoundEngine, _>(|engine, _| engine.voice_handler().clone())
}

fn snapshot(synth: &Synth, destination: &str) -> Snapshot {
    let engine = synth.engine();
    let mono = engine.get_mono_modulation_destination(destination).unwrap();
    let poly = engine.get_poly_modulation_destination(destination);
    let svh = voice_handler(synth);
    Snapshot {
        mono_inputs: mono.connected_inputs(),
        poly_inputs: poly.as_ref().map(|p| p.connected_inputs()),
        mono_switch: engine
            .get_mono_modulation_switch(destination)
            .unwrap()
            .value(),
        poly_switch: engine
            .get_poly_modulation_switch(destination)
            .map(|s| s.value()),
        poly_active: poly.as_ref().map(|p| {
            with_voice_handler::<SynthVoiceHandler, _>(&svh, |vh, _| {
                vh.is_nonaccumulated_output_active(&p.output(0))
            })
        }),
    }
}

fn connection_processor(synth: &Synth, source: &str, destination: &str) -> ProcessorRef {
    voice_handler(synth).with::<SynthVoiceHandler, _>(|svh, _| {
        let bank = svh.modulation_bank();
        let index = (0..bank.num_connections())
            .find(|&i| {
                let connection = bank.at_index(i);
                connection.source_name == source && connection.destination_name == destination
            })
            .expect("connected slot");
        bank.at_index(index).processor.clone()
    })
}

#[test]
fn disconnect_restores_what_connect_changed() {
    let mut synth = Synth::new();
    synth.prepare(44100, 120.0);
    let mut saw_audio_rate = false;
    let mut saw_poly = false;

    for (source, destination) in [
        ("lfo_1", "filter_1_cutoff"),
        ("env_2", "osc_1_level"),
        ("velocity", "filter_1_cutoff"),
        ("macro_control_1", "volume"),
        ("lfo_1", "voice_tune"),
    ] {
        let before = snapshot(&synth, destination);
        assert!(synth.connect_modulation(source, destination));
        synth.process_modulation_changes();

        let processor = connection_processor(&synth, source, destination);
        let connected = snapshot(&synth, destination);
        assert!(processor.enabled(), "{source} -> {destination}");
        assert_eq!(connected.mono_switch, 1.0, "{source} -> {destination}");
        let polyphonic = processor.with::<ModulationConnectionProcessor, _>(|kernel, _| {
            kernel.is_polyphonic_modulation()
        });
        if polyphonic {
            saw_poly = true;
            assert_eq!(connected.poly_switch, Some(1.0));
            assert_eq!(connected.poly_active, Some(true));
            assert_eq!(connected.poly_inputs, before.poly_inputs.map(|n| n + 1));
        } else {
            assert_eq!(connected.mono_inputs, before.mono_inputs + 1);
        }
        saw_audio_rate |= !processor.is_control_rate();

        synth.disconnect_modulation(source, destination);
        synth.process_modulation_changes();

        let after = snapshot(&synth, destination);
        let label = format!("{source} -> {destination}");
        assert_eq!(
            after.mono_inputs, before.mono_inputs,
            "{label}: mono inputs"
        );
        assert_eq!(
            after.poly_inputs, before.poly_inputs,
            "{label}: poly inputs"
        );
        assert_eq!(
            after.mono_switch, before.mono_switch,
            "{label}: mono switch"
        );
        assert_eq!(
            after.poly_switch, before.poly_switch,
            "{label}: poly switch"
        );
        let reset = after.mono_switch == 0.0;
        let expected_active = if reset {
            before.poly_active.map(|_| false)
        } else {
            connected.poly_active
        };
        assert_eq!(after.poly_active, expected_active, "{label}: poly output");
        // With no other audio-rate connection from the source, upstream sets
        // the source's owner back to control rate. An LFO module sets its own
        // flag (`LfoModule::setControlRate`), so the restore shows on its
        // owner; an envelope module only forwards to its inner envelope, so
        // its owner's flag cannot show it.
        if source == "lfo_1" {
            let owner = synth
                .engine()
                .get_modulation_source(source)
                .unwrap()
                .owner()
                .unwrap();
            assert!(owner.is_control_rate(), "{label}: source owner rate");
        }
        assert!(!processor.enabled(), "{label}: processor enabled");
        assert!(processor.is_control_rate(), "{label}: processor rate");
    }
    let volume = snapshot(&synth, "volume");
    assert_eq!(volume.mono_switch, 0.0, "volume's switch is off again");
    // A control-rate poly destination has no reset input, so its switches
    // reset and its poly output leaves the active set.
    let tune = snapshot(&synth, "voice_tune");
    assert_eq!((tune.mono_switch, tune.poly_switch), (0.0, Some(0.0)));
    assert_eq!(tune.poly_active, Some(false));
    assert!(saw_poly, "no connection was polyphonic");
    assert!(saw_audio_rate, "no connection dropped to audio rate");
}
