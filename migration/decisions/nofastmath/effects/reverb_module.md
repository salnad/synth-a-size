# No-fast-math exact cases: effects/reverb_module

ReverbModule. As effects/reverb: the feedback delay network amplifies last-bit fast-math rounding over long tails (migration/ledger/2026-10-21-m6-reverb.md). The port is bit-identical to the no-fast-math build on these cases.

- `effects/reverb_module/decay_and_size_sweep` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb_module/filter_and_delay_sweep` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
- `effects/reverb_module/long_tail_max_decay` (SAS-35; x86): the feedback delay network amplifies last-bit fast-math rounding over the tail
