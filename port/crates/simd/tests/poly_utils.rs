// SPDX-License-Identifier: GPL-3.0-or-later

//! `poly_utils` against lane-wise scalar math.

mod common;

use common::{quads, special_floats, XorShift};
use sas_simd::backend::{Backend, Model};
use sas_simd::poly_utils as u;
use sas_simd::{Matrix, PolyFloat, PolyInt};

fn same(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn assert_lanes(name: &str, got: PolyFloat, want: [f32; 4]) {
    let g = got.to_array();
    for i in 0..4 {
        assert!(
            same(g[i], want[i]),
            "{name}: lane {i}: got {:?} want {:?}",
            g,
            want
        );
    }
}

/// Lanes inside the range where floor/ceil/round are exact.
fn rounding_inputs() -> Vec<f32> {
    let mut v: Vec<f32> = special_floats()
        .into_iter()
        .filter(|x| x.is_finite() && x.abs() < 4_000_000.0)
        .collect();
    let mut rng = XorShift(3);
    for _ in 0..2000 {
        v.push(rng.unit() * 2000.0 - 1000.0);
    }
    for i in -40..40 {
        v.push(i as f32 * 0.5);
    }
    v
}

#[test]
fn conversions_follow_the_native_path() {
    for q in quads(&special_floats()) {
        let v = PolyFloat::from_array(q);
        let model = Model::f_to_i(q);
        assert_eq!(u::to_int(v).to_array(), model);
        assert_eq!(u::trunc_to_int(v).to_array(), model);
        assert_lanes("trunc", u::trunc(v), model.map(|x| x as i32 as f32));
        assert_eq!(u::reinterpret_to_int(v).to_array(), q.map(f32::to_bits));
    }
    let ints = PolyInt::new(0, 1, (-5i32) as u32, 0x7fff_ffff);
    assert_lanes(
        "to_float",
        u::to_float(ints),
        [0.0, 1.0, -5.0, 2_147_483_648.0],
    );
    assert_lanes(
        "reinterpret_to_float",
        u::reinterpret_to_float(PolyInt::splat(0x3f80_0000)),
        [1.0; 4],
    );
}

#[test]
fn floor_ceil_round_mod() {
    for q in quads(&rounding_inputs()) {
        let v = PolyFloat::from_array(q);
        // Upstream builds these from an int round trip plus an added
        // correction, so a zero result is always `+0`.
        let floor = |x: f32| x.floor() + 0.0;
        assert_lanes("floor", u::floor(v), q.map(floor));
        assert_lanes("ceil", u::ceil(v), q.map(|x| x.ceil() + 0.0));
        assert_lanes("round", u::round(v), q.map(|x| floor(x + 0.5)));
        assert_eq!(
            u::floor_to_int(v).to_array(),
            q.map(|x| x.floor() as i32 as u32)
        );
        assert_eq!(
            u::round_to_int(v).to_array(),
            q.map(|x| (x + 0.5).floor() as i32 as u32)
        );
        assert_lanes("modulo", u::modulo(v), q.map(|x| x - floor(x)));
        assert_lanes(
            "mod_once",
            u::mod_once(v),
            q.map(|x| if x < 1.0 { x + 0.0 } else { (x - 1.0) + 0.0 }),
        );
    }
}

#[test]
fn shuffles() {
    let a = PolyFloat::new(1.0, 2.0, 3.0, 4.0);
    let b = PolyFloat::new(5.0, 6.0, 7.0, 8.0);
    assert_eq!(u::swap_stereo(a).to_array(), [2.0, 1.0, 4.0, 3.0]);
    assert_eq!(u::swap_voices(a).to_array(), [3.0, 4.0, 1.0, 2.0]);
    assert_eq!(u::swap_inner(a).to_array(), [1.0, 3.0, 2.0, 4.0]);
    assert_eq!(u::reverse(a).to_array(), [4.0, 3.0, 2.0, 1.0]);
    assert_eq!(u::consolidate_audio(a, b).to_array(), [1.0, 5.0, 2.0, 6.0]);
    assert_eq!(
        u::compact_first_voices(a, b).to_array(),
        [1.0, 2.0, 5.0, 6.0]
    );
    assert_eq!(u::sum_split_audio(a).to_array(), [3.0, 7.0, 3.0, 7.0]);
    let i = PolyInt::new(1, 2, 3, 4);
    assert_eq!(u::swap_stereo_int(i).to_array(), [2, 1, 4, 3]);
    assert_eq!(u::swap_voices_int(i).to_array(), [3, 4, 1, 2]);
    assert_eq!(u::max_float(PolyFloat::new(1.0, 9.0, -3.0, 4.0)), 9.0);
    assert_eq!(u::min_float(PolyFloat::new(1.0, 9.0, -3.0, 4.0)), -3.0);

    let mut m = Matrix::new(a, b, a * 2.0, b * 2.0);
    m.transpose();
    assert_eq!(m.row0.to_array(), [1.0, 5.0, 2.0, 10.0]);
    assert_eq!(m.row3.to_array(), [4.0, 8.0, 8.0, 16.0]);
}

#[test]
fn masks_and_selection() {
    let a = PolyFloat::new(1.0, -2.0, 3.0, -0.0);
    let b = PolyFloat::new(10.0, 20.0, 30.0, 40.0);
    let mask = PolyFloat::greater_than(a, PolyFloat::zero());
    assert_eq!(mask.to_array(), [u32::MAX, 0, u32::MAX, 0]);
    assert_lanes(
        "mask_load",
        u::mask_load(b, a, mask),
        [1.0, 20.0, 3.0, 40.0],
    );
    assert_eq!(
        u::mask_load_int(PolyInt::splat(7), PolyInt::splat(9), mask).to_array(),
        [9, 7, 9, 7]
    );
    assert_eq!(
        u::close_to_zero_mask(PolyFloat::new(0.0, 1e-17, -1e-15, 1.0)).to_array(),
        [u32::MAX, u32::MAX, 0, 0]
    );
    let buffer = [
        PolyFloat::new(0.0, 1.0, 0.0, 0.0),
        PolyFloat::new(0.0, 0.0, 0.5, 0.0),
    ];
    assert_eq!(
        u::get_silent_mask(&buffer).to_array(),
        [u32::MAX, 0, 0, u32::MAX]
    );
    assert!(!u::is_silent(&buffer));
    assert!(u::is_silent(&[PolyFloat::splat(1e-17)]));
    assert_lanes(
        "peak",
        u::peak(&[a, b * -1.0, PolyFloat::splat(100.0)], 2, 1),
        [10.0, 20.0, 30.0, 40.0],
    );
    assert!(u::equal(a, a));
    // `equal` is built on `not_equal`, which is unordered on SSE2 and ordered on
    // NEON, so NaN lanes compare unequal on x86 and equal on arm64 upstream.
    assert_eq!(
        u::equal(PolyFloat::splat(f32::NAN), PolyFloat::splat(f32::NAN)),
        cfg!(target_arch = "aarch64")
    );
    assert!(u::is_contained(PolyFloat::splat(8000.0)));
    assert!(!u::is_contained(PolyFloat::splat(8000.001)));
    assert!(!u::is_finite(PolyFloat::new(0.0, f32::INFINITY, 0.0, 0.0)));
    assert_lanes("clamp", u::clamp(b, 15.0, 35.0), [15.0, 20.0, 30.0, 35.0]);
    assert_eq!(
        u::clamp_int(
            PolyInt::new(0, 5, 10, 15),
            PolyInt::splat(3),
            PolyInt::splat(12)
        )
        .to_array(),
        [3, 5, 10, 12]
    );
}

#[test]
fn interpolation() {
    let mut rng = XorShift(11);
    for _ in 0..500 {
        let t = rng.unit();
        let from = PolyFloat::new(rng.unit(), -rng.unit(), 2.0, -3.0);
        let to = PolyFloat::new(rng.unit(), rng.unit(), -2.0, 5.0);
        let f = from.to_array();
        let g = to.to_array();
        assert_lanes(
            "interpolate",
            u::interpolate(from, to, t),
            [0, 1, 2, 3].map(|i| f[i] + (g[i] - f[i]) * t),
        );

        let tf = PolyFloat::splat(t);
        let cubic = u::get_cubic_interpolation_values(t).to_array();
        let want = [
            (-1.0f32 / 6.0) * t * (t - 1.0) * (t - 2.0),
            (1.0f32 / 2.0) * (t + 1.0) * (t - 1.0) * (t - 2.0),
            (-1.0f32 / 2.0) * (t + 0.0) * (t + 1.0) * (t - 2.0),
            (1.0f32 / 6.0) * t * (t + 1.0) * (t - 1.0),
        ];
        let lagrange_one = [0.0, 1.0, 0.0, 0.0];
        let lagrange_two = [-1.0, -1.0, 1.0, 1.0];
        let lagrange_three = [-2.0, -2.0, -2.0, -1.0];
        let mult = [-1.0f32 / 6.0, 1.0 / 2.0, -1.0 / 2.0, 1.0 / 6.0];
        for i in 0..4 {
            let exact =
                mult[i] * (t + lagrange_one[i]) * (t + lagrange_two[i]) * (t + lagrange_three[i]);
            assert!(same(cubic[i], exact), "cubic lane {i}");
            assert!((cubic[i] - want[i]).abs() < 1e-6);
        }
        let sum: f32 = cubic.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);

        let catmull = u::get_catmull_interpolation_matrix(tf);
        let half_t = t * 0.5;
        let half_t2 = t * half_t;
        let half_t3 = half_t2 * t;
        let half_three_t3 = half_t3 * 3.0;
        assert_lanes(
            "catmull0",
            catmull.row0,
            [half_t2 * 2.0 - half_t3 - half_t; 4],
        );
        assert_lanes(
            "catmull1",
            catmull.row1,
            [(half_three_t3 - half_t2 * 5.0) + 1.0; 4],
        );
        assert_lanes(
            "catmull2",
            catmull.row2,
            [(half_t + half_t2 * 4.0) - half_three_t3; 4],
        );
        assert_lanes("catmull3", catmull.row3, [half_t3 - half_t2; 4]);

        let linear = u::get_linear_interpolation_matrix(tf);
        assert_lanes("linear1", linear.row1, [1.0 - t; 4]);

        let poly = u::get_polynomial_interpolation_matrix(tf);
        let t_prev = t + 1.0;
        let t_to = t - 1.0;
        let t_next = t - 2.0;
        assert_lanes("poly0", poly.row0, [t * (t_to * t_next) * (-1.0 / 6.0); 4]);
        assert_lanes("poly3", poly.row3, [(t_prev * t) * t_to * (1.0 / 6.0); 4]);
    }
}

#[test]
fn matrix_ops() {
    let a = Matrix::new(1.0, 2.0, 3.0, 4.0);
    let b = Matrix::new(
        PolyFloat::new(1.0, 0.0, 0.0, 0.0),
        PolyFloat::new(0.0, 1.0, 0.0, 0.0),
        PolyFloat::new(0.0, 0.0, 1.0, 0.0),
        PolyFloat::new(0.0, 0.0, 0.0, 1.0),
    );
    assert_eq!(a.sum_rows().to_array(), [10.0; 4]);
    assert_eq!(a.multiply_and_sum_rows(&b).to_array(), [1.0, 2.0, 3.0, 4.0]);
    let mut c = a;
    c.interpolate_columns(&b, PolyFloat::splat(0.5));
    assert_eq!(c.row0.to_array(), [1.0, 0.5, 0.5, 0.5]);
    let mut d = a;
    d.interpolate_rows(&b, PolyFloat::new(0.0, 1.0, 0.5, 0.0));
    assert_eq!(d.row1.to_array(), [0.0, 1.0, 0.0, 0.0]);
    assert_eq!(d.row2.to_array(), [1.5, 1.5, 2.0, 1.5]);

    let buffer: Vec<f32> = (0..16).map(|x| x as f32).collect();
    let m = u::get_value_matrix(&buffer, PolyInt::new(0, 2, 5, 12));
    assert_eq!(m.row2.to_array(), [5.0, 6.0, 7.0, 8.0]);
    assert_lanes(
        "gather",
        u::gather(&buffer, PolyInt::new(3, 1, 4, 1)),
        [3.0, 1.0, 4.0, 1.0],
    );
    let (value, next) = u::adjacent_gather(&buffer, PolyInt::new(3, 1, 4, 1));
    assert_eq!(value.to_array(), [3.0, 1.0, 4.0, 1.0]);
    assert_eq!(next.to_array(), [4.0, 2.0, 5.0, 2.0]);
}

#[test]
fn stereo_and_misc() {
    let v = PolyFloat::new(1.0, 3.0, -2.0, 4.0);
    let ms = u::encode_mid_side(v);
    assert_eq!(ms.to_array(), [2.0, 1.0, 1.0, 3.0]);
    assert_eq!(u::decode_mid_side(ms).to_array(), [1.0, 3.0, -2.0, 4.0]);
    assert_eq!(
        u::triangle_wave(PolyFloat::new(0.0, 0.25, 0.5, 0.75)).to_array(),
        [0.5, 1.0, 0.5, 0.0]
    );
    assert_lanes(
        "flt_scale",
        u::flt_scale(PolyFloat::splat(0.5), PolyFloat::splat(3.0)),
        [3.0 * 0.5 / ((3.0 - 1.0) * 0.5 + 1.0); 4],
    );
    let offset = u::get_cycle_offset_from_samples(44100, PolyFloat::splat(1.5), 44100, 1);
    assert_eq!(offset.get(0), 0.5);
    assert_eq!(
        u::pow2_to_float(PolyInt::new(0, 1, (-1i32) as u32, 10)).to_array(),
        [1.0, 2.0, 0.5, 1024.0]
    );
    assert_eq!(u::shift_right::<4>(PolyInt::splat(0xf0)).get(0), 0xf);
    assert_eq!(u::shift_left::<4>(PolyInt::splat(0xf)).get(0), 0xf0);

    let phase = PolyFloat::new(0.0, 0.25, 0.5, 0.75);
    let voice = PolyFloat::new(0.0, 3.0, 15.0, 17.0);
    let encoded = u::encode_phase_and_voice(phase, voice);
    let (decoded_phase, decoded_voice) = u::decode_phase_and_voice(encoded);
    assert_eq!(decoded_voice.to_array(), [1.0, 4.0, 16.0, 2.0]);
    for i in 0..4 {
        assert!((decoded_phase.get(i) - phase.get(i)).abs() < 1e-5);
    }
}

#[test]
fn transpose_snapping() {
    // C major: C D E F G A B.
    let major = 0b1010_1011_0101;
    let mut snap = [0.0f32; 13];
    u::fill_snap_buffer(major, &mut snap);
    for (i, &s) in snap.iter().enumerate() {
        let semitone = i % 12;
        if (major >> semitone) & 1 != 0 {
            assert_eq!(s, i as f32, "enabled note {i} snaps to itself");
        }
    }
    assert!(u::is_transpose_snapping(major));
    assert!(!u::is_transpose_quantize_global(major));
    assert!(u::is_transpose_quantize_global(major | (1 << 12)));

    let snapped = u::snap_transpose(PolyFloat::new(1.2, -0.6, 13.4, 6.4), major);
    assert_eq!(snapped.to_array(), [2.0, -1.0, 14.0, 7.0]);
}
