## 2026-10-27: GUI G5e (3b), drawing on the Wave Source overlay

The second half of G5e-3: the Wave Source overlay's editors edit the
selected keyframe's frame, and a drag is heard as it is drawn.

**Port** (`sections/wave_source_overlay.rs`, `sections/wavetable_edit.rs`,
`interface.rs`, `editor.rs`, `port/crates/plugin/src/editor.rs`,
`port/crates/engine/src/edit_bus.rs`)

- `WaveSourceOverlay` keeps each overlay's state between frames: its
  controls, its editors' presses and pointer positions, and its bar
  editors' bars. Upstream keeps the bars between edits: they load when a
  keyframe is selected (`frameSelected`) or the waveform is drawn
  (`valuesChanged`), and a bar edit leaves them as drawn while the frame
  rebuilt from them normalises. The power scale converts them as
  `BarRenderer::setPowerScale` does.
- The waveform editor (`WaveSourceEditor::mouseDown`, `mouseDrag`,
  `mouseUp`, `changeValues`): a press starts at the grid point nearest it
  and a drag draws the line from the last edit to the pointer snapped to the
  grid, or to the last edit when it isn't within the snap radius
  (`snapToGrid`, comparing JUCE's truncated integer distance). Without a
  grid it draws freehand. The hover circle sits on the nearest grid point,
  or on the last edit while drawing, and a lighter line runs from the last
  edit to the pointer while drawing. The release reports a change after a
  popup-menu press too, as upstream's `mouseUp` does, so a right-click
  commits once.
- The bar editors (`BarEditor::changeValues`, scaled by the zoom), with the
  hovered bar lightened (`editing_quad_`). The controls background lies over
  the top of the amplitudes and takes the presses and the pointer there, as
  upstream's `ControlsBackground` component on top of the bar editor does. Their changes rebuild the frame
  from every bar (`loadFrequencyDomain`: polar amplitudes and phases,
  inverse transform, normalise, forward transform).
- The menus: the waveform's Clear, Flip Vertical and Flip Horizontal; the
  bars' Clear, Clear Left, Clear Right (of the last press), Clear Odd, Clear
  Even and Randomize (bin 0 off, the rest from each bar editor's own
  generator, seeded when first used). The phases clear to the default
  phase.
- `frameChanged` during a drag renders the frame under the playhead into
  the playing and drawn tables (`EditorModel::audition_wavetable_frame`,
  `EditHandle::set_wavetable_frame`, with the drag's token read at its first
  change), and the release or a menu choice commits the whole table
  (`frameDoneEditing`, `set_wavetable_creator`), as upstream's
  `render(position)` and `render()` do. Only the commit changes the saved
  creator (D18); nothing writes to the host. A commit the model refuses (a
  full edit queue) ends the drag instead, and the section reads the saved
  creator back and reloads the bars from it, so the drawn frames don't keep
  playing.
- `PendingAudition`: the drag that is auditioning, shared with the editor
  window. A window that closes or fails during it, or a section hidden
  during it, ends it (`EditorModel::end_wavetable_audition`), so the
  playing table goes back to the saved creator.
- The edit bus keeps `END_AUDITION_RESERVE` (one per oscillator) queue
  entries free for `end_audition`: every other edit, drawn frame and commit
  leaves them. An oscillator's flag records whether a drawn frame reached
  its tables since its last commit, end or restore; an end without one
  queues nothing (the tables are the document's already) and only makes the
  token stale. So the free entries never drop below the oscillators with a
  table to put back, and an end never meets a full queue, however long the
  audio thread stops draining it; the plugin's model calls it once, with no
  retries. A batch that rolls back after setting a wavetable puts back the
  oscillator's commit count and flag, so a drag auditioning before it can
  still draw and end.
- The section doesn't re-read the creator while an overlay editor is
  pressed, as for a keyframe drag.
- The displays show the frame under the playhead as drawn.

**Tests**

- `tests/wavetable_editor.rs`: drawing with the default grid snaps to grid
  points, auditions the playhead's frame during the drag and commits once on
  release, ending nothing and writing nothing to the host; without a grid a
  press draws where it is; Flip Vertical negates the waveform (two commits:
  the right-click's release, then the choice); Clear on the amplitudes
  silences the frame and a right-click on the bars commits nothing; a drag
  on the amplitudes rebuilds and normalises the frame; a window closing
  during a drag ends its audition once, and a committed drag leaves nothing
  to end.
- Unit tests: grid snapping within the truncated radius, and the line a drag
  draws.
- `tests/wavetable_editor.rs`: a drag whose commit is refused ends its
  audition once, commits nothing, and the editor reads the saved creator
  back; a press on a title strip or a gap of the controls background over
  the amplitudes draws nothing and commits nothing.
- Engine (`tests/edit_bus.rs`): `ending_auditions_has_room_whatever_else_is_queued`:
  with every oscillator drawn on and the queue filled with other edits and
  frames until they're refused, each drag still ends, putting its table
  back; an end with nothing drawn queues nothing and makes its token stale.
  `a_failed_batch_keeps_a_drags_audition`. `a_full_queue_refuses_without_changing_the_mirror`
  now fills the queue up to the reserve, which its edits leave.
- Plugin: `an_ended_wavetable_audition_goes_back_to_the_saved_table`: a drawn
  frame plays and draws without touching the saved creator, and ending the
  drag with the queue otherwise full puts the saved table back at once,
  after which the drag's token is stale.

No audio change outside the editor's drags.
