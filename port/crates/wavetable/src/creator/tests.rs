use super::*;
use sas_common::{base64, json};

fn saw_data() -> String {
    let samples: Vec<f32> = (0..WAVEFORM_SIZE)
        .map(|i| i as f32 / 1024.0 - 1.0)
        .collect();
    base64::encode_f32s(&samples)
}

fn every_component_json() -> Value {
    let text = format!(
        r#"{{
  "name": "Every", "author": "Tests", "version": "1.0.6", "remove_all_dc": false, "full_normalize": true,
  "groups": [
    {{"components": [
      {{"type": "Wave Source", "interpolation": 0, "interpolation_style": 2,
        "keyframes": [{{"position": 0, "wave_data": "{saw}"}}, {{"position": 128, "wave_data": "{saw}"}}]}},
      {{"type": "Phase Shift", "style": 3, "interpolation_style": 1,
        "keyframes": [{{"position": 0, "phase": 0.5, "mix": 1}}, {{"position": 256, "phase": 3.0, "mix": 0.25}}]}},
      {{"type": "Wave Window", "window_shape": 4, "keyframes": [{{"position": 10, "left_position": 0.1, "right_position": 0.8}}]}},
      {{"type": "Frequency Filter", "style": 3, "normalize": false, "keyframes": [{{"position": 0, "cutoff": 2.5, "shape": 0.7}}]}},
      {{"type": "Slew Limiter", "keyframes": [{{"position": 0, "up_run_rise": 0.1, "down_run_rise": 0.2}}]}},
      {{"type": "Wave Folder", "keyframes": [{{"position": 0, "fold_boost": 2.0}}, {{"position": 200, "fold_boost": 6.5}}]}},
      {{"type": "Wave Warp", "horizontal_asymmetric": true, "vertical_asymmetric": false,
        "keyframes": [{{"position": 0, "horizontal_power": -3, "vertical_power": 4.25}}]}}
    ]}},
    {{"components": [
      {{"type": "Line Source", "num_points": 4, "keyframes": [
        {{"position": 0, "pull_power": 1.5, "line": {{"num_points": 3, "points": [0, 1, 0.5, 0, 1, 1], "powers": [0, 2, 0], "name": "V", "smooth": true}}}},
        {{"position": 100}}
      ]}}
    ]}},
    {{"components": [
      {{"type": "Audio File Source", "normalize_gain": true, "window_size": 600.5, "fade_style": 2, "phase_style": 2,
        "random_seed": 1234, "audio_sample_rate": 48000, "audio_file": "{audio}",
        "keyframes": [{{"position": 0, "start_position": 0, "window_fade": 0.5, "window_size": 600.5}},
                      {{"position": 256, "start_position": 20.25, "window_fade": 1.0, "window_size": 600.5}}]}}
    ]}},
    {{"components": [{{"type": "Shepard Tone Source", "interpolation": 1, "keyframes": [{{"position": 0, "wave_data": "{saw}"}}]}}]}}
  ]
}}"#,
        saw = saw_data(),
        audio = base64::encode_i16s(
            &(0..3000)
                .map(|i| ((i * 37) % 2000 - 1000) as i16)
                .collect::<Vec<_>>()
        ),
    );
    json::parse(&text).unwrap()
}

#[test]
fn round_trip_is_stable() {
    let data = every_component_json();
    let creator = WavetableCreator::from_json(&data).unwrap();
    let saved = creator.to_json();
    let reloaded = WavetableCreator::from_json(&saved).unwrap();
    assert_eq!(reloaded.to_json().dump(), saved.dump());

    let groups = saved.get("groups").unwrap();
    assert_eq!(groups.size(), 4);
    let line_keyframes = groups
        .at(1)
        .unwrap()
        .get("components")
        .unwrap()
        .at(0)
        .unwrap()
        .get("keyframes")
        .unwrap();
    let inherited = line_keyframes.at(1).unwrap().get("line").unwrap();
    assert_eq!(inherited.get("num_points").unwrap(), &Value::Int(3));
    assert_eq!(inherited.get("smooth").unwrap(), &Value::Bool(true));
    assert_eq!(
        line_keyframes.at(1).unwrap().get("pull_power").unwrap(),
        &Value::Float(0.0)
    );

    let file = groups
        .at(2)
        .unwrap()
        .get("components")
        .unwrap()
        .at(0)
        .unwrap();
    assert_eq!(file.get("normalize_mult").unwrap(), &Value::Bool(true));
    assert_eq!(file.get("random_seed").unwrap(), &Value::Int(1234));
    let audio = base64::decode_i16s(file.get("audio_file").unwrap().to_str().unwrap());
    assert_eq!(audio.len(), (20.25f64 + 2.0 * 600.5 + 4.0) as usize);
}

#[test]
fn missing_required_fields_fail() {
    let data = json::parse(r#"{"groups": [{"components": [{"type": "Wave Folder", "keyframes": [{"position": 0}]}]}]}"#)
        .unwrap();
    assert!(WavetableCreator::from_json(&data).is_err());
    let data = json::parse(r#"{"groups": [{"components": [{"type": "Nope"}]}]}"#).unwrap();
    assert!(WavetableCreator::from_json(&data).is_err());
}

#[test]
fn defaults_when_fields_missing() {
    let data = json::parse(r#"{"groups": []}"#).unwrap();
    let creator = WavetableCreator::from_json(&data).unwrap();
    assert!(!creator.full_normalize);
    assert!(!creator.remove_all_dc);
    assert_eq!(
        creator.to_json().dump(),
        r#"{"author":"","full_normalize":false,"groups":null,"name":"","remove_all_dc":false,"version":"1.0.6"}"#
    );
}

#[test]
fn predefined_waves() {
    let mut creator = WavetableCreator::new();
    creator.name = "Sub".to_string();
    creator.init_predefined_waves();
    let saved = creator.to_json();
    let component = saved
        .get("groups")
        .unwrap()
        .at(0)
        .unwrap()
        .get("components")
        .unwrap()
        .at(0)
        .unwrap();
    let positions: Vec<i64> = component
        .get("keyframes")
        .unwrap()
        .iter_values()
        .map(|k| k.get("position").unwrap().to_i64().unwrap())
        .collect();
    assert_eq!(positions, vec![0, 42, 85, 128, 171, 214]);
    assert_eq!(
        component.get("interpolation_style").unwrap(),
        &Value::Int(0)
    );
    assert_eq!(saved.get("name").unwrap().to_str().unwrap(), "Sub");
    assert_eq!(saved.get("full_normalize").unwrap(), &Value::Bool(false));
}

#[test]
fn line_generator_json_becomes_line_source() {
    let data = json::parse(
        r#"{"name": "Shape", "num_points": 2, "points": [0, 0, 1, 1], "powers": [0, 0]}"#,
    )
    .unwrap();
    let mut creator = WavetableCreator::new();
    creator.author = "Kept".to_string();
    creator.read_json(&data).unwrap();
    let saved = creator.to_json();
    assert_eq!(saved.get("name").unwrap().to_str().unwrap(), "Shape");
    assert_eq!(saved.get("author").unwrap().to_str().unwrap(), "Kept");
    let component = saved
        .get("groups")
        .unwrap()
        .at(0)
        .unwrap()
        .get("components")
        .unwrap()
        .at(0)
        .unwrap();
    assert_eq!(component.get("num_points").unwrap(), &Value::Int(4));
}

#[test]
fn old_versions_upgrade() {
    let data = json::parse(
        r#"{"version": "0.3.2", "name": "Old", "groups": [{"components": [
            {"type": 1, "num_points": 3, "keyframes": [{"position": 0, "points": [0.0, -1.0, 0.5, 1.0, 0.9, 0.0], "powers": [1, 0, 2]}]},
            {"type": 7, "keyframes": [{"position": 0, "fold_boost": 3}]}
        ]}]}"#,
    )
    .unwrap();
    let upgraded = WavetableCreator::update_json(data.clone()).unwrap();
    let components = upgraded
        .get("groups")
        .unwrap()
        .at(0)
        .unwrap()
        .get("components")
        .unwrap();
    assert_eq!(
        components
            .at(1)
            .unwrap()
            .get("type")
            .unwrap()
            .to_str()
            .unwrap(),
        "Wave Folder"
    );
    let line = components.at(0).unwrap();
    assert_eq!(line.get("num_points").unwrap(), &Value::Int(5));
    let converted = line
        .get("keyframes")
        .unwrap()
        .at(0)
        .unwrap()
        .get("line")
        .unwrap();
    assert_eq!(converted.get("num_points").unwrap(), &Value::Int(5));
    assert_eq!(upgraded.get("remove_all_dc").unwrap(), &Value::Bool(false));
    assert_eq!(upgraded.get("full_normalize").unwrap(), &Value::Bool(false));

    let creator = WavetableCreator::from_json(&data).unwrap();
    assert_eq!(creator.groups[0].components.len(), 2);
}

#[test]
fn boolean_values_follow_nlohmann_conversions() {
    let floats = json::parse(
        r#"{"version": "1.0.6", "groups": [{"components": [{"type": "Phase Shift", "style": 1,
            "keyframes": [{"position": true, "phase": true, "mix": false}]}]}]}"#,
    )
    .unwrap();
    let saved = WavetableCreator::from_json(&floats).unwrap().to_json();
    let keyframe = saved
        .get("groups")
        .unwrap()
        .at(0)
        .unwrap()
        .get("components")
        .unwrap()
        .at(0)
        .unwrap();
    let keyframe = keyframe.get("keyframes").unwrap().at(0).unwrap();
    assert_eq!(keyframe.get("position").unwrap(), &Value::Int(1));
    assert_eq!(keyframe.get("phase").unwrap(), &Value::Float(1.0));
    assert_eq!(keyframe.get("mix").unwrap(), &Value::Float(0.0));

    let enum_bool = json::parse(
        r#"{"version": "1.0.6", "groups": [{"components": [{"type": "Phase Shift", "style": true, "keyframes": []}]}]}"#,
    )
    .unwrap();
    assert!(WavetableCreator::from_json(&enum_bool).is_err());

    let double_bool = json::parse(
        r#"{"version": "1.0.6", "groups": [{"components": [{"type": "Audio File Source", "normalize_gain": false,
            "window_size": true, "random_seed": 1, "audio_file": "", "keyframes": []}]}]}"#,
    )
    .unwrap();
    assert!(WavetableCreator::from_json(&double_bool).is_err());
}

#[test]
fn render_init_and_predefined() {
    let mut wavetable = crate::Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    let mut creator = WavetableCreator::new();
    creator.init();
    creator.render(&mut wavetable).unwrap();
    assert_eq!(wavetable.num_frames(), 1);
    assert_eq!(wavetable.name, "Init");
    let frame = wavetable.buffer(0);
    let (min, max) = frame
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    assert!((max - min - 2.0).abs() < 1e-5, "normalised to a span of 2");

    creator.init_predefined_waves();
    creator.render(&mut wavetable).unwrap();
    assert_eq!(wavetable.num_frames(), 215);
    assert!(!wavetable.is_shepard_table());
    let sin_frame = wavetable.frequency_amplitudes(0);
    assert!(
        sin_frame[2] > 100.0 && sin_frame[4] < 1e-3,
        "only the fundamental"
    );
}

#[test]
fn render_rejects_out_of_range_positions() {
    let data = json::parse(
        r#"{"version": "1.0.6", "groups": [{"components": [{"type": "Wave Folder", "keyframes": [{"position": 300, "fold_boost": 1}]}]}]}"#,
    )
    .unwrap();
    let mut creator = WavetableCreator::from_json(&data).unwrap();
    let mut wavetable = crate::Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    assert!(creator.render(&mut wavetable).is_err());
}

#[test]
fn render_to_buffer_normalises_down_only() {
    let mut creator = WavetableCreator::new();
    creator.init();
    let buffer = creator.render_to_buffer(4);
    assert_eq!(buffer.len(), 4 * WAVEFORM_SIZE);
    let peak = buffer.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    assert!(peak <= 1.0 && peak > 0.99);
}

#[test]
fn shepard_tables_are_flagged() {
    let saw = saw_data();
    let data = json::parse(&format!(
        r#"{{"version": "1.0.6", "groups": [{{"components": [{{"type": "Shepard Tone Source", "interpolation": 1, "keyframes": [{{"position": 0, "wave_data": "{saw}"}}]}}]}}]}}"#
    ))
    .unwrap();
    let mut creator = WavetableCreator::from_json(&data).unwrap();
    let mut wavetable = crate::Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    creator.render(&mut wavetable).unwrap();
    assert!(wavetable.is_shepard_table());
    assert_eq!(wavetable.num_frames(), NUM_OSCILLATOR_WAVE_FRAMES as usize);
}

#[test]
fn spliced_audio_import() {
    let audio: Vec<f32> = (0..20_000)
        .map(|i| ((i % 400) as f32 / 200.0 - 1.0) * 0.5)
        .collect();
    let mut creator = WavetableCreator::new();
    creator
        .init_from_audio_file(
            &audio,
            44100,
            AudioFileLoadStyle::WavetableSplice,
            component::FADE_WAVE_BLEND,
        )
        .unwrap();
    let mut wavetable = crate::Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    creator.render(&mut wavetable).unwrap();
    assert_eq!(wavetable.num_frames(), NUM_OSCILLATOR_WAVE_FRAMES as usize);
    let saved = creator.to_json();
    let file = saved
        .get("groups")
        .unwrap()
        .at(0)
        .unwrap()
        .get("components")
        .unwrap()
        .at(0)
        .unwrap();
    assert_eq!(
        file.get("type").unwrap().to_str().unwrap(),
        "Audio File Source"
    );
}

/// The Wave Source overlay's interpolation selector: none, then the
/// waveform and spectral blends, linear then cubic.
#[test]
fn the_interpolation_selector_maps_to_style_and_mode() {
    use super::component::{ComponentType, WavetableComponent};
    use super::edits::set_wave_source_interpolation;
    let mut creator = WavetableCreator::new();
    creator.init();
    let expected = [(0, 0), (1, 0), (1, 1), (2, 0), (2, 1)];
    for (value, &(style, mode)) in expected.iter().enumerate() {
        set_wave_source_interpolation(&mut creator, 0, 0, value as i32);
        let WavetableComponent::WaveSource(source) = &creator.groups[0].components[0] else {
            panic!("the init creator starts with a wave source");
        };
        assert_eq!(source.keyframes.interpolation_style, style, "value {value}");
        assert_eq!(source.interpolation_mode, mode, "value {value}");
        assert_eq!(source.compute_frame.interpolation_mode, mode);
    }
    // Other components are left alone.
    let mut group = super::group::WavetableGroup::new();
    group.add_component(WavetableComponent::new(ComponentType::LineSource));
    creator.add_group(group);
    let before = creator.to_json().dump();
    set_wave_source_interpolation(&mut creator, 1, 0, 3);
    assert_eq!(creator.to_json().dump(), before);
}

/// A File Source's default seed (A13 in
/// `migration/decisions/reference-arm.md`): for the draw behind
/// `wavetable/edit/add_sources`' seed, the steps rounded one by one give
/// the no-fast-math and x86 seed, and Apple clang's fast-math fold
/// (`float(u) + -2^31f`, truncated) gives the macOS official one. Each
/// target takes its own path, and the generic path is the old expression.
#[test]
fn file_source_seeds_follow_their_targets_compiler() {
    use component::{default_random_seed, folded_random_seed, generic_random_seed};
    let draw = 2_357_136_128;
    assert_eq!(generic_random_seed(draw), 209_652_560);
    assert_eq!(folded_random_seed(draw), 209_652_480);
    let expected = if cfg!(target_vendor = "apple") {
        209_652_480
    } else {
        209_652_560
    };
    assert_eq!(default_random_seed(draw), expected);

    // The fold saturates as `fcvtzs` does: draws that round to 2^32 give
    // `i32::MAX`, and 0 gives `i32::MIN`.
    assert_eq!(folded_random_seed(u32::MAX), i32::MAX);
    assert_eq!(folded_random_seed(0), i32::MIN);

    // The generic path is `next() * (INT_MAX / kPi)`, as before.
    let mut generator = sas_common::random::RandomGenerator::new(-scalar::PI, scalar::PI, 7);
    let mut draws = generator.clone();
    for _ in 0..1000 {
        let value = generator.next();
        assert_eq!(
            generic_random_seed(draws.next_draw()),
            (value * (i32::MAX as f32 / scalar::PI)) as i32
        );
    }
}
