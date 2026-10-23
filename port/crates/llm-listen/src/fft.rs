// SPDX-License-Identifier: GPL-3.0-or-later

//! A small in-place radix-2 FFT, enough for the analysis's spectra.

use std::f64::consts::PI;

/// Transforms `re` and `im` (equal power-of-two lengths) in place.
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    assert!(n.is_power_of_two() && im.len() == n);
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let angle = -2.0 * PI / len as f64;
        let (w_im, w_re) = angle.sin_cos();
        for start in (0..n).step_by(len) {
            let (mut cur_re, mut cur_im) = (1.0, 0.0);
            for k in 0..len / 2 {
                let a = start + k;
                let b = a + len / 2;
                let t_re = re[b] * cur_re - im[b] * cur_im;
                let t_im = re[b] * cur_im + im[b] * cur_re;
                re[b] = re[a] - t_re;
                im[b] = im[a] - t_im;
                re[a] += t_re;
                im[a] += t_im;
                let next = cur_re * w_re - cur_im * w_im;
                cur_im = cur_re * w_im + cur_im * w_re;
                cur_re = next;
            }
        }
        len <<= 1;
    }
}

/// The power spectrum (bins 0..=n/2) of `frame` under a Hann window.
pub fn power_spectrum(frame: &[f32]) -> Vec<f64> {
    let n = frame.len();
    let mut re: Vec<f64> = frame
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let w = 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos();
            x as f64 * w
        })
        .collect();
    let mut im = vec![0.0; n];
    fft(&mut re, &mut im);
    (0..=n / 2).map(|k| re[k] * re[k] + im[k] * im[k]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_lands_in_its_bin() {
        let n = 1024;
        let frame: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 64.0 * i as f64 / n as f64).sin() as f32)
            .collect();
        let power = power_spectrum(&frame);
        let peak = (0..power.len())
            .max_by(|&a, &b| power[a].total_cmp(&power[b]))
            .unwrap();
        assert_eq!(peak, 64);
    }
}
