## 2026-10-27: GUI G4c-3b (2), the LFO and remap editors' animated line

**Changed.** The line editors now animate from the synth's status
outputs, as `LfoEditor::render` and `LineMapEditor::render` do. This is
stacked on G4c-3b (1).

- **`LineTarget::animation`:** the source a target animates from.
  - An LFO reads `lfo_n_phase` and `lfo_n_frequency`, and animates while it
    modulates something (`isModSourceEnabled`).
  - A remap curve reads `modulation_source_n`, and animates while its row
    is connected (`setAnimate(rows_[selected_index_]->isActive())`).
  - A target without a source draws still.
- **Each frame, as upstream does (`LineEditor::animate`):**
  - **The remap:** the boosts decay by `kTailDecay` (0.93). While the input
    isn't clear, the stretch from the last adjusted input to the new one is
    boosted (`adjustBoostPhase`, `boostRange`).
  - **The LFO:** the phase and voice are decoded (`decodePhaseAndVoice`).
    The boosts decay by 1 less 5 times the phase's move, at least to 0.9,
    or by 0.9 when the phase is clear. After a voice switch, the boost
    starts a fiftieth of a second of the LFO's travel back. It boosts the
    stretch while animating, and clears the boosts otherwise.
  - The boosts, last phase and last voice carry from frame to frame in the
    editor's state (`primitives::line::Boosts`). The wrap points take the
    boosts of the points they repeat (`resetWavePath`).
- **Drawing:**
  - The fills take the boosts, scaled by `WidgetLineBoost` and
    `WidgetFillBoost`. The right channel's line uses the right boosts. The
    left uses the left boosts always for a remap curve, and only when any
    boost is set for an LFO (`drawLines(anyBoostValue())`). The centre line
    takes no boost.
  - Position rings (`drawPosition`) mark the input or phase on the line,
    right then left. Each is a 9-pixel ring with its centre faded 0.2
    towards the background, at `valueAtPhase`. There is none at phase 0,
    for a clear input, or (for an LFO) in a frame whose voice switched.
  - The order is upstream's.
    - LFO: grid, fills, centre line, positions, points and corners.
    - Remap: grid, fills, the grid again, centre line, points and corners,
      positions, then the points and corners again
      (`LineEditor::render(open_gl, false)` inside `LineMapEditor::render`).
- **Decay rate:** boosts decay per frame, as upstream's do at its
  display's rate.
- **Gallery:** views with a running engine now step 30 frames before the
  image, as ref-gui draws 30 (`--frames`), so decays have run as long in
  both. A new view, `modulations_live`, plays the chord progression on the
  Voice tab with the animated views drawn.

**Verified against ref-gui (1400 × 820).**

- **`modulations_live`:** LFO 1's animated editor (510 × 170) has 8 pixels
  more than 24 off. The boost trail, the fills and the phase ring line up.
- **`modulations_matrix_live`:** in the remap section, the 663 pixels more
  than 24 off are the header text, as in the still view. The left half of
  the fill, boosted from 0 to the input on the first animated frame, reads
  (34, 92, 94) against upstream's (34, 94, 97). The still frames read
  (34, 87, 89).
- **Other views:** with 30 frames, `all_effects_live` moves from 25,023 to
  25,211 pixels more than 24 off. That is the effects views' own animation
  timing, which this doesn't touch. The still views are unchanged.
- **Unit tests:**
  - `boost_phases_count_the_shapes_points` (`adjustBoostPhase`);
  - `boosts_carry_from_frame_to_frame`: a renderer of another size starts
    with none.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
