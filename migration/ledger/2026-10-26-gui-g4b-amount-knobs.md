## 2026-10-26: GUI G4b-2a, the modulations' amount knobs

**Changed.** This ports `ModulationAmountKnob` and the parts of upstream's
`ModulationManager` that show it (`modulation_manager.cpp`), stacked on
G4b-1 (#150).

- **The knob.** `SliderStyle::ModulationKnob { arc, bypass }` draws
  `OpenGlSlider`'s modulation knob.
  - The quad is `kModulationKnobFragment`, half a pixel in from the edge.
    The sweep is from pi by the amount, with the colours swapped below 0.
  - Thickness is 1.8 while hovered, otherwise 1.
  - Unselected is the widget background. Selected is the arc colour mixed
    halfway with the background. The thumb is the arc colour. A bypassed
    modulation is desaturated (`withBypassSaturation`).
  - It is a slider on its slot's `modulation_N_amount`, so dragging it is
    one host gesture. It shows its popup on hover, a double click does
    nothing (`setDoubleClickReturnValue(false, ...)`), and its text entry
    is twice as wide.
  - A linear or indexed destination's amount reads in that destination's
    units (`setModulationSliderScale`).
- **Where knobs show** (`ModulationManager::amount_knobs`):
  - **In each source button drawn** (`positionModulationAmountSlidersInside`,
    with `getModulationAreaBounds` and `getModulationAmountBounds`, rows of
    three from the bottom).
    - The knob takes its destination's rotary arc colour
      (`setDestinationComponent`). That is cached from when the
      destination last drew, or taken from its section's skin if it hasn't
      drawn yet.
    - The popup prefix is the destination's name. The popup goes right
      near the left edge and left near the right edge, as upstream's 150 px
      rule does.
    - A source with more connections than fit uses the expand button and
      callout. That is G4b-2b.
  - **Beside the selected source's destinations**
    (`makeCurrentModulatorAmountsVisible`): 24 px at size ratio 1, on the
    side each control asks for (`setModulationPlacement`). Seven sliders
    set it, as upstream's do: the LFO phase, the oscillator distortion
    phase and wave frame, and the filter cutoff, formant X and Y, and
    resonance. These knobs are in the modulation meter control colour
    (`setCurrentModulator`).
  - **Beside the control the pointer last went over**
    (`makeModulationsVisible`): that control's other sources' knobs,
    centred in a row or column, leaving a gap where the selected source's
    knob is. They stay until another control is hovered or a source is
    pressed.
- **The knob's menu** (`ModulationAmountKnob::mouseDown`): Remove,
  Bypass/Unbypass, Make Bipolar/Unipolar, Make Stereo/Mono, and Enter
  Value. A middle click toggles bypass (`toggleBypass`).
  - Remove disconnects through the edit bus with no host write, as
    upstream's does.
  - The toggles set the slot's host parameter in one gesture. Upstream
    sets only the synth's value (`setModulationValues` → `valueChanged`);
    the port's slot values are host parameters (D18).
- **Supporting changes.**
  - `Slider::instance` keeps the same slot's knobs apart (each kind has its
    own id).
  - `Slider::double_click_returns` disables the double-click reset.
  - `menu_source_display_name` (`getMenuSourceDisplayName`).
  - `Panel::amount_knob` and its menu, and `slider_input`'s menu hook.
- **The `modulations` view draws upstream animated.** Upstream's
  modulation overlay draws only while animating
  (`ModulationManager::renderOpenGlComponents` returns early otherwise).
  The view uses Harness Builder's ref-gui options from #129: `"animate":
  true` with `"seconds": 0.05`, a short silent render that its guard
  requires. The knobs don't depend on the engine.
- **New GUI preset `modulations`** (`generate.py`): seven connections on
  oscillator 1 from LFO 1 (three), envelope 2, velocity, macro 1 (bypassed)
  and random 1. Its view renders with animation on.

**Verified.**

- **Pixels against ref-gui with animation on** (#129's `--animate on`, rendered locally) (image in the PR): the knobs in
  LFO 1's, envelope 2's, macro 1's, random 1's and velocity's buttons match
  upstream's in place, sweep and colour, including the bypassed grey and
  the negative amount. Upstream's source meters and the oscillator level's
  modulation meter also show in that frame; they are G4b-3.
- **Tests:**
  - `a_sources_amount_knob_sets_its_slot_in_one_gesture`: the prefix, and
    one gesture on `modulation_1_amount`.
  - `a_selected_sources_knobs_sit_beside_their_destinations`: below macro
    1, 24 px, prefix "Lfo 1: ".
  - `going_over_a_control_shows_its_other_sources_knobs`: envelope 1's knob
    with the two-knob gap, kept after the pointer leaves.
  - `an_amount_knobs_menu_toggles_and_removes`: the items; bypass in one
    gesture; Remove with no host write.
  - `a_middle_click_on_an_amount_knob_toggles_bypass`.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, the
  `sas-gui` tests, `gui_reference.py layout --check` and `generate.py --check`
  pass.

**Not yet.**
- G4b-2b: the expand button and callout.
- G4b-3: the meters, the editing ring (`showModulationAmountOverlay`) and
  the aux modulations (a drag onto an amount knob).
- The knob's MIDI learn items come with the port's slider MIDI learn menu.
