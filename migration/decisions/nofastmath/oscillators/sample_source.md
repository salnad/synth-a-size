# No-fast-math exact cases: oscillators/sample_source

With a playback ratio other than 1, the playback increment
(`centsToRatio(transpose) * sample_rate_ratio`) is rounded differently under
fast-math. The sample position integrates the increment, so the official
reference drifts away from its own no-fast-math build. The port matches the
no-fast-math build bit for bit. The increment itself is compared on the
official reference in `sample_source_pitch`.

On macOS arm64 only `noise_transpose_40` misses the limits (the largest
transpose); the others pass there and are declared `x86` only (Reference ARM
run 39661942644).

- `oscillators/sample_source/noise_transpose_m13` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/noise_transpose_7` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/noise_transpose_19` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/noise_transpose_40` (SAS-35; x86, macos): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/noise_keytrack_off` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/saw_random_phase` (SAS-35; x86): random start phase: the start position's fast-math rounding offsets the whole render
- `oscillators/sample_source/stereo` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/long_96k_at_48k` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/saw_at_96k` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
- `oscillators/sample_source/block_size_17` (SAS-35; x86): non-unity playback ratio: the sample position integrates the fast-math rounding of the playback increment
