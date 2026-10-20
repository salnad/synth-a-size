# Reference harness

This folder builds the upstream engine from `/vital` and links two
command-line tools that produce the reference audio the port is tested
against:

- `ref-render`: renders a full patch (preset + MIDI file) to WAV.
- `ref-module`: drives one engine `Processor` and dumps its input and
  output buffers to `.npy`.

Nothing in `/vital` is modified. All build glue lives here.

## Build

Requirements: Linux x86_64, GCC (upstream's Linux compiler; tested with
GCC 13.3), CMake 3.16 or newer. JUCE needs no extra system packages,
because the harness disables curl and only uses the non-GUI modules
(`audio_basics`, `audio_formats`, `core`, `data_structures`, `dsp`,
`events`).

```bash
migration/scripts/build_reference.sh            # -> migration/reference/build/{ref-render,ref-module}
migration/scripts/build_reference.sh -DREF_FAST_MATH=OFF   # extra CMake args are passed through
```

A full build takes about 30 s on 8 cores. macOS and arm64 are not
supported or tested; the reference goldens are defined as Linux x86_64
builds.

### How the build differs from upstream

The engine is compiled from upstream's own unity files
(`vital/src/unity_build/common.cpp` and `synthesis.cpp`) with the
headless target's defines (`HEADLESS=1`, `NO_AUTH=1`, `LINUX=1`) and the
same include paths. JUCE is configured by
[`juce_config/AppConfig.h`](juce_config/AppConfig.h), which includes
upstream's `vital/headless/JuceLibraryCode/AppConfig.h` verbatim after
setting these overrides:

| Setting | Upstream headless | Harness | Why |
|---|---|---|---|
| `JUCE_USE_CURL` | 1 | 0 | No network code is needed, and dropping it removes the libcurl build dependency. |
| `JUCE_DSP_USE_SHARED_FFTW` | 1 | 0 | With 1, JUCE `dlopen`s libfftw3f when it happens to be installed, so FFT results (used in wavetable rendering) would depend on the machine. The harness always uses JUCE's built-in FFT. |
| `ProjectInfo::versionString` | `99999.9.9` | `1.0.6` | Matches the shipped plugin, so preset version upgrades (`LoadSave::jsonToState`) and saved `synth_version` fields behave like the plugin. See [`juce_config/JuceHeader.h`](juce_config/JuceHeader.h). |
| Optimisation | `-Ofast -flto` | `-O3 -ffast-math -U__FAST_MATH__ -mno-recip` (no LTO) | Fast-math semantics like upstream release builds, minus the two features that made output CPU-dependent: `-mno-recip` keeps division and `sqrtf` exact instead of `RCPPS`/`RSQRTPS` plus a Newton step, and `-U__FAST_MATH__` stops GCC auto-vectorising libm calls into glibc libmvec. LTO is skipped for build time. `REF_FAST_MATH=OFF` drops fast-math. |
| Target ISA | build-host default | `-march=x86-64 -mtune=generic -msse2 -mfpmath=sse -ffp-contract=off` | Pinned so that no build-host feature, and no FMA contraction, can leak in. |
| libm variants | CPU-dispatched | baseline SSE2 | The tools re-exec once with `GLIBC_TUNABLES=glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4`, because glibc's double-precision `sin`/`exp`/`pow`/... return different bits with and without FMA. `REF_NO_LIBM_PIN=1` opts out. |

Both tools also point `HOME` at an empty temporary directory at startup,
so a developer's upstream user config, user presets or MIDI-learn
mappings can never change the output. `ref-render --keep-home` turns this
off.

## `ref-render`

```text
ref-render [options] <preset.vital|--init> <input.mid|--no-midi> -o <out.wav>

  --sample-rate HZ     default 44100
  --block-size N       host block size, default 64
  --length SECONDS     total length; default = last MIDI event + --tail
  --tail SECONDS       default 2.0
  --bpm BPM            default 120
  --seed N             set the engine's global RandomGenerator seed counter
  --warmup-samples N   silent pre-roll that is processed but not written (default 0)
  --format f32|s16|s24 default f32 (bit-exact float WAV)
  --meta PATH          metadata JSON (default <out>.wav.json)
  --dump-state PATH    write the loaded state back out as preset JSON
```

Example:

```bash
migration/reference/build/ref-render \
  migration/corpus/presets/analog_filter_lfo.vital \
  migration/corpus/midi/chord_progression.mid \
  -o /tmp/out.wav --sample-rate 48000 --block-size 128 --seed 1
```

### Processing contract (the port must reproduce this)

The render loop mirrors `SynthPlugin::processBlock` in
`vital/src/plugin/synth_plugin.cpp` for a host whose transport is always
playing from t=0:

1. Load the preset with `SynthBase::loadFromFile`, or `loadInitPreset`
   for `--init`.
2. Call `engine->setSampleRate`, `engine->setBpm`,
   `engine->updateAllModulationSwitches`, and
   `midi_manager->setSampleRate`.
3. For each host block of `--block-size` samples: run
   `processModulationChanges`, then `processKeyboardEvents`, then split
   the block into engine sub-blocks of at most `kMaxBufferSize` (128)
   samples. For each sub-block run `correctToTime(t)`, deliver the MIDI
   events that fall inside it (`processMidi`), and call `processAudio`.
4. MIDI file times are converted with the file's tempo map. Each event is
   placed at sample `floor(seconds * rate + 0.5)` and delivered in the host
   block that contains it, at that offset. Meta and SysEx events are
   skipped.

The block size changes the output. On `unison_pad_fx` with the chord
MIDI, block 64 and block 128 renders differ by a residual of about
−31 dB, because control-rate values update per sub-block. Port renders
must always be compared at the same sample rate and block size.

### Determinism

- Output is bit-identical across runs of the same binary. CI renders the
  corpus twice and requires identical files.
- Engine randomness (`vital::utils::RandomGenerator`) is a `std::mt19937`
  seeded from a global counter (`RandomGenerator::next_seed_`) that
  increments in construction order. Static initialisation consumes a few
  seeds before `main`. `--seed N` resets the counter to N just before the
  synth is constructed. Without it, the upstream counter is left alone,
  which is still deterministic.
- Random values come from libstdc++'s `std::uniform_real_distribution<float>`.
  To match noise, random phase and random LFOs bit-for-bit, the port must
  reproduce the mt19937 stream, the distribution's float conversion and
  the seed order. Otherwise those cases need statistical comparison
  (PLAN.md, M3/M4).
- Output is bit-identical across x86_64 CPUs for the same build identity
  (compiler, glibc, flags). It was verified between an Intel Xeon and an
  AMD EPYC 9V74, and CI checks every run against the committed
  `migration/corpus/reference_hashes.json` (`run_corpus.py hashes --check`).
  Before the fixes in the table above, the same binary rendered up to 3e-3
  differently on Intel and AMD. The investigation and evidence are in
  [`../decisions/reference-determinism.md`](../decisions/reference-determinism.md).
- `migration/scripts/check_fp_determinism.sh` fails the build if a
  CPU-dependent instruction (`rcp*`, `rsqrt*`, FMA, x87 transcendental) or a
  libmvec call shows up in the binaries. `ref-fp-probe` and
  `migration/scripts/machine_info.sh` record what a machine's libm and
  approximate instructions return, for comparing two machines.
- A different compiler, glibc version or flag set can still change
  low-order bits. Compare across those with `audio-diff` tolerances.

### Metadata (`<out>.wav.json`)

Each render writes its build info (upstream SHA, compiler, glibc, fast-math,
optimisation flags, libm pinning, arch),
all render settings, the frame count, the MIDI event count, a non-finite
sample count and `samples_fnv1a64`, a hash of the raw float samples for
quick equality checks. The exit status is non-zero if any sample is NaN
or infinite.

## `ref-module`

```bash
migration/reference/build/ref-module --list                  # modules, inputs, defaults
migration/reference/build/ref-module spec.json --out-dir DIR
```

Outputs:

- `DIR/in_<input>.npy` and `DIR/out_<output>.npy`: little-endian float32
  arrays of shape `[frames, 4]`. The 4 columns are the 4 `poly_float`
  lanes: voice 0 left, voice 0 right, voice 1 left, voice 1 right.
  Control-rate inputs and outputs are sample-and-hold per block.
- `DIR/capture.json`: the resolved spec, build info, an FNV-1a hash per
  buffer, and the rate of each output.

Spec format (see [`../corpus/modules/`](../corpus/modules/) for real
examples):

```json
{
  "module": "filter",
  "params": { "model": "digital" },
  "sample_rate": 44100,
  "oversample": 1,
  "block_size": 64,
  "num_samples": 22050,
  "seed": 0,
  "inputs": {
    "audio": { "signal": "saw", "freq_hz": 110.0, "amp": 0.5 },
    "reset": { "trigger": [ { "sample": 0, "value": "voice_on" } ] },
    "midi_cutoff": { "signal": "ramp", "from": 30.0, "to": 110.0 },
    "resonance": 0.7
  }
}
```

- `num_samples`, `block_size` and signal frequencies are in the
  processor's internal rate, `sample_rate * oversample`.
  `block_size <= 128 * oversample`.
- Each input is one of:
  - a number, or `{"value": x}`, or `{"value": [l0, l1, l2, l3]}` for a
    constant;
  - `{"signal": "sine"|"saw"|"square"|"noise"|"impulse"|"ramp"|"silence", ...}`
    with options `amp`, `offset`, `freq_hz`, `phase`, `from`/`to` for ramp,
    `at` for impulse, and `seed` and `stereo` for noise;
  - `{"npy": "path.npy"}` with shape `[N]`, `[N,1]`, `[N,2]` (stereo,
    duplicated to both voices) or `[N,4]`;
  - `{"trigger": [{"sample": n, "value": "voice_on"|"voice_off"|"voice_kill"|..., "voices": "all"|"0"|"1"}]}`.
- `"rate": "audio"|"control"` overrides an input's default rate. Inputs
  that are not given use the registry default shown by `--list`.
  Unknown inputs are an error.
- Input values are what arrives at the processor's input port, after any
  parameter scaling the full engine would apply. For example, filter
  `midi_cutoff` is in semitones and `drive_gain` is in dB.

Registered modules: `filter` (every model via `SynthFilter::createFilter`),
`distortion`, `envelope`, `dc_filter`, `stereo_delay`.

To add one, append an entry to `registry()` in
[`src/module_main.cpp`](src/module_main.cpp) with a factory, the input
names mapped to the processor's input enum and default values and rates,
and the outputs. Then add a spec to `migration/corpus/modules/` and a
case to `migration/corpus/cases.json`.

## Files

| File | Purpose |
|---|---|
| `CMakeLists.txt` | Builds `upstream_engine` (JUCE + unity files) and the two tools |
| `juce_config/` | JUCE config and module units for the harness |
| `src/render_main.cpp` | `ref-render` |
| `src/module_main.cpp` | `ref-module` and its module registry |
| `src/harness_io.{h,cpp}` | WAV / NPY I/O, HOME isolation, libm pinning, hashing, argument parsing |
| `src/fp_probe.cpp` | `ref-fp-probe`: hashes libm, `rcpps`/`rsqrtps` and libmvec results for cross-machine comparison |
