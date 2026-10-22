// ref-bench: times the upstream engine's offline render (the same block loop
// as ref-render) for the performance benchmark in migration/BENCHMARKS.md.
// Prints one JSON object on stdout; the port's sas-bench prints the same
// fields, and migration/scripts/bench.py compares the two.

#include "JuceHeader.h"
#include "harness_io.h"
#include "render_synth.h"
#include "utils.h"

#include <time.h>

#include <algorithm>
#include <chrono>
#include <cmath>
#include <iostream>

namespace {

using harness::RenderSynth;
using harness::TimedMessage;

const char* kUsage = R"(usage: ref-bench [options] <preset.vital|--init> <input.mid|--no-midi>

Times offline renders of a preset driven by a MIDI file through the upstream
engine, block by block exactly like ref-render, and prints JSON on stdout.
Each repeat builds a fresh synth (not timed), loads the preset and renders
--length seconds; only the block loop is timed.

Options:
  --preset PATH / --midi PATH / --init / --no-midi   as ref-render
  --sample-rate HZ     default 44100
  --block-size N       host block size, default 64
  --length SECONDS     render length (required)
  --bpm BPM            default 120
  --seed N             engine seed counter before each synth is built, default 1
  --warmup N           untimed renders before the timed ones, default 1
  --repeats N          timed renders, default 3
  -o, --out PATH       also write the last render as a float WAV
  --keep-home          do not isolate HOME
)";

int fail(const std::string& message) {
  std::cerr << "ref-bench: " << message << std::endl;
  return 2;
}

double threadCpuSeconds() {
  timespec now;
  clock_gettime(CLOCK_THREAD_CPUTIME_ID, &now);
  return now.tv_sec + now.tv_nsec * 1e-9;
}

double percentile(std::vector<double> values, double fraction) {
  if (values.empty())
    return 0.0;
  std::sort(values.begin(), values.end());
  size_t index = static_cast<size_t>(std::ceil(fraction * values.size()));
  return values[std::min(values.size(), std::max<size_t>(index, 1)) - 1];
}

struct Run {
  double wall_seconds = 0.0;
  double cpu_seconds = 0.0;
  std::vector<double> block_seconds;
  std::vector<float> interleaved;
};

struct Settings {
  std::string preset_path;
  bool use_init = false;
  std::vector<TimedMessage> events;
  int sample_rate = 44100;
  int block_size = 64;
  double bpm = 120.0;
  int seed = 1;
  long long total_samples = 0;
};

bool renderOnce(const Settings& settings, Run& run, std::string& error) {
  vital::utils::RandomGenerator::next_seed_ = settings.seed;
  std::unique_ptr<RenderSynth> synth = std::make_unique<RenderSynth>();
  if (settings.use_init)
    synth->loadInitPreset();
  else {
    File preset = File::getCurrentWorkingDirectory().getChildFile(settings.preset_path);
    if (!synth->loadFromFile(preset, error)) {
      error = "failed to load preset " + settings.preset_path + ": " + error;
      return false;
    }
  }
  synth->prepare(settings.sample_rate, settings.bpm);

  const int block_size = settings.block_size;
  const long long total_samples = settings.total_samples;
  run.interleaved.assign(static_cast<size_t>(total_samples) * 2, 0.0f);
  run.block_seconds.clear();
  run.block_seconds.reserve(static_cast<size_t>(total_samples / block_size + 1));
  AudioSampleBuffer buffer(2, block_size);
  double seconds_time = 0.0;
  size_t event_index = 0;

  using Clock = std::chrono::steady_clock;
  double cpu_start = threadCpuSeconds();
  Clock::time_point start = Clock::now();
  for (long long block_start = 0; block_start < total_samples; block_start += block_size) {
    int n = static_cast<int>(std::min<long long>(block_size, total_samples - block_start));
    buffer.setSize(2, n, false, false, true);
    buffer.clear();

    MidiBuffer midi;
    while (event_index < settings.events.size() && settings.events[event_index].sample < block_start + n) {
      long long offset = settings.events[event_index].sample - block_start;
      midi.addEvent(settings.events[event_index].message, static_cast<int>(std::max(0LL, offset)));
      ++event_index;
    }

    Clock::time_point block_begin = Clock::now();
    synth->renderBlock(buffer, midi, seconds_time, settings.sample_rate);
    run.block_seconds.push_back(std::chrono::duration<double>(Clock::now() - block_begin).count());

    for (int i = 0; i < n; ++i) {
      size_t frame = static_cast<size_t>(block_start + i);
      run.interleaved[2 * frame] = buffer.getSample(0, i);
      run.interleaved[2 * frame + 1] = buffer.getSample(1, i);
    }
  }
  run.wall_seconds = std::chrono::duration<double>(Clock::now() - start).count();
  run.cpu_seconds = threadCpuSeconds() - cpu_start;
  return true;
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

  Settings settings;
  std::vector<std::string> positional = args.positional();
  size_t next_positional = 0;
  settings.preset_path = args.get("preset");
  settings.use_init = args.has("init");
  if (settings.preset_path.empty() && !settings.use_init && next_positional < positional.size())
    settings.preset_path = positional[next_positional++];
  std::string midi_path = args.get("midi");
  bool no_midi = args.has("no-midi");
  if (midi_path.empty() && !no_midi && next_positional < positional.size())
    midi_path = positional[next_positional++];
  if (settings.preset_path.empty() && !settings.use_init)
    return fail("a preset is required (or pass --init)");
  if (midi_path.empty() && !no_midi)
    return fail("a MIDI file is required (or pass --no-midi)");

  settings.sample_rate = static_cast<int>(args.getInt("sample-rate", 44100));
  settings.block_size = static_cast<int>(args.getInt("block-size", 64));
  settings.bpm = args.getDouble("bpm", 120.0);
  settings.seed = static_cast<int>(args.getInt("seed", 1));
  const double length_seconds = args.getDouble("length", 0.0);
  const int warmup = static_cast<int>(args.getInt("warmup", 1));
  const int repeats = static_cast<int>(args.getInt("repeats", 3));
  if (settings.sample_rate <= 0 || settings.sample_rate > vital::kMaxSampleRate)
    return fail("--sample-rate out of range");
  if (settings.block_size <= 0 || settings.block_size > 65536)
    return fail("--block-size out of range");
  if (!(length_seconds > 0.0))
    return fail("--length must be positive");
  if (warmup < 0 || repeats < 1)
    return fail("--warmup must be >= 0 and --repeats >= 1");
  settings.total_samples = static_cast<long long>(std::ceil(length_seconds * settings.sample_rate));

  if (!args.has("keep-home"))
    harness::isolateUserEnvironment();

  ScopedJuceInitialiser_GUI juce_init;

  if (!no_midi) {
    std::string error;
    double last_event_seconds = 0.0;
    if (!harness::loadMidi(midi_path, settings.sample_rate, settings.events, last_event_seconds, error))
      return fail(error);
  }

  std::vector<Run> runs;
  for (int i = 0; i < warmup + repeats; ++i) {
    Run run;
    std::string error;
    if (!renderOnce(settings, run, error))
      return fail(error);
    if (i >= warmup)
      runs.push_back(std::move(run));
  }

  const Run& last = runs.back();
  double sum_squares = 0.0;
  float peak = 0.0f;
  size_t non_finite = 0;
  for (float sample : last.interleaved) {
    if (!std::isfinite(sample)) {
      ++non_finite;
      continue;
    }
    sum_squares += static_cast<double>(sample) * sample;
    peak = std::max(peak, std::fabs(sample));
  }

  if (args.has("out")) {
    std::string error;
    if (!harness::writeWav(args.get("out"), last.interleaved, 2, settings.sample_rate,
                           harness::WavFormat::kFloat32, error))
      return fail(error);
  }

  harness::json result;
  result["tool"] = "ref-bench";
  result["build"] = harness::buildInfo();
  result["preset"] = settings.use_init ? "<init>" : settings.preset_path;
  result["midi"] = no_midi ? "<none>" : midi_path;
  result["sample_rate"] = settings.sample_rate;
  result["block_size"] = settings.block_size;
  result["frames"] = settings.total_samples;
  result["blocks"] = last.block_seconds.size();
  result["audio_seconds"] = static_cast<double>(settings.total_samples) / settings.sample_rate;
  result["warmup"] = warmup;
  harness::json repeat_list = harness::json::array();
  std::vector<double> all_blocks;
  double first_sum = 0.0;
  for (const Run& run : runs) {
    repeat_list.push_back({ { "wall_seconds", run.wall_seconds }, { "cpu_seconds", run.cpu_seconds } });
    all_blocks.insert(all_blocks.end(), run.block_seconds.begin(), run.block_seconds.end());
    first_sum += run.block_seconds.empty() ? 0.0 : run.block_seconds.front();
  }
  result["repeats"] = repeat_list;
  double block_sum = 0.0;
  for (double value : all_blocks)
    block_sum += value;
  result["block_us"] = {
    { "mean", 1e6 * block_sum / all_blocks.size() },
    { "p50", 1e6 * percentile(all_blocks, 0.50) },
    { "p99", 1e6 * percentile(all_blocks, 0.99) },
    { "max", 1e6 * percentile(all_blocks, 1.0) },
    { "first", 1e6 * first_sum / runs.size() },
  };
  result["output"] = {
    { "rms", std::sqrt(sum_squares / std::max<size_t>(1, last.interleaved.size())) },
    { "peak", peak },
    { "non_finite", non_finite },
    { "samples_fnv1a64",
      harness::hex64(harness::fnv1a64(last.interleaved.data(), last.interleaved.size() * sizeof(float))) },
  };
  std::cout << result.dump(2) << std::endl;
  return non_finite ? 1 : 0;
}
