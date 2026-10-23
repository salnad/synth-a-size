// SPDX-License-Identifier: GPL-3.0-or-later

//! Live-edit parity: render cases with an edit script (`ref-render --edits`)
//! played through the plugin as an editor drives it, with modulation
//! routing, remap curves, LFO shapes, wavetables and the sample sent over
//! the edit bus (`SynthPlugin::edit_handle`), and controls set as host
//! parameters. Each edit is made before the first host block that starts at
//! or after its time, as `ref-render` applies it, and the render must match
//! the reference at the case's tolerances.
//!
//! The engine RNG's seed counter is process-wide, so the cases render one at
//! a time.

use std::path::Path;
use std::sync::Mutex;

use nice_plug::params::InternalParamMut;
use nice_plug::prelude::NoteEvent;
use sas_common::json;
use sas_engine::midi_file::read_midi_file;
use sas_engine::sas_oscillators::RandomValues;
use sas_parity::RenderCase;
use sas_plugin::params::ControlParam;
use sas_plugin::{state, SoundEngineHost, SynthASize, SynthPlugin, Transport};
use sas_preset::{registry, SampleState, SlotValues};

/// Every render case in `migration/corpus/cases.json` with an edit script.
const CASES: [&str; 2] = [
    "render/init_saw__live_edits",
    "render/init_saw__live_edits_48k_b512",
];

static ENGINE_RNG: Mutex<()> = Mutex::new(());

struct TimedEdit {
    sample: usize,
    edit: serde_json::Value,
}

/// The script's edits at `floor(seconds * rate + 0.5)`, as `ref-render`
/// times them.
fn read_edits(path: &Path, sample_rate: u32) -> Vec<TimedEdit> {
    let text = std::fs::read_to_string(path).unwrap();
    let script: serde_json::Value = serde_json::from_str(&text).unwrap();
    script["edits"]
        .as_array()
        .expect("an edit script has an edits array")
        .iter()
        .map(|edit| {
            let seconds = edit["seconds"].as_f64().unwrap();
            TimedEdit {
                sample: (seconds * sample_rate as f64 + 0.5).floor() as usize,
                edit: edit.clone(),
            }
        })
        .collect()
}

fn set_param(plugin: &SynthASize, name: &str, value: f32) {
    let control = plugin
        .synth_params()
        .control(name)
        .unwrap_or_else(|| panic!("unknown control {name}"));
    // SAFETY: as a host sets a parameter between blocks; nothing else is
    // setting it.
    unsafe {
        match &control.param {
            ControlParam::Float(param) => {
                param._internal_set_plain_value(value);
            }
            ControlParam::Stepped(param) => {
                param._internal_set_plain_value(value.round() as i32);
            }
            ControlParam::Bypass(param) => {
                param._internal_set_plain_value(value != 0.0);
            }
        }
    }
}

fn sas_json(value: &serde_json::Value) -> json::Value {
    json::parse(&value.to_string()).unwrap()
}

/// One edit, as the editor makes it: the bus for routing, shapes,
/// wavetables and the sample (loaded onto the current object, as upstream's
/// `jsonToState` loads onto the engine's), host parameters for controls.
fn apply(plugin: &SynthASize, edit: &serde_json::Value) {
    let edits = plugin.edit_handle();
    let text = |key: &str| edit[key].as_str().unwrap();
    let index = |key: &str| edit[key].as_u64().unwrap() as usize;
    match text("type") {
        "set_control" => set_param(plugin, text("name"), edit["value"].as_f64().unwrap() as f32),
        "connect_modulation" => {
            // Dropped on the destination's control at its live value.
            let destination = text("destination");
            let info = registry().get(destination).unwrap();
            let value = plugin
                .synth_params()
                .control(destination)
                .unwrap()
                .param
                .value();
            let bipolar = edit["bipolar"].as_bool().unwrap_or(false);
            let drag = SlotValues::dragged_to(info, value, bipolar);
            let connection = edits
                .connect_modulation(text("source"), destination, drag)
                .unwrap();
            for (name, value) in connection.parameter_changes {
                set_param(plugin, name, value);
            }
        }
        "disconnect_modulation" => {
            edits
                .disconnect_modulation(text("source"), text("destination"))
                .unwrap();
        }
        "line_mapping" => {
            let slot = index("slot");
            let mut line = edits
                .with_document(|document| document.state.modulations()[slot].line_mapping.clone());
            line.from_json(&sas_json(&edit["line"])).unwrap();
            edits.set_line_mapping(slot, line).unwrap();
        }
        "lfo_shape" => {
            let lfo = index("lfo");
            let mut line = edits.with_document(|document| document.state.lfos[lfo].clone());
            line.from_json(&sas_json(&edit["line"])).unwrap();
            edits.set_lfo_shape(lfo, line).unwrap();
        }
        "wavetable" => {
            let oscillator = index("oscillator");
            let mut creator =
                edits.with_document(|document| document.state.wavetables[oscillator].clone());
            creator.read_json(&sas_json(&edit["wavetable"])).unwrap();
            edits.set_wavetable(oscillator, creator).unwrap();
        }
        "sample" => {
            let sample = SampleState::from_json(&sas_json(&edit["data"])).unwrap();
            edits.set_sample(sample).unwrap();
        }
        other => panic!("unknown edit type {other}"),
    }
}

/// Renders `case` through the plugin, applying its edit script if `edits`.
fn render(case: &RenderCase, edits: bool) -> Vec<f32> {
    let _engine_rng = ENGINE_RNG
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sas_common::random::set_next_seed(case.seed().unwrap_or(0) as i32);
    RandomValues::reset_process_state();

    let sample_rate = case.sample_rate();
    let mut plugin: SynthASize = SynthPlugin::new(SoundEngineHost::new());
    plugin
        .synth_params()
        .restore_state(state::plugin_state_from_file(&case.preset_path()).unwrap());
    let params = plugin.synth_params().clone();
    plugin.processor_mut().activate(&params, sample_rate as f32);
    let transport = Transport {
        tempo: Some(case.bpm()),
        playing: false,
        pos_beats: None,
    };

    let script = match (edits, case.edits_path()) {
        (true, Some(path)) => read_edits(&path, sample_rate),
        (true, None) => panic!("{} has no edit script", case.id()),
        (false, _) => Vec::new(),
    };
    let midi = read_midi_file(&std::fs::read(case.midi_path()).unwrap()).unwrap();
    let events: Vec<(usize, [u8; 3])> = midi
        .iter()
        .map(|event| {
            let sample = (event.seconds * sample_rate as f64 + 0.5).floor() as usize;
            (sample, event.message)
        })
        .collect();

    let frames = case.frames();
    let block_size = case.block_size();
    let mut out = Vec::with_capacity(frames * 2);
    let (mut left, mut right) = (vec![0.0f32; block_size], vec![0.0f32; block_size]);
    let (mut next_event, mut next_edit) = (0, 0);
    let mut start = 0;
    while start < frames {
        let n = block_size.min(frames - start);
        while next_edit < script.len() && script[next_edit].sample <= start {
            apply(&plugin, &script[next_edit].edit);
            next_edit += 1;
        }
        let mut block_events: Vec<NoteEvent<()>> = Vec::new();
        while next_event < events.len() && events[next_event].0 < start + n {
            let (sample, message) = events[next_event];
            let timing = sample.saturating_sub(start) as u32;
            block_events.push(NoteEvent::from_midi(timing, &message).unwrap());
            next_event += 1;
        }
        block_events.reverse();
        plugin.processor_mut().process(
            &params,
            &mut left[..n],
            &mut right[..n],
            &transport,
            || block_events.pop(),
        );
        for i in 0..n {
            out.push(left[i]);
            out.push(right[i]);
        }
        start += n;
    }
    assert_eq!(
        next_edit,
        script.len(),
        "every edit falls inside the render"
    );
    out
}

#[test]
fn live_edits_match_reference() {
    for id in CASES {
        let case = RenderCase::load(id);
        case.assert_parity(&render(&case, true)[..]);
    }
}

/// The edits are audible: without them the same render is far from the
/// reference.
#[test]
fn the_edits_change_the_render() {
    let case = RenderCase::load(CASES[0]);
    let edited = render(&case, true);
    let plain = render(&case, false);
    let energy = |signal: &[f32]| signal.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>();
    let difference: Vec<f32> = edited.iter().zip(&plain).map(|(a, b)| a - b).collect();
    let ratio_db = 10.0 * (energy(&difference) / energy(&edited)).log10();
    assert!(
        ratio_db > -20.0,
        "the edits barely change the render ({ratio_db:.1} dB)"
    );
}
