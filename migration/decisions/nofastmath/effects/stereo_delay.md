# No-fast-math exact cases: effects/stereo_delay

Delay<StereoMemory>. Long feedback (0.95, -0.9) with damping filters accumulates Apple clang's fast-math rounding past the gate on macOS arm64 (LEDGER.md, the Apple Silicon reference entry for #2). They pass on x86. The port is bit-identical to the no-fast-math build on these cases.

- `effects/stereo_delay/clamped_dampened_damping_sweep` (SAS-35; macos): high feedback through the damping filters accumulates fast-math rounding
- `effects/stereo_delay/unclamped_unfiltered_negative_feedback` (SAS-35; macos): high feedback through the damping filters accumulates fast-math rounding
