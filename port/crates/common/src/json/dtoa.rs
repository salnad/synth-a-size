// Ports the Grisu2 double-to-string conversion of the JSON library vendored
// upstream (`vital/third_party/json/json.h`, nlohmann/json 3.1.2,
// `nlohmann::detail::to_chars`). That code is MIT-licensed:
// Copyright (c) 2013-2018 Niels Lohmann <http://nlohmann.me>, and the Grisu2
// implementation is based on Florian Loitsch, "Printing Floating-Point
// Numbers Quickly and Accurately with Integers" (PLDI 2010).
//
// Grisu2 does not always produce the shortest representation, so a generic
// shortest-round-trip formatter would print some values differently. This is
// a line-by-line translation so that saved JSON matches upstream byte for byte.

#[derive(Clone, Copy)]
struct DiyFp {
    f: u64,
    e: i32,
}

impl DiyFp {
    fn sub(x: DiyFp, y: DiyFp) -> DiyFp {
        debug_assert!(x.e == y.e && x.f >= y.f);
        DiyFp {
            f: x.f - y.f,
            e: x.e,
        }
    }

    fn mul(x: DiyFp, y: DiyFp) -> DiyFp {
        let u_lo = x.f & 0xFFFF_FFFF;
        let u_hi = x.f >> 32;
        let v_lo = y.f & 0xFFFF_FFFF;
        let v_hi = y.f >> 32;

        let p0 = u_lo * v_lo;
        let p1 = u_lo * v_hi;
        let p2 = u_hi * v_lo;
        let p3 = u_hi * v_hi;

        let p0_hi = p0 >> 32;
        let p1_lo = p1 & 0xFFFF_FFFF;
        let p1_hi = p1 >> 32;
        let p2_lo = p2 & 0xFFFF_FFFF;
        let p2_hi = p2 >> 32;

        let mut q = p0_hi + p1_lo + p2_lo;
        q += 1u64 << (64 - 32 - 1);
        let h = p3 + p2_hi + p1_hi + (q >> 32);
        DiyFp {
            f: h,
            e: x.e + y.e + 64,
        }
    }

    fn normalize(mut x: DiyFp) -> DiyFp {
        debug_assert!(x.f != 0);
        while (x.f >> 63) == 0 {
            x.f <<= 1;
            x.e -= 1;
        }
        x
    }

    fn normalize_to(x: DiyFp, target_exponent: i32) -> DiyFp {
        let delta = x.e - target_exponent;
        debug_assert!(delta >= 0);
        DiyFp {
            f: x.f << delta,
            e: target_exponent,
        }
    }
}

struct Boundaries {
    w: DiyFp,
    minus: DiyFp,
    plus: DiyFp,
}

fn compute_boundaries(value: f64) -> Boundaries {
    const PRECISION: i32 = 53;
    const BIAS: i32 = 1024 - 1 + (PRECISION - 1);
    const MIN_EXP: i32 = 1 - BIAS;
    const HIDDEN_BIT: u64 = 1u64 << (PRECISION - 1);

    let bits = value.to_bits();
    let e = bits >> (PRECISION - 1);
    let f = bits & (HIDDEN_BIT - 1);

    let is_denormal = e == 0;
    let v = if is_denormal {
        DiyFp { f, e: MIN_EXP }
    } else {
        DiyFp {
            f: f + HIDDEN_BIT,
            e: e as i32 - BIAS,
        }
    };

    let lower_boundary_is_closer = f == 0 && e > 1;
    let m_plus = DiyFp {
        f: 2 * v.f + 1,
        e: v.e - 1,
    };
    let m_minus = if lower_boundary_is_closer {
        DiyFp {
            f: 4 * v.f - 1,
            e: v.e - 2,
        }
    } else {
        DiyFp {
            f: 2 * v.f - 1,
            e: v.e - 1,
        }
    };

    let w_plus = DiyFp::normalize(m_plus);
    let w_minus = DiyFp::normalize_to(m_minus, w_plus.e);
    Boundaries {
        w: DiyFp::normalize(v),
        minus: w_minus,
        plus: w_plus,
    }
}

const ALPHA: i32 = -60;

struct CachedPower {
    f: u64,
    e: i32,
    k: i32,
}

#[rustfmt::skip]
const CACHED_POWERS: [(u64, i32, i32); 79] = [
    (0xAB70FE17C79AC6CA, -1060, -300), (0xFF77B1FCBEBCDC4F, -1034, -292),
    (0xBE5691EF416BD60C, -1007, -284), (0x8DD01FAD907FFC3C,  -980, -276),
    (0xD3515C2831559A83,  -954, -268), (0x9D71AC8FADA6C9B5,  -927, -260),
    (0xEA9C227723EE8BCB,  -901, -252), (0xAECC49914078536D,  -874, -244),
    (0x823C12795DB6CE57,  -847, -236), (0xC21094364DFB5637,  -821, -228),
    (0x9096EA6F3848984F,  -794, -220), (0xD77485CB25823AC7,  -768, -212),
    (0xA086CFCD97BF97F4,  -741, -204), (0xEF340A98172AACE5,  -715, -196),
    (0xB23867FB2A35B28E,  -688, -188), (0x84C8D4DFD2C63F3B,  -661, -180),
    (0xC5DD44271AD3CDBA,  -635, -172), (0x936B9FCEBB25C996,  -608, -164),
    (0xDBAC6C247D62A584,  -582, -156), (0xA3AB66580D5FDAF6,  -555, -148),
    (0xF3E2F893DEC3F126,  -529, -140), (0xB5B5ADA8AAFF80B8,  -502, -132),
    (0x87625F056C7C4A8B,  -475, -124), (0xC9BCFF6034C13053,  -449, -116),
    (0x964E858C91BA2655,  -422, -108), (0xDFF9772470297EBD,  -396, -100),
    (0xA6DFBD9FB8E5B88F,  -369,  -92), (0xF8A95FCF88747D94,  -343,  -84),
    (0xB94470938FA89BCF,  -316,  -76), (0x8A08F0F8BF0F156B,  -289,  -68),
    (0xCDB02555653131B6,  -263,  -60), (0x993FE2C6D07B7FAC,  -236,  -52),
    (0xE45C10C42A2B3B06,  -210,  -44), (0xAA242499697392D3,  -183,  -36),
    (0xFD87B5F28300CA0E,  -157,  -28), (0xBCE5086492111AEB,  -130,  -20),
    (0x8CBCCC096F5088CC,  -103,  -12), (0xD1B71758E219652C,   -77,   -4),
    (0x9C40000000000000,   -50,    4), (0xE8D4A51000000000,   -24,   12),
    (0xAD78EBC5AC620000,     3,   20), (0x813F3978F8940984,    30,   28),
    (0xC097CE7BC90715B3,    56,   36), (0x8F7E32CE7BEA5C70,    83,   44),
    (0xD5D238A4ABE98068,   109,   52), (0x9F4F2726179A2245,   136,   60),
    (0xED63A231D4C4FB27,   162,   68), (0xB0DE65388CC8ADA8,   189,   76),
    (0x83C7088E1AAB65DB,   216,   84), (0xC45D1DF942711D9A,   242,   92),
    (0x924D692CA61BE758,   269,  100), (0xDA01EE641A708DEA,   295,  108),
    (0xA26DA3999AEF774A,   322,  116), (0xF209787BB47D6B85,   348,  124),
    (0xB454E4A179DD1877,   375,  132), (0x865B86925B9BC5C2,   402,  140),
    (0xC83553C5C8965D3D,   428,  148), (0x952AB45CFA97A0B3,   455,  156),
    (0xDE469FBD99A05FE3,   481,  164), (0xA59BC234DB398C25,   508,  172),
    (0xF6C69A72A3989F5C,   534,  180), (0xB7DCBF5354E9BECE,   561,  188),
    (0x88FCF317F22241E2,   588,  196), (0xCC20CE9BD35C78A5,   614,  204),
    (0x98165AF37B2153DF,   641,  212), (0xE2A0B5DC971F303A,   667,  220),
    (0xA8D9D1535CE3B396,   694,  228), (0xFB9B7CD9A4A7443C,   720,  236),
    (0xBB764C4CA7A44410,   747,  244), (0x8BAB8EEFB6409C1A,   774,  252),
    (0xD01FEF10A657842C,   800,  260), (0x9B10A4E5E9913129,   827,  268),
    (0xE7109BFBA19C0C9D,   853,  276), (0xAC2820D9623BF429,   880,  284),
    (0x80444B5E7AA7CF85,   907,  292), (0xBF21E44003ACDD2D,   933,  300),
    (0x8E679C2F5E44FF8F,   960,  308), (0xD433179D9C8CB841,   986,  316),
    (0x9E19DB92B4E31BA9,  1013,  324),
];

fn get_cached_power_for_binary_exponent(e: i32) -> CachedPower {
    const MIN_DEC_EXP: i32 = -300;
    const DEC_STEP: i32 = 8;

    let f = ALPHA - e - 1;
    let k = (f * 78913) / (1 << 18) + i32::from(f > 0);
    let index = (-MIN_DEC_EXP + k + (DEC_STEP - 1)) / DEC_STEP;
    let (f, e, k) = CACHED_POWERS[index as usize];
    CachedPower { f, e, k }
}

fn find_largest_pow10(n: u32) -> (i32, u32) {
    if n >= 1_000_000_000 {
        (10, 1_000_000_000)
    } else if n >= 100_000_000 {
        (9, 100_000_000)
    } else if n >= 10_000_000 {
        (8, 10_000_000)
    } else if n >= 1_000_000 {
        (7, 1_000_000)
    } else if n >= 100_000 {
        (6, 100_000)
    } else if n >= 10_000 {
        (5, 10_000)
    } else if n >= 1_000 {
        (4, 1_000)
    } else if n >= 100 {
        (3, 100)
    } else if n >= 10 {
        (2, 10)
    } else {
        (1, 1)
    }
}

fn grisu2_round(buf: &mut [u8], len: usize, dist: u64, delta: u64, mut rest: u64, ten_k: u64) {
    while rest < dist
        && delta - rest >= ten_k
        && (rest + ten_k < dist || dist - rest > rest + ten_k - dist)
    {
        buf[len - 1] -= 1;
        rest += ten_k;
    }
}

fn grisu2_digit_gen(
    buffer: &mut [u8],
    length: &mut usize,
    decimal_exponent: &mut i32,
    m_minus: DiyFp,
    w: DiyFp,
    m_plus: DiyFp,
) {
    let mut delta = DiyFp::sub(m_plus, m_minus).f;
    let mut dist = DiyFp::sub(m_plus, w).f;

    let one = DiyFp {
        f: 1u64 << -m_plus.e,
        e: m_plus.e,
    };

    let mut p1 = (m_plus.f >> -one.e) as u32;
    let mut p2 = m_plus.f & (one.f - 1);

    let (k, mut pow10) = find_largest_pow10(p1);

    let mut n = k;
    while n > 0 {
        let d = p1 / pow10;
        let r = p1 % pow10;
        buffer[*length] = b'0' + d as u8;
        *length += 1;
        p1 = r;
        n -= 1;

        let rest = (u64::from(p1) << -one.e) + p2;
        if rest <= delta {
            *decimal_exponent += n;
            let ten_n = u64::from(pow10) << -one.e;
            grisu2_round(buffer, *length, dist, delta, rest, ten_n);
            return;
        }

        pow10 /= 10;
    }

    let mut m = 0;
    loop {
        p2 *= 10;
        let d = p2 >> -one.e;
        let r = p2 & (one.f - 1);
        buffer[*length] = b'0' + d as u8;
        *length += 1;
        p2 = r;
        m += 1;

        delta *= 10;
        dist *= 10;
        if p2 <= delta {
            break;
        }
    }

    *decimal_exponent -= m;
    let ten_m = one.f;
    grisu2_round(buffer, *length, dist, delta, p2, ten_m);
}

fn grisu2(buf: &mut [u8], len: &mut usize, decimal_exponent: &mut i32, value: f64) {
    let w = compute_boundaries(value);
    let (m_minus, v, m_plus) = (w.minus, w.w, w.plus);

    let cached = get_cached_power_for_binary_exponent(m_plus.e);
    let c_minus_k = DiyFp {
        f: cached.f,
        e: cached.e,
    };

    let w = DiyFp::mul(v, c_minus_k);
    let w_minus = DiyFp::mul(m_minus, c_minus_k);
    let w_plus = DiyFp::mul(m_plus, c_minus_k);

    let big_m_minus = DiyFp {
        f: w_minus.f + 1,
        e: w_minus.e,
    };
    let big_m_plus = DiyFp {
        f: w_plus.f - 1,
        e: w_plus.e,
    };

    *decimal_exponent = -cached.k;
    grisu2_digit_gen(buf, len, decimal_exponent, big_m_minus, w, big_m_plus);
}

fn append_exponent(out: &mut String, mut e: i32) {
    if e < 0 {
        e = -e;
        out.push('-');
    } else {
        out.push('+');
    }
    let k = e as u32;
    if k < 10 {
        out.push('0');
        out.push((b'0' + k as u8) as char);
    } else if k < 100 {
        out.push((b'0' + (k / 10) as u8) as char);
        out.push((b'0' + (k % 10) as u8) as char);
    } else {
        out.push((b'0' + (k / 100) as u8) as char);
        let k = k % 100;
        out.push((b'0' + (k / 10) as u8) as char);
        out.push((b'0' + (k % 10) as u8) as char);
    }
}

fn format_buffer(
    out: &mut String,
    digits: &[u8],
    decimal_exponent: i32,
    min_exp: i32,
    max_exp: i32,
) {
    let k = digits.len() as i32;
    let n = k + decimal_exponent;
    let digit_str = |range: std::ops::Range<usize>| -> &str {
        std::str::from_utf8(&digits[range]).expect("ascii digits")
    };

    if k <= n && n <= max_exp {
        out.push_str(digit_str(0..k as usize));
        for _ in 0..(n - k) {
            out.push('0');
        }
        out.push_str(".0");
        return;
    }

    if 0 < n && n <= max_exp {
        out.push_str(digit_str(0..n as usize));
        out.push('.');
        out.push_str(digit_str(n as usize..k as usize));
        return;
    }

    if min_exp < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..(-n) {
            out.push('0');
        }
        out.push_str(digit_str(0..k as usize));
        return;
    }

    if k == 1 {
        out.push_str(digit_str(0..1));
    } else {
        out.push_str(digit_str(0..1));
        out.push('.');
        out.push_str(digit_str(1..k as usize));
    }
    out.push('e');
    append_exponent(out, n - 1);
}

/// Appends `value` formatted exactly as nlohmann/json 3.1.2 `dump_float`
/// does for a double (`null` for NaN and infinities).
///
/// Fast-math mirror, pending the owner's byte-exact vs bounded JSON decision
/// (`migration/LEDGER.md`): upstream's source tests `std::signbit(value)`
/// (`vital/third_party/json/json.h:8205`), which `-ffast-math` folds into
/// `value < 0`, so the shipped build prints `-0.0` as `0.0`. This follows
/// the shipped build.
pub fn write_f64(out: &mut String, mut value: f64) {
    if !value.is_finite() {
        out.push_str("null");
        return;
    }

    if value < 0.0 {
        value = -value;
        out.push('-');
    }

    if value == 0.0 {
        out.push_str("0.0");
        return;
    }

    let mut buf = [0u8; 32];
    let mut len = 0usize;
    let mut decimal_exponent = 0i32;
    grisu2(&mut buf, &mut len, &mut decimal_exponent, value);

    const MIN_EXP: i32 = -4;
    const MAX_EXP: i32 = 15;
    format_buffer(out, &buf[..len], decimal_exponent, MIN_EXP, MAX_EXP);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(value: f64) -> String {
        let mut s = String::new();
        write_f64(&mut s, value);
        s
    }

    #[test]
    fn formats_like_nlohmann() {
        assert_eq!(fmt(0.0), "0.0");
        // Pins the pending fast-math mirror choice (see `write_f64`); the
        // source formula would print "-0.0".
        assert_eq!(fmt(-0.0), "0.0");
        assert_eq!(fmt(1.0), "1.0");
        assert_eq!(fmt(-2.5), "-2.5");
        assert_eq!(fmt(100.0), "100.0");
        assert_eq!(fmt(0.001), "0.001");
        assert_eq!(fmt(0.0001), "0.0001");
        assert_eq!(fmt(0.00001), "1e-05");
        assert_eq!(fmt(1e14), "100000000000000.0");
        assert_eq!(fmt(123456789012345.0), "123456789012345.0");
        assert_eq!(fmt(1e15), "1e+15");
        assert_eq!(fmt(1e16), "1e+16");
        assert_eq!(fmt(1.5e300), "1.5e+300");
        assert_eq!(fmt(f64::from(0.1f32)), "0.10000000149011612");
        assert_eq!(fmt(f64::from(0.7f32)), "0.699999988079071");
        assert_eq!(fmt(f64::from(std::f32::consts::PI)), "3.1415927410125732");
        assert_eq!(fmt(5e-324), "5e-324");
        assert_eq!(fmt(f64::NAN), "null");
    }

    #[test]
    fn round_trips() {
        let mut state = 0x1234_5678_9abc_def0u64;
        for _ in 0..200_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let value = f64::from_bits(state);
            if !value.is_finite() {
                continue;
            }
            let text = fmt(value);
            let parsed: f64 = text.parse().expect("parse");
            assert_eq!(parsed.to_bits(), value.to_bits(), "{value:e} -> {text}");
        }
    }
}
