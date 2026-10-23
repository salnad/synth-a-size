// SPDX-License-Identifier: GPL-3.0-or-later

//! The features the model gets from a preview: compact enough for about 300
//! tokens, and independent of oscillator phase, so two renders of one patch
//! read the same apart from random modulation and noise.

use serde::Serialize;

use crate::fft::power_spectrum;

/// Envelope frames, in seconds.
const ENVELOPE_FRAME: f64 = 0.01;
/// The spectrum's FFT size and the spectrogram's shape.
const FFT_SIZE: usize = 2048;
pub const BANDS: usize = 16;
pub const SLICES: usize = 8;
const LOWEST_BAND_HZ: f64 = 40.0;
const HIGHEST_BAND_HZ: f64 = 16_000.0;
/// The spectrogram's range: ten steps of 6 dB below the loudest cell.
const SPECTROGRAM_RANGE_DB: f64 = 60.0;
/// Pitch search range and window.
const LOWEST_PITCH_HZ: f64 = 30.0;
const HIGHEST_PITCH_HZ: f64 = 2_000.0;
const PITCH_WINDOW: usize = 4096;
/// Below this, the render counts as silent.
const SILENCE_DB: f64 = -100.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Analysis {
    pub rendered_s: f64,
    pub released_at_s: f64,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub silent: bool,
    pub level: Level,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub envelope: Option<Envelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pitch: Option<Pitch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stereo: Option<Stereo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectrum: Option<Spectrum>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Level {
    pub peak_db: f64,
    pub rms_db: f64,
    /// Samples at or above full scale.
    pub clipped_samples: usize,
    /// NaN or infinite samples (counted, then read as silence).
    pub non_finite_samples: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Envelope {
    /// From the note-on to within 3 dB of the held peak.
    pub attack_ms: f64,
    pub peak_at_ms: f64,
    /// The level just before release, relative to the held peak.
    pub sustain_db: f64,
    /// From the peak until the level settles within 1 dB of the sustain.
    pub decay_ms: f64,
    /// From release until the level falls 40 dB below the level at release;
    /// `None` if it is still sounding at the end of the render.
    pub release_ms: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pitch {
    pub hz: f64,
    /// As a MIDI note number with cents (60 = middle C).
    pub midi_note: i32,
    pub cents: i32,
    /// The normalised autocorrelation at the period (1 = perfectly periodic).
    pub clarity: f64,
    /// Harmonic-to-noise ratio from the clarity.
    pub hnr_db: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stereo {
    /// Correlation of left and right (1 = mono, 0 = unrelated, -1 = inverted).
    pub correlation: f64,
    /// Side level relative to mid level.
    pub side_to_mid_db: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Spectrum {
    pub slice_ms: f64,
    /// Per slice; `None` for a slice 60 dB or more below the loudest.
    pub centroid_hz: Vec<Option<f64>>,
    /// The frequency below which 85% of the energy lies, per slice.
    pub rolloff_hz: Vec<Option<f64>>,
    /// One string per time slice, one digit per band (16 bands, 40 Hz to
    /// 16 kHz, log spaced, low to high); 9 is the loudest cell and each step
    /// down is 6 dB.
    pub spectrogram: Vec<String>,
}

fn db(power_or_amplitude: f64, power: bool) -> f64 {
    let floor = 1e-12;
    if power {
        10.0 * power_or_amplitude.max(floor).log10()
    } else {
        20.0 * power_or_amplitude.max(floor).log10()
    }
}

/// Rounds to `places` decimals, for compact output.
fn round(value: f64, places: i32) -> f64 {
    let scale = 10f64.powi(places);
    (value * scale).round() / scale
}

/// Analyses a stereo render at `sample_rate` whose notes were released at
/// frame `released_at`.
pub fn analyze(left: &[f32], right: &[f32], sample_rate: f64, released_at: usize) -> Analysis {
    let frames = left.len().min(right.len());
    let mut non_finite = 0;
    let mut clipped = 0;
    let mut peak = 0.0f64;
    let mut sum_squares = 0.0f64;
    let clean = |x: f32, non_finite: &mut usize| {
        if x.is_finite() {
            x
        } else {
            *non_finite += 1;
            0.0
        }
    };
    let mut l = Vec::with_capacity(frames);
    let mut r = Vec::with_capacity(frames);
    for i in 0..frames {
        let a = clean(left[i], &mut non_finite);
        let b = clean(right[i], &mut non_finite);
        for x in [a, b] {
            if x.abs() >= 1.0 {
                clipped += 1;
            }
            peak = peak.max(x.abs() as f64);
            sum_squares += (x as f64) * (x as f64);
        }
        l.push(a);
        r.push(b);
    }
    let rms = (sum_squares / (2 * frames.max(1)) as f64).sqrt();
    let level = Level {
        peak_db: round(db(peak, false), 1),
        rms_db: round(db(rms, false), 1),
        clipped_samples: clipped,
        non_finite_samples: non_finite,
    };
    let rendered_s = round(frames as f64 / sample_rate, 2);
    let released_at_s = round(released_at as f64 / sample_rate, 2);
    if db(peak, false) < SILENCE_DB {
        return Analysis {
            rendered_s,
            released_at_s,
            silent: true,
            level,
            envelope: None,
            pitch: None,
            stereo: None,
            spectrum: None,
        };
    }
    let mono: Vec<f32> = l.iter().zip(&r).map(|(a, b)| 0.5 * (a + b)).collect();
    Analysis {
        rendered_s,
        released_at_s,
        silent: false,
        level,
        envelope: Some(envelope(&mono, sample_rate, released_at.min(frames))),
        pitch: pitch(&mono, sample_rate, released_at.min(frames)),
        stereo: Some(stereo(&l, &r)),
        spectrum: Some(spectrum(&mono, sample_rate)),
    }
}

/// RMS levels in dB over [`ENVELOPE_FRAME`] frames.
fn envelope_db(mono: &[f32], sample_rate: f64) -> (Vec<f64>, usize) {
    let hop = ((ENVELOPE_FRAME * sample_rate) as usize).max(1);
    let levels = mono
        .chunks(hop)
        .map(|chunk| {
            let power =
                chunk.iter().map(|&x| (x as f64) * (x as f64)).sum::<f64>() / chunk.len() as f64;
            db(power, true)
        })
        .collect();
    (levels, hop)
}

fn envelope(mono: &[f32], sample_rate: f64, released_at: usize) -> Envelope {
    let (levels, hop) = envelope_db(mono, sample_rate);
    let ms = |frame: usize| round(frame as f64 * hop as f64 / sample_rate * 1000.0, 0);
    let held = (released_at / hop).clamp(1, levels.len());
    let (peak_frame, peak) = levels[..held].iter().copied().enumerate().fold(
        (0, f64::NEG_INFINITY),
        |best, (i, level)| {
            if level > best.1 {
                (i, level)
            } else {
                best
            }
        },
    );
    let attack = levels[..held]
        .iter()
        .position(|&level| level >= peak - 3.0)
        .unwrap_or(peak_frame);
    // The sustain: the average of the last 50 ms before release.
    let tail = 5.min(held);
    let sustain = levels[held - tail..held].iter().sum::<f64>() / tail as f64;
    let decay = if sustain < peak - 1.0 {
        levels[peak_frame..held]
            .iter()
            .position(|&level| level <= sustain + 1.0)
            .unwrap_or(held - peak_frame)
    } else {
        0
    };
    let at_release = sustain;
    let release = levels[held.min(levels.len())..]
        .iter()
        .position(|&level| level <= at_release - 40.0 || level <= SILENCE_DB);
    Envelope {
        attack_ms: ms(attack),
        peak_at_ms: ms(peak_frame),
        sustain_db: round(sustain - peak, 1),
        decay_ms: ms(decay),
        release_ms: release.map(ms),
    }
}

/// The strongest periodicity in the held part, by the normalised square
/// difference function (McLeod's pitch method).
fn pitch(mono: &[f32], sample_rate: f64, released_at: usize) -> Option<Pitch> {
    // Skip the first 100 ms (the attack) when the note is held that long.
    let start = ((0.1 * sample_rate) as usize).min(released_at.saturating_sub(PITCH_WINDOW));
    let end = (start + PITCH_WINDOW).min(released_at).min(mono.len());
    let window = &mono[start..end];
    let min_lag = (sample_rate / HIGHEST_PITCH_HZ) as usize;
    let max_lag = ((sample_rate / LOWEST_PITCH_HZ) as usize).min(window.len() / 2);
    if max_lag <= min_lag + 2 {
        return None;
    }
    let nsdf: Vec<f64> = (0..=max_lag)
        .map(|lag| {
            let mut acf = 0.0;
            let mut energy = 0.0;
            for i in 0..window.len() - lag {
                let (a, b) = (window[i] as f64, window[i + lag] as f64);
                acf += a * b;
                energy += a * a + b * b;
            }
            if energy > 0.0 {
                2.0 * acf / energy
            } else {
                0.0
            }
        })
        .collect();
    // Peaks after the first downward zero crossing; take the first within
    // 90% of the highest.
    let first_negative = nsdf.iter().position(|&v| v < 0.0)?;
    let mut peaks = Vec::new();
    let mut lag = first_negative.max(min_lag);
    while lag < max_lag {
        if nsdf[lag] > 0.0 && nsdf[lag] >= nsdf[lag - 1] && nsdf[lag] >= nsdf[lag + 1] {
            peaks.push(lag);
        }
        lag += 1;
    }
    let highest = peaks.iter().map(|&lag| nsdf[lag]).fold(0.0, f64::max);
    let &best = peaks.iter().find(|&&lag| nsdf[lag] >= 0.9 * highest)?;
    let clarity = nsdf[best];
    if clarity < 0.5 {
        return None;
    }
    // Parabolic interpolation around the peak.
    let (a, b, c) = (nsdf[best - 1], nsdf[best], nsdf[best + 1]);
    let denominator = a - 2.0 * b + c;
    let offset = if denominator.abs() > 1e-12 {
        0.5 * (a - c) / denominator
    } else {
        0.0
    };
    let hz = sample_rate / (best as f64 + offset);
    let note = 69.0 + 12.0 * (hz / 440.0).log2();
    let midi_note = note.round() as i32;
    let clarity = clarity.min(0.999_999);
    Some(Pitch {
        hz: round(hz, 1),
        midi_note,
        cents: ((note - midi_note as f64) * 100.0).round() as i32,
        clarity: round(clarity, 3),
        hnr_db: round(10.0 * (clarity / (1.0 - clarity)).log10(), 1),
    })
}

fn stereo(l: &[f32], r: &[f32]) -> Stereo {
    let (mut ll, mut rr, mut lr, mut mid, mut side) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for (&a, &b) in l.iter().zip(r) {
        let (a, b) = (a as f64, b as f64);
        ll += a * a;
        rr += b * b;
        lr += a * b;
        mid += (0.5 * (a + b)).powi(2);
        side += (0.5 * (a - b)).powi(2);
    }
    let correlation = if ll > 0.0 && rr > 0.0 {
        lr / (ll * rr).sqrt()
    } else {
        1.0
    };
    Stereo {
        correlation: round(correlation, 2),
        side_to_mid_db: round(db(side / mid.max(1e-24), true).max(-120.0), 1),
    }
}

/// The band edges, log spaced from [`LOWEST_BAND_HZ`] to [`HIGHEST_BAND_HZ`].
fn band_edges() -> [f64; BANDS + 1] {
    let ratio = (HIGHEST_BAND_HZ / LOWEST_BAND_HZ).powf(1.0 / BANDS as f64);
    std::array::from_fn(|i| LOWEST_BAND_HZ * ratio.powi(i as i32))
}

fn spectrum(mono: &[f32], sample_rate: f64) -> Spectrum {
    let slice_len = mono.len().div_ceil(SLICES).max(1);
    let edges = band_edges();
    let bin_hz = sample_rate / FFT_SIZE as f64;
    let mut centroids = Vec::with_capacity(SLICES);
    let mut rolloffs = Vec::with_capacity(SLICES);
    let mut bands = Vec::with_capacity(SLICES);
    let mut totals = Vec::with_capacity(SLICES);
    let mut frame = vec![0.0f32; FFT_SIZE];
    for slice in 0..SLICES {
        let start = (slice * slice_len).min(mono.len());
        let end = ((slice + 1) * slice_len).min(mono.len());
        // The average power spectrum of half-overlapping frames in the slice
        // (one zero-padded frame if the slice is shorter than the FFT).
        let mut power = vec![0.0f64; FFT_SIZE / 2 + 1];
        let mut count = 0;
        let mut at = start;
        loop {
            frame.fill(0.0);
            let n = (end.saturating_sub(at)).min(FFT_SIZE);
            frame[..n].copy_from_slice(&mono[at..at + n]);
            for (sum, p) in power.iter_mut().zip(power_spectrum(&frame)) {
                *sum += p;
            }
            count += 1;
            at += FFT_SIZE / 2;
            if at + FFT_SIZE > end {
                break;
            }
        }
        for p in &mut power {
            *p /= count as f64;
        }
        let total: f64 = power.iter().skip(1).sum();
        let centroid = if total > 0.0 {
            power
                .iter()
                .enumerate()
                .skip(1)
                .map(|(k, p)| k as f64 * bin_hz * p)
                .sum::<f64>()
                / total
        } else {
            0.0
        };
        let mut running = 0.0;
        let rolloff_bin = power
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, p)| {
                running += **p;
                running >= 0.85 * total
            })
            .map(|(k, _)| k)
            .unwrap_or(0);
        centroids.push(round(centroid, -1));
        rolloffs.push(round(rolloff_bin as f64 * bin_hz, -1));
        totals.push(total);
        let cells: Vec<f64> = (0..BANDS)
            .map(|band| {
                let energy: f64 = power
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| {
                        let hz = *k as f64 * bin_hz;
                        hz >= edges[band] && hz < edges[band + 1]
                    })
                    .map(|(_, p)| p)
                    .sum();
                db(energy, true)
            })
            .collect();
        bands.push(cells);
    }
    let loudest = bands
        .iter()
        .flatten()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let step = SPECTROGRAM_RANGE_DB / 10.0;
    let spectrogram = bands
        .iter()
        .map(|cells| {
            cells
                .iter()
                .map(|&cell| {
                    let level = ((cell - loudest + SPECTROGRAM_RANGE_DB) / step).floor();
                    char::from(b'0' + level.clamp(0.0, 9.0) as u8)
                })
                .collect()
        })
        .collect();
    let loudest_total = totals.iter().copied().fold(0.0, f64::max);
    let audible = |slice: usize| totals[slice] > 0.0 && totals[slice] >= loudest_total * 1e-6;
    Spectrum {
        slice_ms: round(slice_len as f64 / sample_rate * 1000.0, 0),
        centroid_hz: (0..SLICES)
            .map(|i| audible(i).then_some(centroids[i]))
            .collect(),
        rolloff_hz: (0..SLICES)
            .map(|i| audible(i).then_some(rolloffs[i]))
            .collect(),
        spectrogram,
    }
}
