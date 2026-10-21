## 2026-10-21: M6 PR 7, compressor (`sas-effects`, on `LinkwitzRileyFilter`)

Ported `effects/compressor.{h,cpp}` (`Compressor`, the upward/downward RMS
compressor on two bands at once, one per voice, and `MultibandCompressor`,
three bands split by 120 Hz and 2500 Hz `LinkwitzRileyFilter`s; as
upstream, it owns its filters and compressors and forwards sample rate,
oversampling and resets to them) and `modules/compressor_module.{h,cpp}`.
Built on Core Filters' `cursor/m5a-linkwitz-riley-dc-41d3` (#20), whose
commits this branch carries until #20 merges.

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

Parked, unchanged, under the coordinator's pending no-fast-math proposal
(awaiting the owner), in
`migration/corpus/modules/effects/pending/{multiband_compressor,compressor_module}.json`:
13 multiband-compressor cases and 6 module cases.

Loaded and passing both references: the low-band, high-band and
single-band modes (also at 2× oversampling), and two multiband module cases
(`defaults_saw`, −88.8 dB, and `interim_enable_toggle_short`, −82.9 dB).
Worst loaded `max_abs_error` 6.4e-5. **Coverage gap:** the multiband mode
has only those two module cases until the proposal is decided; the
reference cannot cover it more closely without the no-fast-math comparison.
