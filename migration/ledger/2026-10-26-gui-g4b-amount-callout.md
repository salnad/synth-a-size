## 2026-10-26: GUI G4b-2b, the expand button and the amount knobs' callout

**Changed.** Stacked on G4b-2a (#153). A source with more connections than
its button holds now shows them the way upstream's `ModulationManager`
does: `positionModulationAmountSlidersCallout`,
`showModulationAmountCallout`, `ExpandModulationButton` and
`ModulationExpansionBox`.

- **When.** The button holds three knobs across, in as many rows as its
  modulation area is tall (`kMaxModulationsAcross`). With more
  connections than that, the area is an expand button instead.
- **The expand button** (`ExpandModulationButton`, a lighten button
  triggered on press):
  - Its background is transparent white, mixing towards `kLightenScreen`
    as its hover fades in by 0.2 a frame, and `kOverlayScreen` while
    pressed.
  - It draws a ring per connection (`renderSliderQuads`) in the knob's
    colour, with a half-alpha centre and thickness 2. The rings sit in
    `getNumColumns` columns of square cells, from the bottom row, centred
    vertically.
- **The callout** (`showModulationAmountCallout`):
  - A press on the expand button opens its source's knobs below the
    source button, in a `kBody` box with `kBodyRounding` corners: 30 px
    knobs with 5 px of padding, in the same number of columns, filled from
    the bottom row up.
  - The box is centred under the button and kept inside the editor.
  - A press on the expand button again closes it (`buttonClicked`), and so
    does a press anywhere outside the box (`expansionFocusLost`). The
    knobs inside are amount knobs like the others (`KnobKind::Callout`),
    with the same drag, popup and menu.
  - When the source no longer needs the callout, it closes.
- **New GUI preset and view `modulations_many`**, rendered animated: LFO 1
  on seven controls.

**Verified.**

- **Pixels against ref-gui with animation on** (#129's
  `--seconds 0.05 --animate on`, rendered locally on its branch): LFO 1's
  expand button matches upstream. The ring grid has four columns, two
  rows filled from the bottom, and each ring's colour: the oscillator
  controls' and macro 2's purple, reverb's blue, and the envelope's
  green.
  - With five connections, upstream holds them all in the button, and so
    does the port, which is why the preset has seven.
- The open callout can't be rendered by ref-gui, which can't click. A
  render of the port shows the box and knobs below LFO 1.
- **Tests:**
  - `a_source_with_more_knobs_than_fit_opens_them_in_a_callout`: no knobs
    in the button. The press opens seven 30 px knobs, bottom row first. A
    second press closes them, a third opens them, and a press outside
    closes them.
  - `a_callout_knob_sets_its_slot`: one gesture on `modulation_2_amount`,
    and the callout stays open.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, the
  `sas-gui` tests, `gui_reference.py layout --check` and `generate.py
  --check` pass.
