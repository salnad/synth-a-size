# No-fast-math exact cases: engine/tuning

The official x86 reference rounds two large or cancelling tuning values
differently from its own no-fast-math build, by more than the module limits,
and the port matches the no-fast-math build. The active cases `tun_then_kbm`
and `scl_reference_frequency` stay on the official reference with
`skip_raw` and the normalized outputs; these copies capture every raw output.

- `engine/tuning/tun_then_kbm_raw` (SAS-35; x86): a 127-semitone `.tun` period repeated by a kbm reaches ±697 semitones, where the reference's fast-math rounding is 2 ULP (1.22e-4)
- `engine/tuning/scl_reference_frequency_raw` (SAS-35; x86): `setReferenceNoteFrequency(69, 440 Hz)` cancels to 0 in the source formula; fast-math reassociation leaves −7.63e-6
