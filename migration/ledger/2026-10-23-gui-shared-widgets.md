## 2026-10-23: GUI shared widgets, power button, TextSelector, TabSelector

**Changed.** Widgets that both the Voice tab (G3b) and the Effects and
Advanced tabs (G6) need, landed ahead of both so the two streams don't
write them twice.

- `SynthButton` (was `TextButton`) gains `OpenGlButtonComponent`'s power
  button style: the dot at 0.45 of the button in the power on/off colour,
  then a halo at 0.65 in the pressed colour while down or the hover colour
  as the hover fades in. Same toggle-on-release gesture as the text button.
- `Slider::selector` is a `TextSelector`. A press opens the option list
  below it (the long names when the section gives them, else the
  parameter's options, up to its maximum), and choosing one sets the value
  in one gesture. A right-click still opens the slider menu. The menu's
  selection handling moved from `Interface` to `Overlay::show_menu`, so any
  panel can drive it.
- `controls::tab_selector::TabSelector`: `TabSelector`'s paint, `getTabX`
  and its select-on-press/drag/release behaviour, for any list of names.
  The header's tabs use it; the EQ and reverb band selectors will.
- Section helpers in `section.rs`: `title_width`, `power_button_bounds`,
  `title_bounds`, `paint_container` (the body with the heading background
  clipped to the title strip), `paint_heading_text`, and `off_overlay`
  (the 80% background veil over a section that is switched off).
- `Colour::desaturated` (`withSaturation(0)`), for an inactive selector.

**Verified.**

- Widget tests on a standalone panel: the power button toggles in one
  gesture; a text selector's press lists its options below it without
  editing, and a choice sets the value in one gesture; a right-click opens
  the slider menu; a tab selector follows the pointer while down, clamps
  past its end and makes no host edits.
- The header's tab positions still equal upstream's (`tab_x` unit test),
  and the existing interaction tests for tab switching pass.
- `cargo clippy --workspace --all-targets -D warnings`, `cargo fmt`, and
  the `sas-gui` tests pass.

**Next.** G3b (oscillators, sample, filters) builds on these, and so does
G6 (Effects and Advanced).
