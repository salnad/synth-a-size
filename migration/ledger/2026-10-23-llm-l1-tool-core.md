## 2026-10-23: LLM L1, tool core (`sas-llm-control`)

**Why.** The owner (Salil) approved starting the LLM layer in parallel with
the GUI, on the working defaults in `docs/project-context.md` ("GUI and LLM
build: working defaults", 2026-10-22). L1 is the first milestone of
`docs/llm-plan.md` (project store): the tool layer, with no plugin, network
or engine dependency. Nothing the plugin does changes in this PR.

**Changed**

- `port/crates/llm-control` (`sas-llm-control`), from stub to tool core:
  - `catalog`: the 772 controls (`engine::is_control`), each family of
    numbered controls written once as a template (`osc_N_level (N=1..3)`)
    for the system prompt; ranges, defaults and values in display units;
    word search.
  - `session`: `SynthSession` (snapshot, read a control, apply an `Edit`,
    restore) and `OfflineSession` over a `PresetState`. `Edit` is
    `#[non_exhaustive]`; L2 adds modulation, LFO and wavetable edits, and
    L4 maps edits onto `GuiContext` gestures and the GUI's edit bus (G0).
  - `tools`: JSON schemas and handlers for `get_patch`, `find_params`,
    `get_params`, `set_params`, `checkpoint` and `undo`. `set_params`
    takes display units (a number, a string with units, or an option name),
    clamps to the range, rounds stepped controls, applies the batch whole or
    not at all, and suggests names for unknown ones.
  - `controller`: one undo step per model turn (`begin_turn`), labelled
    checkpoints, a bounded history (`Policy::max_undo`, 50), and edits held
    for the user's confirmation (`Policy::confirm_above`).
  - `patch`: the compact patch description (modules on with their edited
    controls, modules off, modulation routes, edited LFO shapes and
    wavetables).
- `port/crates/preset/src/display.rs` (`sas_preset::display`): upstream's
  `ValueBridge` conversions (`skew`, `unskew`, `display_text`,
  `value_from_text`), moved from `port/crates/plugin/src/params.rs` in a
  semantics-preserving move, so the plugin, the LLM tools and the GUI share one copy. The plugin now
  imports them. Its two tests (`display_text_matches_the_value_bridge`,
  `text_input_inverts_the_display`) moved with the code, unchanged; none was
  deleted or narrowed. Host parameter behaviour is unchanged.
- `migration/DIVERGENCES.md`: D13, the LLM layer as an addition with no
  upstream counterpart, and a tombstone for D12 (temporary, removed in
  #82; frozen ledger entries still cite it).
- `migration/IMPROVEMENTS.md`: I14 (MCP bridge inside the plugin), I15
  (preview audio sent to models), I16 (note preview through the live
  engine), parked per the plan and the working defaults.

**Measured**

- Catalog for the system prompt: 227 rows, under 12 KB (about 2.2k
  tokens), against about 8.7k tokens for the flat list of 772 controls
  (pinned by `catalog_lists_each_family_once`).
- `get_patch` on the init patch: under 1.2 KB (pinned by
  `the_init_patch_is_short`).
- Tests: `cargo test -p sas-llm-control` (10 unit, 11 integration) and the
  two moved display tests in `sas-preset`. Clippy `-D warnings` and rustfmt
  are clean.

**Decisions**

- Working defaults, each a field or constant that is easy to change:
  - raising master `volume` above 0 dB (its range is -80..+6 dB) waits for
    the user (`Policy::confirm_above`); lowering never asks;
  - edits apply at once with undo.
- Numbers in `set_params` are display numbers, the same reading as host text
  entry (D1), so `osc_1_level` 0.5 is engine value √0.5 on its quadratic
  scale.

**Review (#91).** Chain Reviewer approved at 78566eb, with nits applied: the
move is described as semantics-preserving, and I14–I16 state their rationale
inline instead of citing the project store's plan. Parity Reviewer asked for
the entry to be D13, since D12 was the temporary entry added in #79 and
removed in #82 that frozen ledger entries cite; D12 now has a tombstone.
D13 also says the off-by-default setting arrives with the plugin wiring
(L4/L5).

**Blocked:** nothing. L2 (structural tools and the golden test) and L3
(providers and replay tests) follow; L4 waits for G0, L5 for G1.
