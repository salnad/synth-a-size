// SPDX-License-Identifier: GPL-3.0-or-later

//! MIDI learn (`MidiManager::armMidiLearn` / `midiInput` and the controller
//! dispatch in `processMidiMessage`), through `Synth`.

use sas_engine::synth::Synth;

fn control(synth: &Synth, name: &str) -> f32 {
    synth.engine().get_controls()[name].value()
}

fn cc(synth: &mut Synth, controller: u8, value: u8) {
    synth.process_midi([0xb0, controller, value], 0);
}

fn changes(synth: &mut Synth) -> Vec<(&'static str, f32)> {
    let mut out = Vec::new();
    synth.drain_midi_value_changes(|name, value| out.push((name, value)));
    out
}

fn learned_ccs(synth: &Synth, name: &str) -> Vec<i32> {
    synth
        .midi_learn_map()
        .iter()
        .filter(|(_, controls)| controls.contains_key(name))
        .map(|(cc, _)| *cc)
        .collect()
}

#[test]
fn an_armed_parameter_learns_the_next_controller_and_follows_it() {
    let mut synth = Synth::new();
    assert!(!synth.is_midi_mapped("filter_1_cutoff"));
    synth.arm_midi_learn("filter_1_cutoff");
    assert!(!synth.take_midi_learn_changed());

    cc(&mut synth, 20, 64);
    assert!(synth.is_midi_mapped("filter_1_cutoff"));
    assert_eq!(learned_ccs(&synth, "filter_1_cutoff"), [20]);
    assert!(synth.take_midi_learn_changed());
    assert!(!synth.take_midi_learn_changed());
    // midiInput: value / 127 of the range [8, 136], from the minimum.
    let expected = 64.0 / 127.0 * (136.0 - 8.0) + 8.0;
    assert_eq!(control(&synth, "filter_1_cutoff"), expected);
    assert_eq!(changes(&mut synth), [("filter_1_cutoff", expected)]);

    // Disarmed: the next controller follows the map and learns nothing.
    cc(&mut synth, 20, 127);
    assert_eq!(control(&synth, "filter_1_cutoff"), 136.0);
    cc(&mut synth, 21, 0);
    assert_eq!(control(&synth, "filter_1_cutoff"), 136.0);
    assert!(!synth.take_midi_learn_changed());
    assert_eq!(changes(&mut synth), [("filter_1_cutoff", 136.0)]);
}

#[test]
fn indexed_parameters_round() {
    let mut synth = Synth::new();
    synth.arm_midi_learn("osc_1_unison_voices");
    cc(&mut synth, 30, 64);
    // 64 / 127 * 15 + 1 = 8.56.
    assert_eq!(control(&synth, "osc_1_unison_voices"), 9.0);
}

#[test]
fn one_controller_sets_several_parameters_and_a_parameter_several_controllers() {
    let mut synth = Synth::new();
    synth.arm_midi_learn("macro_control_2");
    cc(&mut synth, 5, 0);
    synth.arm_midi_learn("macro_control_1");
    cc(&mut synth, 5, 0);
    synth.arm_midi_learn("macro_control_1");
    cc(&mut synth, 6, 0);
    changes(&mut synth);

    cc(&mut synth, 5, 127);
    // In name order, as upstream's std::map iterates.
    assert_eq!(
        changes(&mut synth),
        [("macro_control_1", 1.0), ("macro_control_2", 1.0)]
    );
    assert_eq!(learned_ccs(&synth, "macro_control_1"), [5, 6]);

    assert!(synth.clear_midi_learn("macro_control_1"));
    assert!(!synth.is_midi_mapped("macro_control_1"));
    assert!(synth.is_midi_mapped("macro_control_2"));
    assert!(!synth.clear_midi_learn("macro_control_1"));
    cc(&mut synth, 6, 0);
    assert_eq!(control(&synth, "macro_control_1"), 1.0);
}

#[test]
fn cancel_and_unknown_names_learn_nothing() {
    let mut synth = Synth::new();
    synth.arm_midi_learn("macro_control_1");
    synth.cancel_midi_learn();
    cc(&mut synth, 7, 100);
    assert!(synth.midi_learn_map().is_empty());

    synth.arm_midi_learn("not_a_parameter");
    cc(&mut synth, 7, 100);
    assert!(synth.midi_learn_map().is_empty());
    assert!(!synth.take_midi_learn_changed());
}

#[test]
fn bank_folder_and_all_notes_off_are_not_learnable() {
    // processMidiMessage returns before midiInput for these.
    for controller in [0x00, 0x20, 0x79, 0x7b] {
        let mut synth = Synth::new();
        synth.arm_midi_learn("macro_control_1");
        cc(&mut synth, controller, 100);
        assert!(synth.midi_learn_map().is_empty(), "CC {controller}");
    }
}

#[test]
fn handled_controllers_still_reach_midi_learn() {
    // Mod wheel, sustain, sostenuto, soft pedal, slide, the LSBs and all
    // sounds off break out of the inner switch and fall through to
    // midiInput.
    for controller in [0x01, 0x40, 0x42, 0x43, 0x4a, 0x66, 0x6a, 0x78] {
        let mut synth = Synth::new();
        synth.arm_midi_learn("macro_control_3");
        cc(&mut synth, controller, 127);
        assert_eq!(
            learned_ccs(&synth, "macro_control_3"),
            [controller as i32],
            "CC {controller}"
        );
        assert_eq!(control(&synth, "macro_control_3"), 1.0, "CC {controller}");
    }
}

#[test]
fn the_map_can_be_replaced() {
    let mut synth = Synth::new();
    synth.arm_midi_learn("macro_control_4");
    cc(&mut synth, 9, 0);
    let map = synth.midi_learn_map().clone();

    let mut other = Synth::new();
    other.set_midi_learn_map(map);
    assert!(other.is_midi_mapped("macro_control_4"));
    cc(&mut other, 9, 127);
    assert_eq!(control(&other, "macro_control_4"), 1.0);
}
