## 2026-10-23: LLM L2, structural tools and the golden render case

**Why.** L2 of `docs/llm-plan.md` (project store): the tools that change
more than a knob (modulation routes, LFO shapes, wavetables, presets), and
the PLAN.md M9 exit check that a patch made with the tools renders the same
in the reference and the port. Stacked on #91 (L1). Nothing the plugin does
changes: it does not use `sas-llm-control` yet.

**Changed**

- `sas-llm-control`:
  - `Edit` gains `ConnectModulation` (with the slot's own settings),
    `DisconnectModulation`, `SetLfoShape`, `SetWavetable` and `LoadPreset`.
    `SynthSession::apply` returns `Applied`, which carries a created or
    reused slot. A batch whose later edit fails restores the session, so a
    batch applies whole or not at all.
  - The offline connect follows upstream's GUI
    (`SynthGuiInterface::connectModulation`, `vital/src/common/synth_gui_interface.cpp:97-121`):
    `PresetState::add_modulation` (slot, bipolar default), then for a
    created slot a linear remap curve and `modulation_N_power` = 0. G0 owns
    the shared form, `PresetState::connect_modulation_from_gui`; the offline
    session switches to it when G0's first PR lands. The rule is the same,
    so the golden preset does not change.
  - New tools: `connect_modulation`, `update_modulation`,
    `disconnect_modulation`, `set_lfo_shape` (named shape, or 2–100 points
    with y = 0 bottom .. 1 top, stored inverted as upstream does),
    `set_wavetable` (1–64 keyframes at 0..256, each a basic shape or a
    harmonic spectrum, in one Wave Source with spectral morphing like the
    init wavetable), `list_presets`, `load_preset`, `save_preset` and
    `init_patch`. `find_params` marks modulatable controls.
  - `library::PresetLibrary`: the preset folder, the only files the tools
    read or write. Names cannot contain path separators or start with a dot.
    Saving over an existing preset needs `overwrite`.
  - Loading a preset that would raise master volume past the threshold waits
    for confirmation, like `set_params`.
- Golden render case (M9 exit):
  - `migration/corpus/llm/sessions/glass_pad.json`: an authored tool
    session on `presets/init_saw.vital`.
  - `migration/corpus/llm/presets/glass_pad.vital`: what the session
    produces. It is written by the tools, checked by
    `tests/golden.rs` (`SAS_LLM_GOLDEN_WRITE=1` rewrites it).
  - `cases.json`: render case `llm_glass_pad__chords`
    (`midi/chord_progression.mid`), added to `sas-engine`'s
    `tests/render.rs`.
  - `modules/preset/state.json`: preset-state case `llm_glass_pad` (load →
    save, and round trip, under the JSON bound).
  - `sas-preset` `tests/reference.rs`: `every_preset_has_a_state_case` now
    also covers `llm/presets` (a tightening).
  - `migration/corpus/README.md`: the `llm/` section, the one corpus folder
    not written by `generate.py`.
- `migration/DIVERGENCES.md` D12 extended for the connect rule, generated
  wavetables and the preset folder.

**Measured** (local x86 build with the CI identity: Ubuntu 24.04, GCC
13.3, glibc 2.39; the local tree reproduces every committed hash, and only
the two new entries were written)

- `render/llm_glass_pad__chords`, port vs official reference: rms_diff
  0.00017 dB (limit 0.01), residual −63.66 dB (limit −50), lsd 0.033 dB
  (limit 0.1). The port's output (`sas-render --seed 0`) is bit-identical to
  the `REF_FAST_MATH=OFF` reference, so these are the official build's own
  fast-math differences. The official build passes against its no-fast-math
  build with the same numbers (the CI calibration step).
- `preset/state/llm_glass_pad`: `state.json` and `roundtrip.json` pass under
  the JSON bound.
- `cargo test -p sas-llm-control`: 15 unit, 8 structural, 11 tool and 2
  golden tests. `sas-engine` `tests/render.rs` (3 tests, 17 cases) and
  `sas-preset` `tests/reference.rs` pass against the local captures.

**Finding: stereo modulation of a filter cutoff amplifies the reference's
fast-math noise.** The first draft of the session also set `stereo` on the
LFO 1 → filter 1 cutoff route. There, the official reference against its
own no-fast-math build gave residual −49.64 dB on the right channel (limit
−50), and the port matched the no-fast-math build bit for bit. Variants of
that preset rendered with both reference builds (residual L / R, dB):

| Variant | L | R |
|---|--:|--:|
| as drafted | −64.33 | −49.64 |
| stereo off | −64.33 | −64.91 |
| chorus off | −65.00 | −50.40 |
| reverb off | −64.33 | −49.64 |
| unison 1 voice | −65.21 | −48.33 |
| filter 1 off | −46.27 | −45.78 |

So the reference itself cannot meet the render floor for that patch, and
no port change could fix it. Render cases have no no-fast-math route: SAS-35
covers module cases only. The committed session leaves `stereo` off (it
changes the route's amount instead), and no tolerance was touched. Gating
the stereo variant would need an owner decision: extend SAS-35 to render
cases, or approve a tolerance exception for it. The filter-off row shows the
same amplification in the unfiltered unison wavetable path, so the risk
applies to other generated patches as well.

**Decisions**

- Golden sessions start from a corpus preset rather than the bare init
  patch, so the committed preset carries `init_saw`'s 2 KB sample instead of
  the init patch's 118 KB of white noise (84 KB file, mostly wavetable data).
- `init_patch` builds the init state with sample seed 0 in the offline
  session. The live session (L4) goes through `GuiContext::set_state`, as G0
  requires for whole-preset loads.

**Blocked:** nothing.
