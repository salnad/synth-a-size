## 2026-10-21: M6 PR 7, compressor (`sas-effects`, on `LinkwitzRileyFilter`)

Ported `effects/compressor.{h,cpp}` (`Compressor`, the upward/downward RMS
compressor on two bands at once, one per voice, and `MultibandCompressor`,
three bands split by 120 Hz and 2500 Hz `LinkwitzRileyFilter`s; as
upstream, it owns its filters and compressors and forwards sample rate,
oversampling and resets to them) and `modules/compressor_module.{h,cpp}`.
Built on Core Filters' `LinkwitzRileyFilter` (#20, merged).

Captures: `effects/multiband_compressor` (all seven outputs: audio and the
six band mean squares) and `effects/compressor_module`: every band mode,
upper and lower thresholds −80→0, ratios, gains −30→30, attack and release
0→1, mix, band switching, quiet expansion, loud input, 48/96 kHz and 2×
oversampling. All under `module_defaults`.

**The port is bit-identical to the `REF_FAST_MATH=OFF` build on every
output.** The full multiband path (both crossovers in series) misses the
module gates against the fast-math reference from the first block, at 1×
as well as 2×, and the proxy misses by the same amounts, so 0.1 s interim
versions fail too. This is Core Filters' crossover finding (fast-math
reassociates a coefficient formula near-unstable poles), already visible at
1× in the series 120 Hz → 2500 Hz configuration. Worst proxy noise:
`compressor_module/gain_attack_release_mix` 1.0e-3 / −71.2 dB audio; the
high-band mean squares (around −90 dB) are off by up to −23.5 dB relative.

Parked, unchanged, under SAS-35 (exact comparison against no-fast-math;
approved, on main with #51), in
`migration/corpus/modules/effects/pending/{multiband_compressor,compressor_module}.json`:
13 multiband-compressor cases and 6 module cases. They return in the
SAS-35 re-enable PR.

Loaded and passing both references: the low-band, high-band and
single-band modes (also at 2× oversampling), and two multiband module cases
(`defaults_saw`, −88.8 dB, and `interim_enable_toggle_short`, −82.9 dB).
Worst loaded `max_abs_error` 6.4e-5. **Coverage gap:** the multiband mode
has only those two module cases until the re-enable PR; in multiband mode the
reference cannot cover it more closely without the no-fast-math comparison.

**Interim single-band coverage** (cases named `interim_*`, review): the
crossover noise only affects multiband mode. With `enabled_bands: 3` (single
band, no crossovers) the parked parameter sweeps are well-conditioned, so 9
cases run them live in `effects/multiband_compressor`. They are
`interim_single_band_{upper_threshold_sweeps, lower_threshold_sweeps,
ratio_sweeps, gain_sweeps, attack_release_sweeps, mix_sweep,
quiet_expansion}` (the parked inputs plus `enabled_bands: 3`),
`interim_band_switching_from_1` (`enabled_bands` 1→3.99) and
`interim_single_band_negative_lower_ratio` (quiet noise at amp 0.01, all
lower ratios −1). Official vs no-fast-math passes on all 9 (x86), and the
port is bit-identical to no-fast-math on them. Against the official reference
the worst is `interim_band_switching_from_1` audio at 3.8e-5 / −90.1 dB. They
catch three clamp mutants that survived before: `MIN_THRESHOLD` −100→−60,
`MAX_GAIN` 30→20, and the lower-ratio clamp −1→0. The last one fails
`interim_single_band_negative_lower_ratio` at max abs 4.2e-2. Low-band
(`enabled_bands: 1`) variants are not added, because they still go through
the 120 Hz crossover.
