## 2026-10-27: GUI: the save and delete overlays take drags as well as clicks

A fix found while building G7d-1's popup browser. The save section (G7c)
and the delete section (G7b-3) covered the editor with a widget that sensed
clicks only. egui gives a press to the top widget that senses clicks and,
separately, to the top one that senses drags, so a drag outside either box
still reached the knob or slider beneath it. With the save section open, a
drag on oscillator 1's distortion knob moved it. Upstream's overlays cover
the editor and take every mouse event.

- Both now sense clicks and drags, so nothing beneath gets the press.
- A drag ending outside the box closes the overlay, as a click does:
  upstream's `SaveSection::mouseUp` and `DeleteSection::mouseUp` close on
  any mouse up outside the box.

**Tests** (`tests/preset_selector.rs`)

- `the_save_section_takes_drags_meant_for_controls_beneath`: a drag on
  oscillator 1's distortion knob, outside the save box, reaches no host
  parameter and closes the section. It fails with the old click-only
  sensing.
- `the_delete_section_takes_drags_meant_for_controls_beneath`: the same
  for the header's volume under the delete section.

No audio change.
