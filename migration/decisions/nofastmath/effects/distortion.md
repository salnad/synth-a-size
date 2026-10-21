# No-fast-math exact cases: effects/distortion

Distortion. With the drive swept, the downsample period changes every sample, and a last-bit difference in it moves the sample-and-hold update by a frame (migration/ledger/2026-10-21-m6-distortion.md). The port is bit-identical to the no-fast-math build on these cases.

- `effects/distortion/down_sample_drive_sweep_48k` (SAS-35; x86): swept drive: a last-bit period difference moves the downsample sample-and-hold by a frame
- `effects/distortion/down_sample_96k` (SAS-35; x86): swept drive: a last-bit period difference moves the downsample sample-and-hold by a frame
- `effects/distortion/interim_down_sample_drive_sweep_short` (SAS-35; macos): swept drive: a last-bit period difference moves the downsample sample-and-hold by a frame
