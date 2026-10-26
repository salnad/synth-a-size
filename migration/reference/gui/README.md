# ref-gui: upstream's interface as the editor's reference

`ref-gui` builds upstream's interface (`vital/src/interface`, unmodified)
with JUCE's GUI modules and OpenGL, loads a preset, selects a tab and either
writes the rendered frame as a PNG or dumps the bounds of every named
component. The port's editor (`port/crates/gui`) is checked against both.

It is a separate CMake project from the audio reference (`../`), so the
parity build keeps its headless configuration and needs no GUI libraries.
Like the audio reference, it compiles upstream's unity files with
`NO_AUTH=1` (rule 4); no audio device is opened.

## Build and run

Linux, GCC, CMake 3.16+, and the X11, freetype and OpenGL development
packages (on Ubuntu: `libfreetype-dev libgl-dev libx11-dev libxext-dev
libxinerama-dev libxcursor-dev libxrandr-dev`), plus `xvfb` and Mesa
(`libgl1-mesa-dri`) to run headless.

```bash
python3 migration/scripts/gui_reference.py build
python3 migration/scripts/gui_reference.py layout          # writes migration/corpus/gui/layout/*.json
python3 migration/scripts/gui_reference.py layout --check  # CI: fails if they are stale
python3 migration/scripts/gui_reference.py frames --out build/gui/upstream
```

The script runs itself under `xvfb-run` when there is no `DISPLAY`. The tool
itself:

```
ref-gui [--out frame.png] [--layout layout.jsonl] [--preset file.vital]
        [--tab voice|effects|matrix|advanced] [--width 1400] [--height 820] [--frames 30]
        [--effects-scroll 0] [--animate 0|1]
```

Frames render with animation off, so views driven by the (silent) engine
draw at rest. `--animate 1` (a view's `"animate": true` in
`migration/corpus/gui/views.json`) turns it on for views of the modulation
overlay, which upstream draws only while animating.

## Rules

- Frames show upstream's branding. They are CI artifacts only and are
  never committed. The layout fixtures are plain geometry and are committed.
- Presets passed to it are authored content (`migration/corpus`), never
  factory presets (rule 3).
- `migration/corpus/gui/layout_paths.txt` lists the components whose bounds
  are checked; each GUI PR adds the ones it ports, then regenerates the
  fixtures.
