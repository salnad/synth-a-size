# No-fast-math exact cases: effects/distortion_module

DistortionModule. A swept DigitalSvf before the linear fold: the fold amplifies the filter's fast-math rounding past the gate (migration/ledger/2026-10-21-m6-eq-distortion-module.md). The port is bit-identical to the no-fast-math build on this case.

- `effects/distortion_module/filter_before` (SAS-35; x86): a swept filter into the linear fold amplifies the filter's fast-math rounding
