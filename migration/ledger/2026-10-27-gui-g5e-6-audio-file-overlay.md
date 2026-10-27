## 2026-10-27: GUI G5e (6), the wavetable editor's Audio File Source overlay

Taken over from Wavetable Builder's G5e plan, on the overlay dispatch: the
overlay for an Audio File Source's keyframes
(`vital/src/interface/wavetable/overlays/file_source_overlay.cpp`).

**Port** (`sections/file_source_overlay.rs`, `sections/wavetable_edit.rs`)

- **Dispatch:** `OverlayKind::FileSource` for `ComponentType::FileSource`,
  with an arm in `overlay_input`, `paint_overlay` and `show_overlay`. It
  covers no display (`obscures` is all false), as `FileSourceOverlay`
  keeps the base's bounds setters, which return false.
- **Layout** (`setEditBounds`): along the edit strip, inset 1.5 strip
  heights each side, the controls background with its seven titles and six
  lines; the file viewer, the start position and window size text fields
  (2.5 heights), the window fade (3), the blend and phase style selectors
  (3 each) and the normalize toggle (2.5, a sixth of the height in).
- **Controls** (`frameSelected`, `sliderValueChanged`, `sliderDragEnded`,
  `buttonClicked`): they load from the keyframe when it's selected. The
  window fade sets the keyframe's fade and auditions, committing when the
  press ends; the style selectors set the source's styles (`setPhaseStyle`
  bumps the vocode seed) and commit; the toggle flips the source's
  normalize and commits. They're local controls, one set per oscillator,
  never host parameters. The window fade has no parameter assignment, so it
  shows JUCE's slider text, seven decimals, and double-clicks to 1.
- **Text fields** (`loadStartingPositionText`, `loadWindowSizeText`, on
  Return or focus loss): the start position is clamped to the audio less a
  window and set on the keyframe; the window size reads a number (1 to
  9999.9) or a note's period at the source's sample rate (`windowTextToSize`,
  with `noteToMidiKey` counting A and B below the octave's C). Upstream's
  fields end in the light face at half their height, centred.
- **Viewer** (`AudioFileViewer`): 256 points, each the largest sample in
  its share of the audio, mirrored about the middle with a 0.1-alpha fill,
  brightened inside the window (`setWindowValues`). A drag moves the start
  position by its share of the audio less a window, auditioning, and the
  release commits (`positionMovedRelative`).
- **Loading** (`AudioFileDropSource`, `loadFile`): only by dropping one
  audio file on the viewer, which highlights while a file it takes is over
  it. Upstream's LOAD button is never given bounds, so it never shows, and
  there's no dialog (coordinator, 2026-10-27). The file's first channel is
  read up to 176,400 frames (`EditorModel::read_audio_file`, the plugin's
  `read_audio_channels` with its frame limit) and loads at sample rate 0,
  as upstream's viewer never sets its rate (kept for the sound and the
  saved preset; the vocode morph falls back to 88.2 kHz). The window
  becomes the detected period (`detectPitch`), and the start position is
  clamped, applied and committed. The window size text keeps its old
  value, as upstream's does.
- **D33**: a file that can't be read, or audio shorter than pitch
  detection needs, loads nothing (approved by the coordinator,
  2026-10-27).
- Edits go through the creator's copy and `set_wavetable_creator`, and
  auditions through `frame_edited` and `PendingAudition`: nothing is
  written to the host, so D18 doesn't apply. A viewer press, or a window
  fade change until its drag ends, counts as pressed, so the section keeps
  its copy of the creator through a change that reaches the model
  mid-drag.
- Supporting changes: local controls round only when indexed; `Format`
  gains JUCE's text for a slider with no parameter;
  `LineRenderer::set_boost_left`; the text fields can centre and keep their
  prompt as an accessible name only; a toggle without background
  (`just_text_toggle`).

**Tests**

- `tests/wavetable_editor.rs`:
  - `an_audio_file_source_keyframe_shows_its_overlay`: the controls load
    the keyframe's fade and the source's styles, with nothing committed or
    written to the host.
  - `dropping_audio_on_the_viewer_loads_it`: the read asks for 176,400
    frames; the audio loads at sample rate 0 with the detected window, and
    commits once.
  - `unreadable_or_short_audio_loads_nothing`: a missing file and 8095
    samples leave the source unchanged and commit nothing (D33).
  - `dragging_the_viewer_moves_the_start_position`: half the viewer moves
    the start by half the audio less a window, auditioning, then commits on
    release.
- `a_refresh_during_a_window_fade_drag_keeps_the_drag`: a model change
  during a window fade drag leaves the committed fade the drag's.
- `the_file_source_overlay_obscures_nothing`: the mapping, and `obscures`
  all false.
- `file_source_overlay` unit tests: the window size text (numbers, clamps,
  note names at 0 Hz and 44.1 kHz) and the viewer's peaks and window
  boosts.

No audio change.
