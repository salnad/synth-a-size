## 2026-10-21: M6 PR 3, distortion processor (`sas-effects`)

Ported `effects/distortion.{h,cpp}` (`Distortion`: soft clip, hard clip,
linear fold, sine fold, bit crush, downsample). As upstream does, it
compacts voice 0 two frames per `poly_float`, distorts, and expands with
voice 1 copied from voice 0; the drive output keeps the compacted drive.
`DistortionModule` needs `DigitalSvf` and follows with the filter PRs.

Captures (`effects/distortion`, the M0 case plus 12): every type with drive
swept over the full −30→30 dB parameter range, drive −60→60 dB (clamping),
a type sweep 0→6 that crosses every type and the out-of-range passthrough,
odd block sizes (63 and 37, odd frame counts), downsampling at 48 kHz,
96 kHz and with 2× oversampling. All under `module_defaults`.

Metrics (x86_64): 22 outputs pass; 17 are bit-identical. Worst
`max_abs_error` 1.5e-6 and residual −134.2 dB (`sin_fold_drive_sweep`).
The port is bit-identical to the `REF_FAST_MATH=OFF` build on all 26
outputs, the parked ones included.

Parked under the coordinator's pending proposal, awaiting the owner (exact
comparison against no-fast-math only where the official reference fails the limits against its own no-fast-math
build): `effects/distortion/down_sample_drive_sweep_48k` (proxy 5.1e-2,
−62.3 dB) and `effects/distortion/down_sample_96k` (9.3e-1, −7.3 dB), in
`migration/corpus/modules/effects/pending/distortion.json` with their inputs.
With the drive swept, the downsample period changes every sample, and a
last-bit difference in it moves the sample-and-hold update by a frame.
That is a discrete timing change, not noise that scales with the signal. The
constant-drive downsample case (2× oversampling) is loaded and
bit-identical. Alternatives as for the reverb: per-case exceptions or
render-style gates.

**Interim coverage** (cases named `interim_*`, reviewer request): downsample
at 48 kHz and 96 kHz with constant drive, with drive stepping between −15
and 25 dB at 4 Hz, and a 0.1 s drive sweep −30→30 at 48 kHz. All 8 outputs
are bit-identical to the reference and the proxy.

`effects/distortion/down_sample_reentry` (review nit): `type` switches
5 → 0 → 5 at 10 Hz, so downsampling is re-entered from a non-zero state and
the `current_samples_` reset on a type change is pinned (a mutant without it
fails at −17.6 dB). Port 3.0e-7 / −143.9 dB. With it and the interim cases,
32 outputs pass and 26 are bit-identical.

**macOS arm64 (after #2):** `effects/distortion/interim_down_sample_drive_sweep_short`
misses the macOS no-fast-math proxy (max abs 0.18, residual −44 dB) although
it is bit-identical on x86. It is the same swept-drive sample-and-hold timing
effect as the two parked cases. Per `migration/AGENTS.md` it is parked with
its inputs unchanged in
`migration/corpus/modules/effects/pending/distortion_arm64.json`, its x86 hash
entries are removed, and no tolerance changes. It returns with the other two
under SAS-35 (`reference: nofastmath`). 30 outputs stay loaded on x86. Every
other distortion output passes the macOS proxy (Reference ARM run
39622357646), and the macOS arm64 hashes come from that run's
`ref-arm64-macos-15` artifact.
