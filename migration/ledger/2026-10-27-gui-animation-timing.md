## 2026-10-27: GUI, animation steps once per frame; the gallery's live views match ref-gui's frame count and seeds

**Why.** With live views stepped 30 frames (G4c-3b (2)), `all_effects_live`
at first measured further from ref-gui than at 2 frames. The coordinator
asked which effect parts differ over time.

**Findings.**

- **The gallery rendered 36 frames, not 30.** Building the kittest harness
  already runs a few frames, so `run_steps(30)` gave 36. The equalizer's
  spectrum moved 36 times against upstream's 30.
- **Each view depended on the views before it.** ref-gui renders every
  view in a fresh process. The gallery renders them all in one, so each
  view's engine took its seeds from where the previous views left the
  counter. Adding a view earlier in `views.json` changed later live views:
  this is where the first measurement's +188 pixels came from. ref-gui's
  own frames are byte-identical from run to run, and its counter is 0 when
  it builds its synth: setting it to 0 explicitly changes no frame.
- **The animations stepped once per call, not once per frame.** The
  spectrogram's amplitudes, the volume meter's clamp decay and the line
  editors' boosts stepped every time a view was shown. egui can run a frame
  in more than one pass, while upstream steps once per render. The
  gallery's harness runs one pass per frame, so this changes none of its
  images, but a real editor frame with a discarded pass would have stepped
  twice.
- **What still differs over time** in `all_effects_live` and the other
  live views:
  - **The header oscilloscope:** it shows the same waveform at a different
    phase. The port reads the synth's oscilloscope memory as upstream does
    (`updateMemoryOutput`), so the difference is in the engine's window of
    that memory, not in the view. That belongs to the engine side.
  - **The equalizer spectrum and the volume meter:** they follow the
    engine's output, and they converge as upstream's do once the frame
    count matches.
  - **The Random 1 viewer** (on the Voice and Matrix tabs): upstream's
    `RandomViewer` animates with its boosts (`kDecayMult` 0.9). The port's
    is drawn at rest (`sections/random.rs`, "Not yet").

**Changed.**

- **`SpectrogramAnalysis::update_once`, the volume meter's clamp decay and
  `LineEditor::animate`** now step once per egui frame
  (`cumulative_frame_nr`), as upstream steps once per render.
- **Gallery:**
  - It restores the seed counter to its starting value before each view,
    so each renders as from a fresh process.
  - Live views run exactly ref-gui's 30 frames, counting the frames the
    harness already ran.

**Verified.**

- **Pixels more than 24 off ref-gui (1400 × 820), before → after:**
  `all_effects_live` 24,913 → 24,729, `modulations_live` 23,413 → 22,508,
  `modulations_matrix_live` 34,051 → 32,644. The still views are unchanged.
- **Frame count:** the equalizer's spectrum now moves 30 times in
  `all_effects_live`, checked with a temporary counter.
- **Determinism:** two gallery runs, and two ref-gui runs, are each
  byte-identical.
- **Checks:** `cargo clippy --workspace --all-targets -D warnings`, the
  `sas-gui` and `sas-plugin` tests, `gui_reference.py layout --check` and
  `ledger.py --check` all pass.
