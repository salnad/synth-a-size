# No-fast-math exact cases: effects/flanger_module

FlangerModule. A last-bit drift in the LFO phase moves the modulated delay read, and the feedback delay amplifies fast-math rounding; the 7 macos-only cases miss only on macOS arm64 (migration/ledger/2026-10-21-m6-chorus-flanger.md). The port is bit-identical to the no-fast-math build on these cases.

- `effects/flanger_module/defaults_saw` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/free_frequency_sweep` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/dotted_and_triplet` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/center_sweep` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/feedback_sweep` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/depth_and_phase_offset_sweep` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/enable_toggle` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/long_tail_full_feedback` (SAS-35; x86): LFO phase drift from fast-math rounding moves the modulated delay read
- `effects/flanger_module/tempo_sweep` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
- `effects/flanger_module/wet_sweep` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
- `effects/flanger_module/oversampled_2x` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
- `effects/flanger_module/interim_no_depth_enable_toggle` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
- `effects/flanger_module/interim_no_depth_phase_offset` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
- `effects/flanger_module/interim_small_depth_free_frequency` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
- `effects/flanger_module/interim_full_feedback_short_tail` (SAS-35; macos): the feedback delay amplifies Apple clang's fast-math rounding (passes on x86)
