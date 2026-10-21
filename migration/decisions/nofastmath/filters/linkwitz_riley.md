# No-fast-math exact cases: filters/linkwitz_riley

LinkwitzRileyFilter. GCC fast-math reassociates `1 + √2·warp + warp²` in `computeCoefficients`; at a low cutoff/rate ratio the poles sit next to the unit circle and the recursion amplifies the 1-ULP coefficient difference (LEDGER 2026-10-21 M5a PR 2). The port is bit-identical to the no-fast-math build on these cases.

- `filters/linkwitz_riley/noise_120hz_44_1k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_44_1k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_48k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_48k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_44_1k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_44_1k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_48k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_48k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_88_2k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_88_2k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_96k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_96k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_176_4k_x1` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_176_4k_x1` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_192k_x1` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_192k_x1` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_88_2k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_88_2k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_96k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_96k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_176_4k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_176_4k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_192k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_192k_x2` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_176_4k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_176_4k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_120hz_192k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_120hz_192k_x4` (SAS-35; x86): 120 Hz crossover at an oversampled rate: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/noise_2500hz_176_4k_x4` (SAS-35; x86): 2500 Hz crossover at 705.6 kHz: poles next to the unit circle amplify the reassociated coefficient
- `filters/linkwitz_riley/saw_2500hz_176_4k_x4` (SAS-35; x86): 2500 Hz crossover at 705.6 kHz: poles next to the unit circle amplify the reassociated coefficient
