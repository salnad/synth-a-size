//! Parity of rendered wavetables (time-domain frames and the oscillator's
//! harmonic coefficients) against
//! `migration/corpus/modules/wavetable/render.json`.

mod support;

use sas_common::json;
use sas_parity::{assert_parity, ModuleCase};
use sas_wavetable::creator::WavetableCreator;
use sas_wavetable::wave_frame::DEFAULT_SAMPLE_RATE;
use sas_wavetable::wavetable::NUM_HARMONICS;
use sas_wavetable::{Wavetable, NUM_OSCILLATOR_WAVE_FRAMES};

struct Rendered {
    wave: Vec<f32>,
    coefficients: Vec<f32>,
    phase_coefficients: Vec<f32>,
    info: Vec<f32>,
}

/// The arrays `migration/reference/src/modules/wavetable/render.cpp` emits.
fn render(case: &ModuleCase) -> Rendered {
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
    let mut wavetable = Wavetable::new(NUM_OSCILLATOR_WAVE_FRAMES as usize);
    creator.render(&mut wavetable).expect("renders");

    let data = wavetable.data();
    let mut rendered = Rendered {
        wave: Vec::new(),
        coefficients: Vec::new(),
        phase_coefficients: Vec::new(),
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
            rendered.phase_coefficients.extend([
                amplitude * phases[2 * h].cos(),
                amplitude * phases[2 * h].sin(),
            ]);
        }
    }
    rendered
}

#[test]
fn wavetable_render() {
    for id in support::case_ids("wavetable/render/") {
        let case = ModuleCase::load(&id);
        let rendered = render(&case);
        assert_parity!(&format!("{id}:info"), rendered.info);
        assert_parity!(&format!("{id}:wave"), rendered.wave);
        assert_parity!(&format!("{id}:coefficients"), rendered.coefficients);
        assert_parity!(
            &format!("{id}:phase_coefficients"),
            rendered.phase_coefficients
        );
    }
}
