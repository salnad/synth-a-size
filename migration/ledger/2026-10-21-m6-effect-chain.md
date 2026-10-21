## 2026-10-21: M6 effect chain, ReorderableEffectChain and FilterFxModule (`sas-effects`)

**Changed.** Ported `modules/reorderable_effect_chain.{h,cpp}`, including the `FilterFxModule` class defined in that `.cpp`, as `ReorderableEffectChain` and `FilterFxModule` in `sas-effects`.

- **Chain:** the nine effect modules, each switched by its `<effect>_on` control and run in the order decoded from the `order` input. As upstream, the order is decoded only when the rounded code changes; an effect is `enable()`d or disabled when its switch differs from its state; disabled effects are skipped; and each effect's output feeds the next.
- **Accessors:** `getEffect`, `getEqualizerMemory`, `hardReset` and `correctToTime` are ported.
- **`FilterFxModule`:** a mono `FilterModule` ("filter_fx") from #58, without its own `filter_fx_on` control (the chain owns it), with keytrack from the chain.

No intentional divergence, so `migration/DIVERGENCES.md` is unchanged. The test parameter lookup now falls back to `sas-preset`'s registry for names outside its effects table (the `_on` switches and `filter_fx_*`), via a new `sas-preset` dev-dependency (one line in `port/Cargo.lock`).

**Captures:** `effects/reorderable_effect_chain` (new `migration/reference/src/modules/effects/reorderable_effect_chain.cpp`). It drives the chain like SoundEngine: order, keytrack, tempo and controls set per block. There are 25 cases:

- all nine effects on in the default, reversed and a custom order;
- an order sweep through all 362880 codes, and an order toggle;
- every effect's switch toggled at its own rate;
- each effect alone;
- `FilterFxModule` model and cutoff, style, drive and blend, keytrack, formant and mix/blend-transpose sweeps, plus its enable toggle;
- all on at 2× oversampling and at 96 kHz;
- interim cases with the other seven effects on (chorus and flanger off) for order toggling and enable toggles on the official reference.

**Parity (x86):**

- The 15 official-reference cases pass. The worst is `interim_seven_on_order_toggle` at 4.5e-5 / −87.4 dB, and `only_eq` is bit-identical.
- The 10 cases that run the chorus or flanger at default depth miss the gates against the fast-math reference, through the same LFO-drift mechanism as `effects/chorus_module` and `effects/flanger_module`. They are declared `"reference": "nofastmath"` (SAS-35, `x86`) in `migration/decisions/nofastmath/effects/reorderable_effect_chain.md`, and all print `justified on x86`.
- The port is **bit-identical to the no-fast-math build on all 10**, including the full order sweep, the toggles and the 2×/96 kHz cases.
- Hashes: official 25 and no-fast-math 10 entries, from local CI-identity trees (`hashes --check` 2740 and 359, both 0 problems). `allowlist-approval` reports 0 problems.

**Mutants:**

- Ignoring the order input fails `all_on_custom_order`.
- Never calling `enable()` fails `enable_toggles`.
- `FilterFxModule` with `setMono(false)` survives. That one is equivalent here: the chain's controls carry no per-voice modulation, so the poly and mono control paths give identical values.

**macOS:** Reference ARM is non-gating by owner policy. The macOS hashes, and `macos` targets for any case the macOS reference also fails, come from this PR's macOS artifact if it arrives.

Official vs no-fast-math, per case (max abs / residual):

| Case | Reference | Official vs no-fast-math |
|---|---|---|
| `all_on_default_order` | nofastmath | 1.7e−05 / −78.5 dB |
| `all_on_reversed_order` | nofastmath | 2.5e−04 / −83.3 dB |
| `all_on_custom_order` | nofastmath | 2.2e−03 / −79.1 dB |
| `order_sweep_all_on` | nofastmath | 3.5e−02 / −65.3 dB |
| `order_toggle_all_on` | nofastmath | 2.1e−02 / −71.8 dB |
| `enable_toggles` | nofastmath | 3.9e−02 / −53.3 dB |
| `only_chorus` | nofastmath | 1.4e−03 / −89.3 dB |
| `only_compressor` | official | 3.4e−05 / −90.0 dB |
| `only_delay` | official | 3.0e−08 / −152.7 dB |
| `only_distortion` | official | 1.2e−07 / −145.5 dB |
| `only_eq` | official | 0 / −inf dB |
| `only_filter_fx` | official | 1.2e−07 / −133.5 dB |
| `only_flanger` | nofastmath | 1.2e−04 / −104.1 dB |
| `only_phaser` | official | 1.3e−06 / −117.7 dB |
| `only_reverb` | official | 1.4e−05 / −104.3 dB |
| `filter_fx_model_and_cutoff_sweep` | official | 2.4e−06 / −119.4 dB |
| `filter_fx_style_sweep` | official | 4.8e−07 / −136.5 dB |
| `filter_fx_keytrack` | official | 1.3e−06 / −124.1 dB |
| `filter_fx_formant` | official | 2.1e−06 / −119.3 dB |
| `filter_fx_mix_and_blend_transpose` | official | 6.0e−08 / −170.3 dB |
| `filter_fx_enable_toggle` | official | 8.9e−07 / −130.2 dB |
| `all_on_oversampled_2x` | nofastmath | 6.2e−05 / −68.6 dB |
| `all_on_96k` | nofastmath | 1.3e−04 / −66.6 dB |
| `interim_seven_on_order_toggle` | official | 4.5e−05 / −87.4 dB |
| `interim_seven_enable_toggles` | official | 2.3e−05 / −92.4 dB |
