// ref-render: deterministic offline render of a preset + MIDI file using the
// upstream engine. See migration/reference/README.md for the full contract.

#include "JuceHeader.h"
#include "harness_io.h"
#include "render_synth.h"
#include "load_save.h"
#include "utils.h"

#include <cmath>
#include <iostream>

namespace {

using harness::RenderSynth;
using harness::TimedMessage;

const char* kUsage = R"(usage: ref-render [options] <preset.vital|--init> <input.mid|--no-midi> -o <out.wav>

Renders a preset driven by a Standard MIDI File through the upstream engine.

Options:
  --preset PATH        preset file (or first positional argument)
  --midi PATH          MIDI file (or second positional argument)
  --init               render the engine's init state instead of a preset
  --no-midi            render with no MIDI input
  -o, --out PATH       output WAV path (required)
  --sample-rate HZ     default 44100
  --block-size N       host block size, default 64 (engine sub-blocks of <=128,
                       exactly like the upstream plugin's processBlock)
  --length SECONDS     total output length; default: last MIDI event + --tail
  --tail SECONDS       extra time after the last MIDI event, default 2.0
  --bpm BPM            tempo for tempo-synced modulators, default 120
  --tuning PATH        load a .scl or .tun tuning file after the preset
  --keyboard-map PATH  then load a .kbm keyboard map on top of it
  --edits PATH         a JSON edit script: live edits applied while rendering,
                       each at the start of the first host block that starts
                       at or after its time (see README.md); an edit after
                       the end of the render is an error
  --seed N             set the engine's global RandomGenerator seed counter
                       before the synth is constructed (default: leave the
                       upstream counter untouched; output is deterministic
                       either way)
  --warmup-samples N   process N samples of silence before t=0 (not written),
                       default 0
  --format f32|s16|s24 WAV sample format, default f32 (bit-exact)
  --meta PATH          write render metadata JSON (default: <out>.json)
  --dump-state PATH    write the engine state after loading, as preset JSON
  --keep-home          do not isolate HOME (by default the harness points HOME
                       at an empty temp dir so user config cannot leak in)
)";

int fail(const std::string& message) {
  std::cerr << "ref-render: " << message << std::endl;
  return 2;
}

} // namespace

int main(int argc, const char* argv[]) {
  harness::pinLibmVariants(argc, argv);
  harness::Args args(argc, argv, { "init", "no-midi", "keep-home", "help" });
  if (args.has("help") || argc == 1) {
    std::cout << kUsage;
    return args.has("help") ? 0 : 2;
  }
  if (!args.unknown().empty())
    return fail("missing value for " + args.unknown()[0]);

  std::vector<std::string> positional = args.positional();
  size_t next_positional = 0;
  std::string preset_path = args.get("preset");
  bool use_init = args.has("init");
  if (preset_path.empty() && !use_init && next_positional < positional.size())
    preset_path = positional[next_positional++];
  std::string midi_path = args.get("midi");
  bool no_midi = args.has("no-midi");
  if (midi_path.empty() && !no_midi && next_positional < positional.size())
    midi_path = positional[next_positional++];
  std::string out_path = args.get("out");

  if (out_path.empty())
    return fail("an output path is required (-o out.wav)");
  if (preset_path.empty() && !use_init)
    return fail("a preset is required (or pass --init)");
  if (midi_path.empty() && !no_midi)
    return fail("a MIDI file is required (or pass --no-midi)");

  const int sample_rate = static_cast<int>(args.getInt("sample-rate", 44100));
  const int block_size = static_cast<int>(args.getInt("block-size", 64));
  const double tail = args.getDouble("tail", 2.0);
  const double bpm = args.getDouble("bpm", 120.0);
  const long long warmup_samples = args.getInt("warmup-samples", 0);
  harness::WavFormat format = harness::WavFormat::kFloat32;
  if (!harness::parseWavFormat(args.get("format", "f32"), format))
    return fail("unknown --format " + args.get("format"));
  if (sample_rate <= 0 || sample_rate > vital::kMaxSampleRate)
    return fail("--sample-rate out of range");
  if (block_size <= 0 || block_size > 65536)
    return fail("--block-size out of range");

  if (!args.has("keep-home"))
    harness::isolateUserEnvironment();

  ScopedJuceInitialiser_GUI juce_init;

  if (args.has("seed"))
    vital::utils::RandomGenerator::next_seed_ = static_cast<int>(args.getInt("seed", 0));

  std::unique_ptr<RenderSynth> synth = std::make_unique<RenderSynth>();

  if (use_init) {
    synth->loadInitPreset();
  }
  else {
    File preset = File::getCurrentWorkingDirectory().getChildFile(preset_path);
    if (!preset.existsAsFile())
      return fail("preset not found: " + preset_path);
    std::string error;
    if (!synth->loadFromFile(preset, error))
      return fail("failed to load preset " + preset_path + ": " + error);
  }

  // SynthBase::loadTuningFile, as the plugin's tuning menu applies a file:
  // the scale first, then a keyboard map on top of it.
  for (const char* key : { "tuning", "keyboard-map" }) {
    if (!args.has(key))
      continue;
    File tuning_file = File::getCurrentWorkingDirectory().getChildFile(args.get(key));
    if (!tuning_file.existsAsFile())
      return fail(std::string(key) + " file not found: " + args.get(key));
    synth->loadTuningFile(tuning_file);
  }

  if (args.has("dump-state")) {
    if (!harness::writeTextFile(args.get("dump-state"), synth->state().dump(2) + "\n"))
      return fail("cannot write " + args.get("dump-state"));
  }

  struct TimedEdit {
    long long sample;
    json edit;
  };
  std::vector<TimedEdit> edits;
  if (args.has("edits")) {
    File edits_file = File::getCurrentWorkingDirectory().getChildFile(args.get("edits"));
    if (!edits_file.existsAsFile())
      return fail("edits file not found: " + args.get("edits"));
    try {
      json script = json::parse(edits_file.loadFileAsString().toStdString());
      long long last_sample = 0;
      for (const json& edit : script.at("edits")) {
        double seconds = edit.at("seconds").get<double>();
        long long sample = static_cast<long long>(std::floor(seconds * sample_rate + 0.5));
        if (!(seconds >= 0.0) || sample < last_sample)
          return fail("edit times must be non-negative and in order");
        last_sample = sample;
        edits.push_back({ sample, edit });
      }
    }
    catch (const json::exception& e) {
      return fail(std::string("cannot read edits: ") + e.what());
    }
  }

  std::vector<TimedMessage> events;
  double last_event_seconds = 0.0;
  if (!no_midi) {
    std::string error;
    if (!harness::loadMidi(midi_path, sample_rate, events, last_event_seconds, error))
      return fail(error);
  }

  double length_seconds = args.has("length") ? args.getDouble("length", 0.0) : last_event_seconds + tail;
  if (!(length_seconds > 0.0))
    return fail("render length must be positive");
  const long long total_samples = static_cast<long long>(std::ceil(length_seconds * sample_rate));

  synth->prepare(sample_rate, bpm);

  double seconds_time = -static_cast<double>(warmup_samples) / sample_rate;
  {
    AudioSampleBuffer scratch(2, block_size);
    MidiBuffer empty;
    for (long long done = 0; done < warmup_samples;) {
      int n = static_cast<int>(std::min<long long>(block_size, warmup_samples - done));
      scratch.setSize(2, n, false, false, true);
      synth->renderBlock(scratch, empty, seconds_time, sample_rate);
      done += n;
    }
  }
  seconds_time = 0.0;

  std::vector<float> interleaved(static_cast<size_t>(total_samples) * 2);
  AudioSampleBuffer buffer(2, block_size);
  size_t event_index = 0;
  size_t dropped_events = 0;
  size_t edit_index = 0;

  for (long long block_start = 0; block_start < total_samples; block_start += block_size) {
    int n = static_cast<int>(std::min<long long>(block_size, total_samples - block_start));
    buffer.setSize(2, n, false, false, true);
    buffer.clear();

    MidiBuffer midi;
    while (event_index < events.size() && events[event_index].sample < block_start + n) {
      long long offset = events[event_index].sample - block_start;
      midi.addEvent(events[event_index].message, static_cast<int>(std::max(0LL, offset)));
      ++event_index;
    }

    while (edit_index < edits.size() && edits[edit_index].sample <= block_start) {
      std::string error;
      try {
        error = synth->applyEdit(edits[edit_index].edit);
      }
      catch (const json::exception& e) {
        error = e.what();
      }
      if (!error.empty())
        return fail("edit " + std::to_string(edit_index) + ": " + error);
      ++edit_index;
    }

    synth->renderBlock(buffer, midi, seconds_time, sample_rate);

    for (int i = 0; i < n; ++i) {
      size_t frame = static_cast<size_t>(block_start + i);
      interleaved[2 * frame] = buffer.getSample(0, i);
      interleaved[2 * frame + 1] = buffer.getSample(1, i);
    }
  }
  dropped_events = events.size() - event_index;
  if (edit_index < edits.size())
    return fail(std::to_string(edits.size() - edit_index) + " edits fall after the end of the render");

  size_t non_finite = 0;
  for (float& sample : interleaved) {
    if (!std::isfinite(sample))
      ++non_finite;
  }

  std::string error;
  if (!harness::writeWav(out_path, interleaved, 2, sample_rate, format, error))
    return fail(error);

  harness::json meta;
  meta["tool"] = "ref-render";
  meta["build"] = harness::buildInfo();
  meta["preset"] = use_init ? "<init>" : preset_path;
  meta["midi"] = no_midi ? "<none>" : midi_path;
  meta["output"] = out_path;
  meta["format"] = harness::wavFormatName(format);
  meta["channels"] = 2;
  meta["sample_rate"] = sample_rate;
  meta["block_size"] = block_size;
  meta["engine_max_sub_block"] = vital::kMaxBufferSize;
  meta["bpm"] = bpm;
  if (args.has("tuning"))
    meta["tuning"] = args.get("tuning");
  if (args.has("keyboard-map"))
    meta["keyboard_map"] = args.get("keyboard-map");
  if (args.has("edits")) {
    meta["edits"] = args.get("edits");
    meta["edits_applied"] = edit_index;
  }
  meta["seed"] = args.has("seed") ? harness::json(args.getInt("seed", 0)) : harness::json("upstream-default");
  meta["warmup_samples"] = warmup_samples;
  meta["length_seconds"] = length_seconds;
  meta["frames"] = total_samples;
  meta["midi_events"] = events.size();
  meta["midi_events_after_end"] = dropped_events;
  meta["non_finite_samples"] = non_finite;
  meta["samples_fnv1a64"] = harness::hex64(harness::fnv1a64(interleaved.data(), interleaved.size() * sizeof(float)));

  std::string meta_path = args.get("meta", out_path + ".json");
  if (!harness::writeTextFile(meta_path, meta.dump(2) + "\n"))
    return fail("cannot write " + meta_path);

  std::cout << out_path << ": " << total_samples << " frames @ " << sample_rate << " Hz, hash "
            << meta["samples_fnv1a64"].get<std::string>() << std::endl;

  if (non_finite)
    return fail(std::to_string(non_finite) + " non-finite samples in output");
  return 0;
}
