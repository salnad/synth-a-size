//! Parity of wavetable creator JSON (load -> save and `updateJson`, byte for
//! byte) and of the `init`/predefined keyframe waveforms (with tolerances)
//! against `migration/corpus/modules/wavetable/creator_json.json`.

mod support;

use sas_common::json;
use sas_parity::{assert_artifact, assert_parity, ModuleCase};
use sas_wavetable::creator::{WavetableComponent, WavetableCreator};
use sas_wavetable::wave_frame::WAVEFORM_SIZE;

#[test]
fn creator_json() {
    for id in support::case_ids("wavetable/creator_json/") {
        let case = ModuleCase::load(&id);
        let params = case.params();
        if let Some(text) = params["creator"].as_str() {
            let input = support::expand(&json::parse(text).expect("creator JSON"));
            let upgraded = WavetableCreator::update_json(input.clone()).expect("upgrades");
            assert_artifact!(&id, "upgraded.json", upgraded.dump());
            let creator = WavetableCreator::from_json(&input).expect("loads");
            assert_artifact!(&id, "saved.json", creator.to_json().dump());
            continue;
        }

        let mut creator = WavetableCreator::new();
        match params["preset"].as_str() {
            Some("init") => creator.init(),
            Some("predefined") => {
                creator.init_predefined_waves();
                creator.name = "Sub".to_string();
            }
            other => panic!("unknown preset {other:?}"),
        }
        let WavetableComponent::WaveSource(source) = &creator.groups[0].components[0] else {
            panic!("preset starts with a wave source");
        };
        let waves: Vec<f32> = source
            .keyframes
            .keyframes
            .iter()
            .flat_map(|k| k.data.wave_frame.time_domain[..WAVEFORM_SIZE].to_vec())
            .collect();
        assert_parity!(&format!("{id}:keyframe_waves"), waves);
        assert_artifact!(&id, "saved.json", creator.to_json().dump());
    }
}
