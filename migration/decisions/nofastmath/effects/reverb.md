# No-fast-math exact cases: effects/reverb

Reverb. The 16-line feedback delay network with chorus-modulated reads, allpass diffusion and shelving filters feeds last-bit fast-math rounding back through long decays until it exceeds the module gates (migration/ledger/2026-10-21-m6-reverb.md). The port is bit-identical to the no-fast-math build on these cases.

- `effects/reverb/chorus_sweep_sine` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/decay_sweep_saw` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/filter_sweeps` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/impulse_long_tail` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/max_decay_full_size_tail` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/pre_delay_sweep` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/rate_48k` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/rate_96k` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/rate_192k` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb/size_sweep_noise` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
