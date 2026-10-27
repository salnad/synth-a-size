//! Parity of rendered wavetables (time-domain frames and the oscillator's
//! harmonic coefficients) against
//! `migration/corpus/modules/wavetable/render.json`, and of the wavetable
//! editor's edits (the saved creator and its render) against `edit.json`.

mod support;

use sas_common::json;
use sas_parity::{assert_artifact, assert_parity, ModuleCase};
use sas_wavetable::creator::component::ComponentType;
use sas_wavetable::creator::edits::{self, KeyframeIndex, MODIFIER_TYPES, SOURCE_TYPES};
use sas_wavetable::creator::WavetableCreator;
use sas_wavetable::wave_frame::DEFAULT_SAMPLE_RATE;
use sas_wavetable::wavetable::{FREQUENCY_FLOATS, NUM_HARMONICS};
use sas_wavetable::{Wavetable, NUM_OSCILLATOR_WAVE_FRAMES};
use serde_json::Value;

struct Rendered {
    wave: Vec<f32>,
    coefficients: Vec<f32>,
    phase_coefficients: Vec<f32>,
    lanes: Vec<f32>,
    info: Vec<f32>,
}

/// The case's starting creator: `params.creator` or `params.preset`.
fn creator(case: &ModuleCase) -> WavetableCreator {
    let params = case.params();
    let mut creator = WavetableCreator::new();
    if let Some(text) = params["creator"].as_str() {
        let input = support::expand(&json::parse(text).expect("creator JSON"));
        creator.read_json(&input).expect("loads");
    } else {
        match params["preset"].as_str() {
            Some("init") => creator.init(),
            Some("predefined") => creator.init_predefined_waves(),
            other => panic!("unknown preset {other:?}"),
        }
    }
    creator
}

/// The arrays `migration/reference/src/modules/wavetable/render.cpp` (and
/// `render_arrays.h`) emit for `creator`.
fn render(creator: &mut WavetableCreator) -> Rendered {
    let mut wavetable = Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    creator.render(&mut wavetable).expect("renders");

    let data = wavetable.data();
    let mut rendered = Rendered {
        wave: Vec::new(),
        coefficients: Vec::new(),
        phase_coefficients: Vec::new(),
        lanes: vec![0.0; 3],
        info: vec![
            data.num_frames as f32 / NUM_OSCILLATOR_WAVE_FRAMES as f32,
            data.frequency_ratio,
            data.sample_rate / DEFAULT_SAMPLE_RATE,
            if wavetable.is_shepard_table() {
                1.0
            } else {
                0.0
            },
        ],
    };
    for w in 0..data.num_frames {
        rendered.wave.extend_from_slice(&data.wave_data[w]);
        let (amplitudes, normalized, phases) = (
            &data.frequency_amplitudes[w],
            &data.normalized_frequencies[w],
            &data.phases[w],
        );
        for h in 0..NUM_HARMONICS {
            let amplitude = amplitudes[2 * h] / 1024.0;
            rendered.coefficients.extend([
                amplitude * normalized[2 * h],
                amplitude * normalized[2 * h + 1],
            ]);
            // The capture's own cosf/sinf pair (render.cpp), which the
            // no-fast-math reference evaluates as separate calls.
            let (sin, cos) = sas_common::scalar::sin_cos_unmerged(phases[2 * h]);
            rendered
                .phase_coefficients
                .extend([amplitude * cos, amplitude * sin]);
            rendered.lanes[0] += f32::from(u8::from(
                amplitudes[2 * h + 1].to_bits() != amplitudes[2 * h].to_bits(),
            ));
            rendered.lanes[1] += f32::from(u8::from(
                phases[2 * h + 1].to_bits() != phases[2 * h].to_bits(),
            ));
        }
        for i in 2 * NUM_HARMONICS..FREQUENCY_FLOATS {
            for array in [amplitudes, normalized, phases] {
                rendered.lanes[2] += f32::from(u8::from(array[i].to_bits() != 0));
            }
        }
    }
    rendered
}

/// The phase modifier's `std::polar` pair is merged into `__sincosf_stret`
/// in the official macOS build but evaluated as separate calls in the
/// no-fast-math one (A11). The port follows the official build there, so a
/// case compared bit for bit against the no-fast-math reference must not
/// contain a phase modifier.
fn check_no_phase_modifier_in_nofastmath_case(case: &ModuleCase) {
    if case.comparison().reference != sas_parity::cases::Reference::NoFastMath {
        return;
    }
    let creator = case.params()["creator"].as_str().unwrap_or_default();
    assert!(
        !creator.contains("\"Phase Shift\""),
        "{}: a \"reference\": \"nofastmath\" case cannot contain a Phase Shift \
         modifier; the macOS builds evaluate its sin/cos pair differently (A11)",
        case.id()
    );
}

#[test]
fn wavetable_render() {
    for id in support::case_ids("wavetable/render/") {
        let case = ModuleCase::load(&id);
        check_no_phase_modifier_in_nofastmath_case(&case);
        let rendered = render(&mut creator(&case));
        assert_parity!(&format!("{id}:info"), rendered.info);
        assert_parity!(&format!("{id}:lanes"), rendered.lanes);
        assert_parity!(&format!("{id}:wave"), rendered.wave);
        assert_parity!(&format!("{id}:coefficients"), rendered.coefficients);
        assert_parity!(
            &format!("{id}:phase_coefficients"),
            rendered.phase_coefficients
        );
    }
}

fn index(value: &Value) -> usize {
    value.as_i64().expect("an index") as usize
}

fn keyframe(value: &Value) -> KeyframeIndex {
    KeyframeIndex {
        group: index(&value[0]),
        component: index(&value[1]),
        keyframe: index(&value[2]),
    }
}

/// One of `edit.cpp`'s edits, through `creator::edits` as the editor makes
/// it.
fn apply_edit(creator: &mut WavetableCreator, edit: &Value) {
    let group = edit["group"].as_i64().unwrap_or(-1) as usize;
    let component = edit["component"].as_i64().unwrap_or(-1) as usize;
    let kind = |types: &[ComponentType]| types[index(&edit["type"])];
    match edit["op"].as_str().expect("an op") {
        "add_source" => edits::add_source(creator, kind(&SOURCE_TYPES)),
        "add_modifier" => edits::add_modifier(creator, group, kind(&MODIFIER_TYPES)),
        "remove_group" => edits::remove_group(creator, group),
        "remove_component" => edits::remove_component(creator, group, component),
        "reset_component" => edits::reset_component(creator, group, component),
        "move_group_up" => edits::move_group_up(creator, group),
        "move_group_down" => edits::move_group_down(creator, group),
        "move_modifier_up" => edits::move_modifier_up(creator, group, component),
        "move_modifier_down" => edits::move_modifier_down(creator, group, component),
        "create_keyframe" => {
            let position = edit["position"].as_i64().expect("a position") as i32;
            edits::create_keyframe(creator, group, component, position);
        }
        "delete_keyframe" => edits::delete_keyframe(
            creator,
            KeyframeIndex {
                group,
                component,
                keyframe: index(&edit["keyframe"]),
            },
        ),
        "drag_keyframes" => {
            let selected: Vec<KeyframeIndex> = edit["selected"]
                .as_array()
                .expect("selected keyframes")
                .iter()
                .map(keyframe)
                .collect();
            let dragged = keyframe(&edit["dragged"]);
            for target in edit["targets"].as_array().expect("targets") {
                let target = target.as_i64().expect("a target") as i32;
                edits::drag_keyframes(creator, &selected, dragged, target);
            }
            edits::finish_keyframe_drag(creator, &selected);
        }
        other => panic!("unknown edit {other}"),
    }
}

#[test]
fn wavetable_edit() {
    for id in support::case_ids("wavetable/edit/") {
        let case = ModuleCase::load(&id);
        let mut creator = creator(&case);
        // Upstream's jsonToState and init render, which leaves the compute
        // frames that a keyframe created with no keyframes left copies.
        creator
            .render(&mut Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize))
            .expect("renders");
        // edit.cpp sets the generator counter before the edits.
        let seed = case.params()["seed"].as_i64().unwrap_or(0) as i32;
        sas_common::random::use_thread_seed_counter(seed);
        for edit in case.params()["edits"].as_array().expect("edits") {
            apply_edit(&mut creator, edit);
        }
        let rendered = render(&mut creator);
        assert_artifact!(&id, "saved.json", creator.to_json().dump());
        assert_parity!(&format!("{id}:info"), rendered.info);
        assert_parity!(&format!("{id}:lanes"), rendered.lanes);
        assert_parity!(&format!("{id}:wave"), rendered.wave);
        assert_parity!(&format!("{id}:coefficients"), rendered.coefficients);
        assert_parity!(
            &format!("{id}:phase_coefficients"),
            rendered.phase_coefficients
        );
    }
}
