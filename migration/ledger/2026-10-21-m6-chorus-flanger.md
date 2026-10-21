## 2026-10-21: M6 PR 4, chorus and flanger modules (`sas-effects`)

Ported `modules/chorus_module.{h,cpp}` (up to four modulated `MultiDelay`
pairs) and `modules/flanger_module.{h,cpp}` (a clamped, unfiltered
`StereoDelay` swept by a triangle LFO around a center note). Both are driven
through the effect-module harness from M6 PR 1.

Upstream quirk kept and covered: `ProcessorRouter::setSampleRate` skips idle
processors, and neither module forwards it, so the chorus and flanger delays
run at the default 44.1 kHz times the oversampling amount at any host rate.
The 48 and 96 kHz cases capture this.

Captures: `effects/chorus_module` (16 cases: voices 1, 2, 3 and a 1→4 sweep,
free frequency −6→3, tempo 0→10, dotted and triplet, feedback −0.95→0.95,
depth 0→1, both delay times over −10→−5.64, cutoff, spread and wet sweeps,
enable toggling, 48/96 kHz, 2× oversampling, a 5 s tail) and
`effects/flanger_module` (13 cases: the same axes plus center 8→136, phase
offset 0→1, wet 0→0.5, and a 5 s feedback-1.0 tail). All under
`module_defaults`.

**The port is bit-identical to the `REF_FAST_MATH=OFF` build on all 29
outputs.** Against the fast-math reference, 24 miss the module gates, and
the proxy misses them by the same amounts. The modulated delay time drifts:
a last-bit difference in the LFO phase or delay-time arithmetic grows over
the case, and moving a delay read shifts high-frequency content (the
render-calibration "phase drift" class). Worst proxy noise: chorus
`filter_and_wet_sweep` 1.0e-1 / −52.5 dB, chorus `voices_3` 7.1e-2 / −51.0 dB,
flanger `dotted_and_triplet` 2.7e-1 / −43.8 dB. Mildest failures: flanger
`defaults_saw` 1.2e-4 / −108.8 dB and chorus `rate_48k` 1.1e-3 / −90.8 dB.

Under the coordinator's pending proposal, awaiting the owner (exact comparison
against no-fast-math only where the official reference fails against its own
no-fast-math build), the 24 are parked with their inputs, unchanged, in
`migration/corpus/modules/effects/pending/{chorus_module,flanger_module}.json`:
all 16 chorus cases and 8 flanger cases.

**Interim coverage** (cases named `interim_*`, reviewer request): settings
where the LFO phase drift cannot move a delay read by more than a last bit,
namely zero modulation depth (constant delay times), or a small depth (0.05)
over short renders (0.1 s). Chorus: 11 cases (voices 1–4, both delay times
over −10→−5.64, feedback −0.95→0.95 with cutoff, spread and wet sweeps,
enable toggling, 48 kHz, 2× oversampling, small depth free and tempo-synced).
Flanger: 7 cases (center at 8 and at 136, feedback −1→1, phase offset 0→1,
enable toggling, a feedback-1.0 tail, small depth free and dotted/triplet). A
swept center at zero depth still fails the proxy (the sweep itself moves the
read), so the center is covered at its two extremes. Every interim case passes
the reference and the proxy. Worst: chorus `interim_small_depth_short`
3.4e-5 / −106.4 dB; flanger `interim_full_feedback_short_tail` −142.1 dB.

Loaded: 11 chorus and 12 flanger cases. The 5 full-range flanger cases
(`tempo_sweep`, `wet_sweep`, `rate_48k`, `rate_96k`, `oversampled_2x`) pass
both the reference and the proxy, worst 9.5e-5 / −104.9 dB. Alternatives, as
for the reverb: per-case exceptions at proxy noise plus 6 dB, or render-style
gates.

**macOS arm64 (after #2):** 7 flanger cases miss the macOS no-fast-math
proxy although they pass on x86: `tempo_sweep`, `wet_sweep`,
`oversampled_2x`, `interim_no_depth_enable_toggle`,
`interim_no_depth_phase_offset`, `interim_small_depth_free_frequency` and
`interim_full_feedback_short_tail`. Max abs is at most 1.6e-4, with
residuals from −74.6 to −104.8 dB. Apple clang's fast-math reassociation in the
feedback delay is the noise source. Per `migration/AGENTS.md` they are parked
with their inputs unchanged in
`migration/corpus/modules/effects/pending/flanger_module_arm64.json`, their
x86 hash entries are removed, and no tolerance changes. They return under
SAS-35 (`reference: nofastmath`). The flanger keeps 6 loaded cases
(`rate_48k`, `rate_96k`, `interim_no_depth_center_low`,
`interim_no_depth_center_high`, `interim_no_depth_feedback_sweep`,
`interim_small_depth_dotted_triplet`). All 11 chorus cases pass the macOS
proxy (Reference ARM run 39622362623), and the macOS arm64 hashes come from
that run's `ref-arm64-macos-15` artifact.
