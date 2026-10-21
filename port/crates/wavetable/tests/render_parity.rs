//! Parity of rendered wavetables (time-domain frames and the oscillator's
//! harmonic coefficients) against
//! `migration/corpus/modules/wavetable/render.json`.

mod support;

use sas_common::json;
use sas_parity::{assert_parity, ModuleCase};
use sas_wavetable::creator::WavetableCreator;
use sas_wavetable::wave_frame::DEFAULT_SAMPLE_RATE;
use sas_wavetable::wavetable::{FREQUENCY_FLOATS, NUM_HARMONICS};
use sas_wavetable::{Wavetable, NUM_OSCILLATOR_WAVE_FRAMES};

struct Rendered {
    wave: Vec<f32>,
    coefficients: Vec<f32>,
    phase_coefficients: Vec<f32>,
    lanes: Vec<f32>,
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
            let (sin, cos) = sas_common::scalar::sin_cos(phases[2 * h]);
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

#[test]
fn wavetable_render() {
    for id in support::case_ids("wavetable/render/") {
        let case = ModuleCase::load(&id);
        let rendered = render(&case);
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
