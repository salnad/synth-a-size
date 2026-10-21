## 2026-10-21: owner approval — plugin host text entry (#59)

The owner (Salil) decided this on 2026-10-21. The coordinator relayed the
decision to the Plugin Wrapper thread, which recorded it here.

- **Keep the port's host text entry: approved.** When a value is typed into a
  host parameter, it is read as the displayed value. The port removes
  `post_offset`, divides by `display_multiply` and unskews it, which inverts
  `ValueBridge::getText`. The result is clamped to the parameter's range, and
  a stepped control's value is rounded to a step. Typing an option name
  (case-insensitive) selects that option.
- **Upstream's out-of-range typing bug is not replicated.**
  `ValueBridge::getValueForText` returns an engine-domain value, and JUCE's
  VST3 wrapper passes it to the host as the normalized value
  (`vital/third_party/JUCE/modules/juce_audio_plugin_client/VST3/juce_VST3_Wrapper.cpp:529`).
  `convertToEngineValue` never clamps it. So upstream sets the engine cutoff
  to 72 × 128 + 8 = 9224 when "72" is typed into Filter 1 Cutoff, and
  typing an option name selects option 0. nice-plug clamps text input to
  [0, 1] in any case.

This affects only display and text entry in the host's parameter UI. Engine
values, automation and preset state are unaffected, so there is no parity
impact.

Implemented in `port/crates/plugin/src/params.rs` (`value_from_text`).
Tests: `text_input_inverts_the_display` and
`host_text_shows_options_and_display_units` (`port/crates/plugin`).
