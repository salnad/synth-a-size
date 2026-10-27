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
        [--midi file.mid] [--seconds S] [--block-size 64] [--animate on|off]
        [--overlay preset_browser|save_section|popup_browser|wavetable_editor]
        [--wavetable-select group:component[:keyframe]] [--data-dir folder]
```

The interface draws with upstream's `DefaultLookAndFeel`, which the
plugin's editor sets (`SynthEditor`), so components that don't set their
own (text editors, scroll bars) look as they do in the plugin. `ref-gui`
always runs with `HOME` in a fresh temporary folder and the XDG folders
unset (as the other reference tools' `isolateUserEnvironment`, keeping
`XAUTHORITY`), so upstream finds no config, favourites or presets on the
machine and `--data-dir` never writes the user's own config. Run through the
script, it gets `--data-dir migration/corpus/gui/data`: authored presets
(`migration/corpus/generate.py`) that the browser lists and the port's
gallery reads too.

`--overlay preset_browser` opens the preset browser over the tab, as the
header's preset selector does; in `views.json`, `"overlay":
"preset_browser"`. `--overlay wavetable_editor` opens oscillator 1's
wavetable editor in place of the interface, as its edit button does
(`"overlay": "wavetable_editor"`). With it, `--wavetable-select
group:component[:keyframe]` (zero-based, keyframe 0 by default) selects that
keyframe of oscillator 1's wavetable in the editor's organizer, as a click
on it does, so its component's overlay shows (`"wavetable_select":
"0:3"`; `@wavetable-select 0:3` in `layout_paths.txt`).

## A running engine

By default the engine never processes audio, and the interface is drawn
with animation off, so the views that read engine outputs show the
controls at rest. With `--midi` or `--seconds`, `ref-gui` first runs the
engine the way `ref-render` does: the same `prepare` and block loop
(`RenderLoop` in `../src/render_synth.h`), the same MIDI reader, 44.1 kHz
and 120 BPM. It renders `S` seconds of audio (default: up to the last MIDI
event) before the interface is on screen, then hands the interface the
oscilloscope and audio memory, as the plugin's editor does. Displays that
read status outputs or that memory then show that state: chorus and
flanger taps, compressor meters, the EQ spectrum, the oscilloscope, the
level meter and held keys.

`--animate on` draws the animated views (filter responses, envelope and LFO
positions and the like) from the engine's outputs, as the plugin's editor
does by default. It needs a render that isn't empty (`--midi`, or
`--seconds` above 0), because a silent engine's outputs are idle (a filter
response at cutoff 0).

The port's gallery (`port/crates/gui/examples/gallery.rs`) runs its engine
the same way for such a view: the preset and MIDI through
`sas_engine::synth::render` at 44.1 kHz and 120 BPM in 64-sample blocks,
then the editor reads that engine's telemetry and held notes. Live views
compare like for like. One difference remains: the port's editor has no
animate switch yet, so a live view without `"animate": true` still draws its
animated views from the engine (the gallery prints a note).

In `views.json`, a view sets these with `"midi"` (an authored file in
`migration/corpus/midi/`), `"seconds"` and `"animate": true`.

## Rules

- Frames show upstream's branding. They are CI artifacts only and are
  never committed. The layout fixtures are plain geometry and are committed.
- Presets passed to it are authored content (`migration/corpus`), never
  factory presets (rule 3).
- `migration/corpus/gui/layout_paths.txt` lists the components whose bounds
  are checked; each GUI PR adds the ones it ports, then regenerates the
  fixtures.
