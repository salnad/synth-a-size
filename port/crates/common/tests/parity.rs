//! Parity tests against the reference captures in
//! `migration/corpus/modules/common/*.json` (modules registered in
//! `migration/reference/src/modules/common/`). Numeric outputs are compared
//! with the case tolerances; saved JSON is compared byte for byte.

use sas_common::random::RandomGenerator;
use sas_common::{json, scalar, LineGenerator};
use sas_parity::{assert_artifact, assert_parity, ModuleCase};

fn case_ids(prefix: &str) -> Vec<String> {
    let ids: Vec<String> = sas_parity::all_case_ids()
        .into_iter()
        .filter(|id| id.starts_with(prefix))
        .collect();
    assert!(!ids.is_empty(), "no cases under {prefix}");
    ids
}

#[test]
fn line_generator() {
    for id in case_ids("common/line_generator/") {
        let case = ModuleCase::load(&id);
        let params = case.params();
        let resolution = params["resolution"]
            .as_u64()
            .unwrap_or(LineGenerator::DEFAULT_RESOLUTION as u64) as usize;
        let mut generator = LineGenerator::new(resolution);
        let line = json::parse(params["line"].as_str().expect("line text")).expect("line JSON");
        generator.from_json(&line).expect("line loads");
        if params["loop"].as_bool().unwrap_or(false) {
            generator.set_loop(true);
        }

        assert_parity!(
            &format!("{id}:buffer"),
            generator.cubic_interpolation_buffer().to_vec()
        );
        let values: Vec<f32> = (0..=40)
            .map(|i| generator.value_at_phase(i as f32 / 40.0))
            .collect();
        assert_parity!(&format!("{id}:value_at_phase"), values);
        assert_artifact!(&id, "saved.json", generator.to_json().dump());
    }
}

#[test]
fn random_generator() {
    for id in case_ids("common/random/") {
        let case = ModuleCase::load(&id);
        let params = case.params();
        let (min, max) = if params["range"].as_str() == Some("pi") {
            (-scalar::PI, scalar::PI)
        } else {
            (
                params["min"].as_f64().unwrap_or(-1.0) as f32,
                params["max"].as_f64().unwrap_or(1.0) as f32,
            )
        };
        let mut generator = RandomGenerator::new(min, max, 0);
        generator.seed(params["seed"].as_i64().expect("seed") as i32);
        let count = params["count"].as_u64().unwrap_or(4096) as usize;
        let values: Vec<f32> = (0..count).map(|_| generator.next()).collect();
        assert_parity!(&format!("{id}:values"), values);
    }
}

/// The input grid of `migration/reference/src/modules/common/futils.cpp`.
fn futils_grid() -> Vec<f32> {
    let mut xs: Vec<f32> = (-4000..=4000).map(|i| i as f32 * 0.01).collect();
    let mut state = 12345u32;
    for _ in 0..20000 {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let exponent = (state >> 23) & 0xff;
        if (100..=133).contains(&exponent) {
            xs.push(f32::from_bits(state));
        }
    }
    xs
}

#[test]
fn futils_scalar() {
    let id = "common/futils/grid";
    let xs = futils_grid();
    // Scaled by an exact power of two into about [1, 2): raw values reach
    // ~6e3, where one float ulp exceeds the absolute module gate.
    let exp2: Vec<f32> = xs
        .iter()
        .map(|&x| {
            let e = x * 0.1;
            let k = e.floor() as i32;
            scalar::exp2(e) * f32::from_bits(((127 - k) as u32) << 23)
        })
        .collect();
    let log2: Vec<f32> = xs
        .iter()
        .filter(|&&x| x != 0.0)
        .map(|&x| scalar::log2(x.abs()))
        .collect();
    let tanh: Vec<f32> = xs.iter().map(|&x| scalar::tanh(x)).collect();
    assert_parity!(&format!("{id}:exp2"), exp2);
    assert_parity!(&format!("{id}:log2"), log2);
    assert_parity!(&format!("{id}:tanh"), tanh);

    let mut power_scale = Vec::new();
    for i in 0..=100 {
        for p in -40..=40 {
            power_scale.push(scalar::power_scale(i as f32 / 100.0, p as f32 * 0.25));
        }
    }
    assert_parity!(&format!("{id}:power_scale"), power_scale);

    let buffer = [0.3f32, -0.7, 0.9, 0.1];
    let catmull: Vec<f32> = (0..=64)
        .map(|i| scalar::catmull_interpolate(&buffer, 0, i as f32 / 64.0))
        .collect();
    assert_parity!(&format!("{id}:catmull"), catmull);
}

/// The value generator of `migration/reference/src/modules/common/json.cpp`.
fn generated_floats(count: usize) -> json::Value {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut values = Vec::new();
    for i in 0..count {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        if i % 2 == 0 {
            let exponent = (state >> 52) & 0x7ff;
            if exponent == 0 || exponent == 0x7ff {
                continue;
            }
            values.push(json::Value::Float(f64::from_bits(state)));
        } else {
            let bits = (state >> 32) as u32;
            let exponent = (bits >> 23) & 0xff;
            if exponent == 0 || exponent == 0xff {
                continue;
            }
            values.push(json::Value::from(f32::from_bits(bits)));
        }
    }
    json::Value::Array(values)
}

#[test]
fn json_serializer() {
    for id in case_ids("common/json/") {
        let case = ModuleCase::load(&id);
        let params = case.params();
        if let Some(text) = params["text"].as_str() {
            let value = json::parse(text).expect("parses");
            assert_artifact!(&id, "dump.json", value.dump());
            assert_artifact!(&id, "pretty.json", value.dump_pretty(2));
        }
        if let Some(count) = params["float_count"].as_u64() {
            assert_artifact!(&id, "floats.json", generated_floats(count as usize).dump());
        }
    }
}
