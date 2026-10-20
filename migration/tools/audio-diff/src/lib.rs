//! Differential comparison of a test render against a reference render.
//!
//! Metrics (all computed per channel, then reduced to the worst channel):
//!
//! * `max_abs_error` — largest `|test - ref|` sample difference.
//! * `rms_diff_db` — `|20·log10(rms(test) / rms(ref))|`: overall level mismatch.
//! * `residual_db` — `20·log10(rms(test - ref) / rms(ref))`: null-test depth;
//!   more negative is better, `-inf` for identical signals. When the reference
//!   channel is silent the residual is reported in dBFS instead.
//! * `lsd_db` — band-wise log-spectral distance in dB: STFT (Hann,
//!   `fft_size`/`hop`), bin powers summed into 1/6-octave bands, bands more
//!   more than 80 dB below each signal's frame peak ignored, RMS dB difference over bands,
//!   averaged over frames that are not silent (`silence_db`) in both signals.
//!
//! A metric is only checked when its tolerance is set. See
//! `migration/tools/audio-diff/README.md` for the CLI contract.

pub mod audio;

use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};

pub use audio::{load, Audio, LoadError};

/// Pass/fail limits. `None` means "report only".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Tolerances {
    pub max_abs_error: Option<f64>,
    pub rms_diff_db: Option<f64>,
    pub residual_db: Option<f64>,
    pub lsd_db: Option<f64>,
}

impl Tolerances {
    /// Fills unset fields from `other`.
    pub fn or(self, other: &Tolerances) -> Tolerances {
        Tolerances {
            max_abs_error: self.max_abs_error.or(other.max_abs_error),
            rms_diff_db: self.rms_diff_db.or(other.rms_diff_db),
            residual_db: self.residual_db.or(other.residual_db),
            lsd_db: self.lsd_db.or(other.lsd_db),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Options {
    pub fft_size: usize,
    pub hop: usize,
    /// Frames quieter than this (dBFS, both signals) are excluded from LSD.
    pub silence_db: f64,
    /// Log-spaced bands per octave for the spectral metric.
    pub bands_per_octave: usize,
    /// Per-frame dynamic range of the spectral metric (dB below loudest band).
    pub dynamic_range_db: f64,
    /// Compare only the common prefix when lengths differ instead of failing.
    pub allow_length_mismatch: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fft_size: 2048,
            hop: 512,
            silence_db: -100.0,
            bands_per_octave: 6,
            dynamic_range_db: 80.0,
            allow_length_mismatch: false,
        }
    }
}

/// JSON has no infinities; write non-finite metrics as "inf" / "-inf" / "nan".
fn ser_metric<S: serde::Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_finite() {
        serializer.serialize_f64(*value)
    } else if value.is_nan() {
        serializer.serialize_str("nan")
    } else if *value > 0.0 {
        serializer.serialize_str("inf")
    } else {
        serializer.serialize_str("-inf")
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChannelMetrics {
    #[serde(serialize_with = "ser_metric")]
    pub max_abs_error: f64,
    pub max_abs_error_frame: usize,
    #[serde(serialize_with = "ser_metric")]
    pub rms_ref_db: f64,
    #[serde(serialize_with = "ser_metric")]
    pub rms_test_db: f64,
    #[serde(serialize_with = "ser_metric")]
    pub rms_diff_db: f64,
    #[serde(serialize_with = "ser_metric")]
    pub residual_db: f64,
    #[serde(serialize_with = "ser_metric")]
    pub lsd_db: f64,
    pub lsd_frames: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Check {
    pub metric: String,
    #[serde(serialize_with = "ser_metric")]
    pub value: f64,
    pub limit: f64,
    pub pass: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Report {
    pub channels: usize,
    pub frames_ref: usize,
    pub frames_test: usize,
    pub frames_compared: usize,
    pub sample_rate: Option<u32>,
    pub per_channel: Vec<ChannelMetrics>,
    /// Worst value over channels for each metric.
    #[serde(serialize_with = "ser_metric")]
    pub max_abs_error: f64,
    #[serde(serialize_with = "ser_metric")]
    pub rms_diff_db: f64,
    #[serde(serialize_with = "ser_metric")]
    pub residual_db: f64,
    #[serde(serialize_with = "ser_metric")]
    pub lsd_db: f64,
    pub tolerances: Tolerances,
    pub checks: Vec<Check>,
    pub pass: bool,
}

#[derive(Debug)]
pub enum CompareError {
    ChannelMismatch { reference: usize, test: usize },
    SampleRateMismatch { reference: u32, test: u32 },
    LengthMismatch { reference: usize, test: usize },
    Empty,
}

impl std::fmt::Display for CompareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChannelMismatch { reference, test } => {
                write!(
                    f,
                    "channel count differs: reference {reference}, test {test}"
                )
            }
            Self::SampleRateMismatch { reference, test } => {
                write!(f, "sample rate differs: reference {reference}, test {test}")
            }
            Self::LengthMismatch { reference, test } => write!(
                f,
                "length differs: reference {reference} frames, test {test} frames \
                 (use --allow-length-mismatch to compare the common prefix)"
            ),
            Self::Empty => write!(f, "nothing to compare (zero frames)"),
        }
    }
}

impl std::error::Error for CompareError {}

fn to_db(ratio: f64) -> f64 {
    if ratio > 0.0 {
        20.0 * ratio.log10()
    } else {
        f64::NEG_INFINITY
    }
}

fn rms(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|x| x * x).sum::<f64>() / samples.len() as f64).sqrt()
}

/// Band-wise log-spectral distance (dB) between two equal-length mono signals.
///
/// Per STFT frame, bin powers are summed into log-spaced bands
/// (`bands_per_octave`, from the first non-DC bin to Nyquist). Bands more than
/// `dynamic_range_db` below that signal's loudest band are clamped to that
/// floor, and bands at the floor in both signals are skipped, so near-empty
/// bins cannot dilute the score. A frame's distance is the RMS over counted bands
/// of `10·log10(P_ref / P_test)`; the result is the mean over non-silent
/// frames. A pure gain change of g dB therefore scores exactly |g|.
fn log_spectral_distance(reference: &[f64], test: &[f64], options: &Options) -> (f64, usize) {
    let n = options.fft_size.max(16);
    let hop = options.hop.max(1);
    let window: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())
        .collect();
    let silence_power = 10f64.powf(options.silence_db / 10.0);
    let range = 10f64.powf(-options.dynamic_range_db / 10.0);

    let bins = n / 2 + 1;
    let bands = band_edges(bins, options.bands_per_octave.max(1));

    let fft = FftPlanner::<f64>::new().plan_fft_forward(n);
    let mut buffer_ref = vec![Complex::new(0.0, 0.0); n];
    let mut buffer_test = vec![Complex::new(0.0, 0.0); n];
    let mut band_ref = vec![0.0; bands.len()];
    let mut band_test = vec![0.0; bands.len()];

    let len = reference.len();
    let num_frames = if len <= n {
        1
    } else {
        1 + (len - n).div_ceil(hop)
    };
    let mut total = 0.0;
    let mut counted = 0usize;

    for frame in 0..num_frames {
        let start = frame * hop;
        let mut energy_ref = 0.0;
        let mut energy_test = 0.0;
        for i in 0..n {
            let (r, t) = if start + i < len {
                (reference[start + i], test[start + i])
            } else {
                (0.0, 0.0)
            };
            energy_ref += r * r;
            energy_test += t * t;
            buffer_ref[i] = Complex::new(r * window[i], 0.0);
            buffer_test[i] = Complex::new(t * window[i], 0.0);
        }
        if energy_ref / (n as f64) < silence_power && energy_test / (n as f64) < silence_power {
            continue;
        }

        fft.process(&mut buffer_ref);
        fft.process(&mut buffer_test);

        let mut loudest_ref = 0.0f64;
        let mut loudest_test = 0.0f64;
        for (b, &(lo, hi)) in bands.iter().enumerate() {
            band_ref[b] = buffer_ref[lo..hi].iter().map(|c| c.norm_sqr()).sum();
            band_test[b] = buffer_test[lo..hi].iter().map(|c| c.norm_sqr()).sum();
            loudest_ref = loudest_ref.max(band_ref[b]);
            loudest_test = loudest_test.max(band_test[b]);
        }
        // Each signal's floor follows its own loudest band, so a pure gain
        // change scales both floors identically; a signal far quieter than the
        // other is floored at twice the range below the louder one.
        let loudest = loudest_ref.max(loudest_test);
        let floor_ref = (loudest_ref.max(loudest * range) * range).max(f64::MIN_POSITIVE);
        let floor_test = (loudest_test.max(loudest * range) * range).max(f64::MIN_POSITIVE);

        let mut sum_sq = 0.0;
        let mut used = 0usize;
        for b in 0..bands.len() {
            if band_ref[b] <= floor_ref && band_test[b] <= floor_test {
                continue;
            }
            let diff = 10.0 * (band_ref[b].max(floor_ref) / band_test[b].max(floor_test)).log10();
            sum_sq += diff * diff;
            used += 1;
        }
        if used > 0 {
            total += (sum_sq / used as f64).sqrt();
            counted += 1;
        }
    }

    if counted == 0 {
        (0.0, 0)
    } else {
        (total / counted as f64, counted)
    }
}

/// Half-open bin ranges of log-spaced bands covering bins `1..bins`.
fn band_edges(bins: usize, bands_per_octave: usize) -> Vec<(usize, usize)> {
    let ratio = 2f64.powf(1.0 / bands_per_octave as f64);
    let mut edges = Vec::new();
    let mut lo = 1usize;
    let mut upper = 1.0f64;
    while lo < bins {
        upper *= ratio;
        let hi = (upper.round() as usize).max(lo + 1).min(bins);
        edges.push((lo, hi));
        lo = hi;
    }
    edges
}

fn channel_metrics(reference: &[f64], test: &[f64], options: &Options) -> ChannelMetrics {
    let mut max_abs_error = 0.0f64;
    let mut max_abs_error_frame = 0usize;
    let mut residual = Vec::with_capacity(reference.len());
    for (i, (r, t)) in reference.iter().zip(test).enumerate() {
        let d = t - r;
        // NaN in either signal must never compare as "within tolerance".
        let magnitude = if d.is_nan() { f64::INFINITY } else { d.abs() };
        if magnitude > max_abs_error {
            max_abs_error = magnitude;
            max_abs_error_frame = i;
        }
        residual.push(d);
    }

    let rms_ref = rms(reference);
    let rms_test = rms(test);
    let rms_residual = rms(&residual);

    let rms_diff_db = if rms_ref == 0.0 && rms_test == 0.0 {
        0.0
    } else if rms_ref == 0.0 || rms_test == 0.0 {
        f64::INFINITY
    } else {
        (to_db(rms_test) - to_db(rms_ref)).abs()
    };
    let residual_db = if rms_residual.is_nan() {
        f64::INFINITY
    } else if rms_ref > 0.0 {
        to_db(rms_residual / rms_ref)
    } else {
        to_db(rms_residual)
    };
    let (lsd_db, lsd_frames) = log_spectral_distance(reference, test, options);

    ChannelMetrics {
        max_abs_error,
        max_abs_error_frame,
        rms_ref_db: to_db(rms_ref),
        rms_test_db: to_db(rms_test),
        rms_diff_db: if rms_diff_db.is_nan() {
            f64::INFINITY
        } else {
            rms_diff_db
        },
        residual_db,
        lsd_db: if lsd_db.is_nan() {
            f64::INFINITY
        } else {
            lsd_db
        },
        lsd_frames,
    }
}

/// Compares `test` against `reference`.
pub fn compare(
    reference: &Audio,
    test: &Audio,
    tolerances: &Tolerances,
    options: &Options,
) -> Result<Report, CompareError> {
    if reference.channels != test.channels {
        return Err(CompareError::ChannelMismatch {
            reference: reference.channels,
            test: test.channels,
        });
    }
    if let (Some(a), Some(b)) = (reference.sample_rate, test.sample_rate) {
        if a != b {
            return Err(CompareError::SampleRateMismatch {
                reference: a,
                test: b,
            });
        }
    }
    if reference.frames != test.frames && !options.allow_length_mismatch {
        return Err(CompareError::LengthMismatch {
            reference: reference.frames,
            test: test.frames,
        });
    }
    let frames = reference.frames.min(test.frames);
    if frames == 0 {
        return Err(CompareError::Empty);
    }

    let per_channel: Vec<ChannelMetrics> = (0..reference.channels)
        .map(|c| {
            let r: Vec<f64> = reference.channel(c).take(frames).map(f64::from).collect();
            let t: Vec<f64> = test.channel(c).take(frames).map(f64::from).collect();
            channel_metrics(&r, &t, options)
        })
        .collect();

    let worst =
        |f: fn(&ChannelMetrics) -> f64| per_channel.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
    let max_abs_error = worst(|m| m.max_abs_error);
    let rms_diff_db = worst(|m| m.rms_diff_db);
    let residual_db = worst(|m| m.residual_db);
    let lsd_db = worst(|m| m.lsd_db);

    let mut checks = Vec::new();
    let mut check = |metric: &str, value: f64, limit: Option<f64>| {
        if let Some(limit) = limit {
            checks.push(Check {
                metric: metric.to_string(),
                value,
                limit,
                pass: value <= limit,
            });
        }
    };
    check("max_abs_error", max_abs_error, tolerances.max_abs_error);
    check("rms_diff_db", rms_diff_db, tolerances.rms_diff_db);
    check("residual_db", residual_db, tolerances.residual_db);
    check("lsd_db", lsd_db, tolerances.lsd_db);

    let length_ok = reference.frames == test.frames;
    let pass = checks.iter().all(|c| c.pass) && length_ok;

    Ok(Report {
        channels: reference.channels,
        frames_ref: reference.frames,
        frames_test: test.frames,
        frames_compared: frames,
        sample_rate: reference.sample_rate.or(test.sample_rate),
        per_channel,
        max_abs_error,
        rms_diff_db,
        residual_db,
        lsd_db,
        tolerances: tolerances.clone(),
        checks,
        pass,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, freq: f64, amp: f64, channels: usize) -> Audio {
        let mut data = Vec::with_capacity(frames * channels);
        for i in 0..frames {
            let v = (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / 48000.0).sin()) as f32;
            for _ in 0..channels {
                data.push(v);
            }
        }
        Audio::new(channels, Some(48000), data)
    }

    fn strict() -> Tolerances {
        Tolerances {
            max_abs_error: Some(1e-6),
            rms_diff_db: Some(0.01),
            residual_db: Some(-100.0),
            lsd_db: Some(0.1),
        }
    }

    #[test]
    fn identical_signals_pass_strict_tolerances() {
        let a = sine(48000, 440.0, 0.5, 2);
        let report = compare(&a, &a, &strict(), &Options::default()).unwrap();
        assert!(report.pass, "{report:?}");
        assert_eq!(report.max_abs_error, 0.0);
        assert_eq!(report.residual_db, f64::NEG_INFINITY);
        assert!(report.lsd_db.abs() < 1e-9);
    }

    #[test]
    fn half_gain_is_six_db() {
        let a = sine(48000, 440.0, 0.5, 1);
        let b = sine(48000, 440.0, 0.25, 1);
        let report = compare(&a, &b, &strict(), &Options::default()).unwrap();
        assert!(!report.pass);
        assert!(
            (report.rms_diff_db - 6.0206).abs() < 1e-3,
            "{}",
            report.rms_diff_db
        );
        assert!((report.residual_db - (-6.0206)).abs() < 1e-3);
        assert!((report.max_abs_error - 0.25).abs() < 1e-4);
        assert!((report.lsd_db - 6.0206).abs() < 1e-6, "{}", report.lsd_db);
    }

    #[test]
    fn different_pitch_has_large_spectral_distance() {
        let a = sine(48000, 440.0, 0.5, 1);
        let b = sine(48000, 880.0, 0.5, 1);
        let report = compare(&a, &b, &Tolerances::default(), &Options::default()).unwrap();
        assert!(report.rms_diff_db < 0.01);
        assert!(report.lsd_db > 20.0, "{}", report.lsd_db);
        assert!(report.pass, "no tolerances set means report-only");
    }

    #[test]
    fn nan_never_passes() {
        let a = sine(4800, 440.0, 0.5, 1);
        let mut b = a.clone();
        b.data[100] = f32::NAN;
        let report = compare(&a, &b, &strict(), &Options::default()).unwrap();
        assert!(!report.pass);
        assert!(report.max_abs_error.is_infinite());
    }

    #[test]
    fn length_mismatch_is_an_error_unless_allowed() {
        let a = sine(4800, 440.0, 0.5, 1);
        let b = a.truncated(4000);
        assert!(matches!(
            compare(&a, &b, &strict(), &Options::default()),
            Err(CompareError::LengthMismatch { .. })
        ));
        let options = Options {
            allow_length_mismatch: true,
            ..Options::default()
        };
        let report = compare(&a, &b, &strict(), &options).unwrap();
        assert_eq!(report.frames_compared, 4000);
        assert!(!report.pass, "a length mismatch still fails the comparison");
    }

    #[test]
    fn silent_signals_compare_equal() {
        let a = Audio::new(2, Some(44100), vec![0.0; 8820]);
        let report = compare(&a, &a, &strict(), &Options::default()).unwrap();
        assert!(report.pass);
        assert_eq!(report.per_channel[0].lsd_frames, 0);
    }
}
