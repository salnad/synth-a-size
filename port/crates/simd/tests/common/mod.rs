// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared inputs for the `sas-simd` tests.

#![allow(dead_code)]

/// Floats that exercise every special case of the SSE2 semantics.
pub fn special_floats() -> Vec<f32> {
    let mut values = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        -0.5,
        1.5,
        -1.5,
        2.5,
        -2.5,
        3.5,
        0.499_999_97,
        -0.499_999_97,
        1e-16,
        -1e-16,
        1e-20,
        f32::MIN_POSITIVE,
        f32::MIN_POSITIVE / 4.0,
        -f32::MIN_POSITIVE / 4.0,
        f32::from_bits(1),
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        -f32::NAN,
        f32::from_bits(0x7fc0_1234),
        2_147_483_648.0,
        -2_147_483_648.0,
        2_147_483_520.0,
        -2_147_483_520.0,
        4_294_967_296.0,
        8_388_608.5,
        8_388_607.5,
        16_777_217.0,
        1e10,
        -1e10,
        0.1,
        -0.1,
        12.0,
        127.0,
        128.0,
        -127.0,
        -128.0,
        8000.0,
        -8000.0,
        8000.001,
    ];
    let mut rng = XorShift(0x1234_5678);
    for _ in 0..64 {
        values.push(f32::from_bits(rng.next()));
    }
    for _ in 0..64 {
        values.push(rng.unit() * 8.0 - 4.0);
    }
    for _ in 0..32 {
        values.push(rng.unit() * 200.0 - 100.0);
    }
    values
}

/// Integers covering signed/unsigned boundaries.
pub fn special_ints() -> Vec<u32> {
    let mut values = vec![
        0,
        1,
        2,
        0x7f,
        0x80,
        0xff,
        0x7fff_ffff,
        0x8000_0000,
        0x8000_0001,
        0xffff_fffe,
        0xffff_ffff,
        0x0001_0000,
        0x00ff_00ff,
        0x7f80_0000,
    ];
    let mut rng = XorShift(0x9e37_79b9);
    for _ in 0..64 {
        values.push(rng.next());
    }
    values
}

/// Groups `values` into 4-lane vectors (rotating so every value meets every
/// lane position).
pub fn quads<T: Copy>(values: &[T]) -> Vec<[T; 4]> {
    let n = values.len();
    let mut out = Vec::with_capacity(n * 4);
    for offset in [0, 1, 7, 13] {
        for i in 0..n {
            out.push([
                values[i],
                values[(i + offset + 1) % n],
                values[(i + 2 * offset + 2) % n],
                values[(i + 3 * offset + 3) % n],
            ]);
        }
    }
    out
}

/// Every ordered pair of quads from `quads(values)`, subsampled to keep the
/// count manageable.
pub fn quad_pairs<T: Copy>(values: &[T]) -> Vec<([T; 4], [T; 4])> {
    let q = quads(values);
    let mut out = Vec::new();
    for (i, a) in q.iter().enumerate() {
        for (j, b) in q.iter().enumerate() {
            if (i * 31 + j * 17) % 7 == 0 {
                out.push((*a, *b));
            }
        }
    }
    out
}

pub struct XorShift(pub u32);

impl XorShift {
    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn bits(a: [f32; 4]) -> [u32; 4] {
    a.map(f32::to_bits)
}
