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
        // Exact: tells the libstdc++ and libc++ columns apart (A9), which
        // `values` alone cannot within tolerance.
        let at_max = values.iter().filter(|&&v| v == max).count() as f32;
        assert_parity!(&format!("{id}:values"), values);
        assert_parity!(&format!("{id}:at_max"), vec![at_max]);
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

    let mut exp_split = (Vec::new(), Vec::new());
    let mut log = Vec::new();
    let mut pow_split = (Vec::new(), Vec::new());
    for &x in &xs {
        if x.abs() <= 80.0 {
            push_split(&mut exp_split, scalar::exp(x));
        }
        if x != 0.0 {
            log.push(scalar::log(x.abs()));
            for exponent in [-2.5f32, -1.0, -0.5, 0.25, 0.5, 1.0, 1.5, 3.0] {
                push_split(&mut pow_split, scalar::pow(x.abs(), exponent));
            }
        }
    }
    assert_parity!(&format!("{id}:exp_mantissa"), exp_split.0);
    assert_parity!(&format!("{id}:exp_exponent"), exp_split.1);
    assert_parity!(&format!("{id}:log"), log);
    assert_parity!(&format!("{id}:pow_mantissa"), pow_split.0);
    assert_parity!(&format!("{id}:pow_exponent"), pow_split.1);
}

/// `pushSplit` of `futils.cpp`: mantissa in [1, 2) with the sign, and the
/// unbiased exponent.
fn push_split((mantissas, exponents): &mut (Vec<f32>, Vec<f32>), value: f32) {
    let bits = value.to_bits();
    exponents.push((((bits >> 23) & 0xff) as i32 - 127) as f32);
    mantissas.push(f32::from_bits((bits & 0x807f_ffff) | (127 << 23)));
}

#[test]
fn load_save_codecs() {
    use sas_common::{base64, load_save};
    for id in case_ids("common/load_save/") {
        let case = ModuleCase::load(&id);
        let params = case.params();
        match params["kind"].as_str().expect("kind") {
            "base64_encode" => {
                let bytes: Vec<u8> = params["bytes"]
                    .as_array()
                    .expect("bytes")
                    .iter()
                    .map(|b| b.as_u64().expect("byte") as u8)
                    .collect();
                assert_artifact!(&id, "base64.txt", base64::encode(&bytes));
            }
            "base64_decode" => {
                let (bytes, ok) = base64::decode(params["text"].as_str().expect("text"));
                let mut result = vec![f32::from(u8::from(ok)), bytes.len() as f32];
                result.extend(bytes.iter().map(|&b| f32::from(b)));
                assert_parity!(&format!("{id}:result"), result);
            }
            "pcm" => {
                let values: Vec<f32> = params["values"]
                    .as_array()
                    .expect("values")
                    .iter()
                    .map(|v| v.as_f64().expect("value") as f32)
                    .collect();
                let mut data = json::Value::Null;
                data.set("samples", base64::encode_f32s(&values));

                load_save::convert_buffer_to_pcm(&mut data, "samples").expect("to PCM");
                assert_artifact!(&id, "pcm.json", data.dump());
                let pcm = base64::decode_i16s(data.get("samples").unwrap().to_str().unwrap());
                let pcm: Vec<f32> = pcm.iter().map(|&v| f32::from(v) / 32768.0).collect();
                assert_parity!(&format!("{id}:pcm"), pcm);

                load_save::convert_pcm_to_float_buffer(&mut data, "samples").expect("to float");
                assert_artifact!(&id, "float.json", data.dump());
                let floats = base64::decode_f32s(data.get("samples").unwrap().to_str().unwrap());
                assert_parity!(&format!("{id}:floats"), floats);
            }
            "versions" => {
                let results: Vec<f32> = params["pairs"]
                    .as_array()
                    .expect("pairs")
                    .iter()
                    .map(|pair| {
                        load_save::compare_version_strings(
                            pair[0].as_str().expect("a"),
                            pair[1].as_str().expect("b"),
                        ) as f32
                    })
                    .collect();
                assert_parity!(&format!("{id}:results"), results);
            }
            other => panic!("unknown kind {other}"),
        }
    }
}

#[test]
fn line_generator_edit() {
    for id in case_ids("common/line_generator_edit/") {
        let case = ModuleCase::load(&id);
        let params = case.params();
        let resolution = params["resolution"]
            .as_u64()
            .unwrap_or(LineGenerator::DEFAULT_RESOLUTION as u64) as usize;
        let mut generator = LineGenerator::new(resolution);
        match params["init"].as_str().unwrap_or("linear") {
            "linear" => generator.init_linear(),
            "triangle" => generator.init_triangle(),
            "square" => generator.init_square(),
            "sin" => generator.init_sin(),
            "saw_up" => generator.init_saw_up(),
            "saw_down" => generator.init_saw_down(),
            other => panic!("unknown init {other}"),
        }
        let empty = Vec::new();
        for op in params["ops"].as_array().unwrap_or(&empty) {
            let index = || op["index"].as_u64().expect("index") as usize;
            let point = || {
                (
                    op["x"].as_f64().expect("x") as f32,
                    op["y"].as_f64().expect("y") as f32,
                )
            };
            match op["op"].as_str().expect("op") {
                "add_point" => generator.add_point(index(), point()),
                "add_middle_point" => generator.add_middle_point(index()),
                "remove_point" => generator.remove_point(index()),
                "flip_horizontal" => generator.flip_horizontal(),
                "flip_vertical" => generator.flip_vertical(),
                "set_point" => generator.set_point(index(), point()),
                "set_power" => {
                    generator.set_power(index(), op["power"].as_f64().expect("power") as f32)
                }
                "set_num_points" => {
                    generator.set_num_points(op["count"].as_u64().expect("count") as usize)
                }
                "set_smooth" => generator.set_smooth(op["value"].as_bool().expect("value")),
                "set_loop" => generator.set_loop(op["value"].as_bool().expect("value")),
                other => panic!("unknown op {other}"),
            }
        }
        generator.render();

        assert_parity!(
            &format!("{id}:buffer"),
            generator.cubic_interpolation_buffer().to_vec()
        );
        let values: Vec<f32> = (0..=40)
            .map(|i| generator.value_at_phase(i as f32 / 40.0))
            .collect();
        assert_parity!(&format!("{id}:value_at_phase"), values);
        let state = vec![
            f32::from(u8::from(generator.is_linear())),
            generator.num_points() as f32,
        ];
        assert_parity!(&format!("{id}:state"), state);
        if params["saved_json"].as_bool().unwrap_or(true) {
            assert_artifact!(&id, "saved.json", generator.to_json().dump());
        }
    }
}

#[test]
fn json_accessors() {
    use json::Value;
    let id = "common/json_accessors/conversions";
    let case = ModuleCase::load(id);
    let params = case.params();
    let or_null = |value: Option<Value>| value.unwrap_or(Value::Null);

    let values = json::parse(params["values"].as_str().expect("values")).expect("values JSON");
    let mut conversions = Value::Array(Vec::new());
    for value in values.iter_values() {
        let mut result = Value::Null;
        result.set("bool", or_null(value.to_bool().ok().map(Value::Bool)));
        result.set("double", or_null(value.to_f64().ok().map(Value::Float)));
        result.set(
            "float",
            or_null(value.to_f32().ok().map(|v| Value::Float(f64::from(v)))),
        );
        result.set(
            "int",
            or_null(value.to_i32().ok().map(|v| Value::Int(i64::from(v)))),
        );
        result.set("int64", or_null(value.to_i64().ok().map(Value::Int)));
        result.set(
            "string",
            or_null(value.to_str().ok().map(|v| Value::String(v.to_string()))),
        );
        conversions.push(result);
    }
    assert_artifact!(id, "conversions.json", conversions.dump());

    let enum_values =
        json::parse(params["enum_values"].as_str().expect("enum_values")).expect("enum JSON");
    let mut enums = Value::Array(Vec::new());
    for value in enum_values.iter_values() {
        enums.push(or_null(
            value.to_enum().ok().map(|v| Value::Int(i64::from(v))),
        ));
    }
    assert_artifact!(id, "enums.json", enums.dump());
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
