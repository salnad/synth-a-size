# No-fast-math exact cases: effects/reorderable_effect_chain

ReorderableEffectChain. With the chorus or flanger on at their default modulation depth, a last-bit drift in their LFO phase moves the modulated delay reads, and every later effect in the chain carries the difference (as for effects/chorus_module and effects/flanger_module). The port is bit-identical to the no-fast-math build on these cases.

- `effects/reorderable_effect_chain/all_on_default_order` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/all_on_reversed_order` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/all_on_custom_order` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/order_sweep_all_on` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/order_toggle_all_on` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/enable_toggles` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/only_chorus` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/only_flanger` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/all_on_oversampled_2x` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
- `effects/reorderable_effect_chain/all_on_96k` (SAS-35; x86): chorus/flanger LFO phase drift from fast-math rounding moves the modulated delay reads
