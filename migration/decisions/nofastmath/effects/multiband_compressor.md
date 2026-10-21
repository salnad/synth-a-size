# No-fast-math exact cases: effects/multiband_compressor

MultibandCompressor. In multiband mode the 120 Hz and 2500 Hz Linkwitz-Riley crossovers run in series, and fast-math reassociates their coefficient formula near-unstable poles, so the official reference misses the module gates from the first block (Core Filters' crossover finding; migration/ledger/2026-10-21-m6-compressor.md). The port is bit-identical to the no-fast-math build on these cases.

- `effects/multiband_compressor/multiband_noise` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/multiband_saw_loud` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/multiband_quiet_expansion` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/upper_threshold_sweeps` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/lower_threshold_sweeps` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/ratio_sweeps` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/gain_sweeps` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/attack_release_sweeps` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/mix_sweep` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/band_switching` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/rate_48k` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/rate_96k` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/multiband_compressor/oversampled_2x` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
