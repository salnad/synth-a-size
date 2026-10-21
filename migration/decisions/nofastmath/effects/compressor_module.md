# No-fast-math exact cases: effects/compressor_module

CompressorModule. In multiband mode the 120 Hz and 2500 Hz Linkwitz-Riley crossovers run in series, and fast-math reassociates their coefficient formula near-unstable poles, so the official reference misses the module gates from the first block (Core Filters' crossover finding; migration/ledger/2026-10-21-m6-compressor.md). The port is bit-identical to the no-fast-math build on these cases.

- `effects/compressor_module/defaults_noise` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/compressor_module/threshold_and_ratio_sweeps` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/compressor_module/gain_attack_release_mix` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/compressor_module/enable_toggle` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/compressor_module/rate_96k` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
- `effects/compressor_module/oversampled_2x` (SAS-35; x86): the series 120 Hz and 2500 Hz crossovers amplify fast-math coefficient rounding
