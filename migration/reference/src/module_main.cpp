// ref-module: drive a single upstream engine Processor with described input
// signals and dump every input and output buffer as .npy, for module-level
// differential tests. See migration/reference/README.md for the spec format.

#include "JuceHeader.h"
#include "harness_io.h"
#include "module_registry.h"

#include "processor.h"
#include "synth_constants.h"
#include "utils.h"

#include <algorithm>
#include <cctype>
#include <array>

#include <cmath>
#include <functional>
#include <iostream>
#include <random>

using harness::json;

namespace refsig {
  float ramp(float from, float to, long long frame, long long total_frames);
  float periodic(int kind, long long frame, int sample_rate, double freq, double phase);
}

namespace {

const char* kUsage = R"(usage: ref-module <spec.json> --out-dir DIR
       ref-module --list

Instantiates one upstream Processor, feeds it the input signals described in
spec.json block by block, and writes:
  DIR/in_<input>.npy    float32 [frames, 4]   (poly lanes: v0L v0R v1L v1R)
  DIR/out_<output>.npy  float32 [frames, 4]
  DIR/artifact_<name>   exact-match text artifacts (modules with an artifact hook)
  DIR/capture.json      resolved spec, build info, per-buffer and per-artifact hashes

Options:
  --corpus-dir DIR      migration/corpus, for artifact hooks that read corpus files
)";

using refmod::InputDef;
using refmod::ModuleDef;
using refmod::OutputDef;
using refmod::Rate;

// Generates one input signal, frame by frame, into 4 poly lanes.
class Signal {
  public:
    bool init(const json& spec, float default_value, int sample_rate, std::string& error) {
      constant_.assign(4, default_value);
      if (spec.is_null())
        return true;

      if (spec.is_number()) {
        constant_.assign(4, spec.get<float>());
        return true;
      }

      if (!spec.is_object()) {
        error = "input spec must be a number or an object";
        return false;
      }

      sample_rate_ = sample_rate;
      if (spec.count("value")) {
        const json& value = spec["value"];
        if (value.is_array()) {
          if (value.size() != 4) {
            error = "'value' arrays must have 4 lanes";
            return false;
          }
          for (int i = 0; i < 4; ++i)
            constant_[i] = value[i].get<float>();
        }
        else {
          constant_.assign(4, value.get<float>());
        }
        return true;
      }

      if (spec.count("npy")) {
        std::vector<size_t> shape;
        if (!harness::readNpy(spec["npy"].get<std::string>(), npy_data_, shape, error))
          return false;
        npy_frames_ = shape.empty() ? 0 : shape[0];
        npy_channels_ = shape.size() > 1 ? shape[1] : 1;
        if (shape.size() > 2 || (npy_channels_ != 1 && npy_channels_ != 2 && npy_channels_ != 4)) {
          error = "npy inputs must have shape [N], [N,1], [N,2] or [N,4]";
          return false;
        }
        type_ = "npy";
        return true;
      }

      if (spec.count("trigger")) {
        type_ = "trigger";
        for (const json& event : spec["trigger"]) {
          TriggerEvent trigger;
          trigger.sample = event.value("sample", 0LL);
          trigger.value = vital::kVoiceOn;
          if (event.count("value")) {
            const json& value = event["value"];
            if (value.is_string()) {
              std::string name = value.get<std::string>();
              if (name == "voice_on") trigger.value = vital::kVoiceOn;
              else if (name == "voice_off") trigger.value = vital::kVoiceOff;
              else if (name == "voice_kill") trigger.value = vital::kVoiceKill;
              else if (name == "voice_idle") trigger.value = vital::kVoiceIdle;
              else if (name == "voice_hold") trigger.value = vital::kVoiceHold;
              else if (name == "voice_decay") trigger.value = vital::kVoiceDecay;
              else {
                error = "unknown trigger value " + name;
                return false;
              }
            }
            else {
              trigger.value = value.get<float>();
            }
          }
          std::string lanes = event.value("voices", std::string("all"));
          if (lanes == "all") trigger.mask = { -1, -1, -1, -1 };
          else if (lanes == "0") trigger.mask = { -1, -1, 0, 0 };
          else if (lanes == "1") trigger.mask = { 0, 0, -1, -1 };
          else {
            error = "trigger 'voices' must be all, 0 or 1";
            return false;
          }
          triggers_.push_back(trigger);
        }
        return true;
      }

      if (!spec.count("signal")) {
        error = "input object needs one of: value, signal, npy, trigger";
        return false;
      }

      type_ = spec["signal"].get<std::string>();
      amp_ = spec.value("amp", 1.0f);
      offset_ = spec.value("offset", 0.0f);
      freq_ = spec.value("freq_hz", 440.0);
      phase_ = spec.value("phase", 0.0);
      from_ = spec.value("from", 0.0f);
      to_ = spec.value("to", 1.0f);
      impulse_at_ = spec.value("at", 0LL);
      stereo_ = spec.value("stereo", false);
      noise_engine_.seed(spec.value("seed", 1u));
      if (type_ != "sine" && type_ != "saw" && type_ != "square" && type_ != "noise" &&
          type_ != "impulse" && type_ != "ramp" && type_ != "silence") {
        error = "unknown signal type " + type_;
        return false;
      }
      return true;
    }

    void setTotalFrames(long long frames) { total_frames_ = frames; }

    // Value of each lane at frame index `frame` (called in increasing order).
    void frame(long long frame, float* lanes) {
      if (type_.empty() || type_ == "trigger") {
        for (int i = 0; i < 4; ++i)
          lanes[i] = constant_[i];
        return;
      }

      if (type_ == "npy") {
        for (int i = 0; i < 4; ++i)
          lanes[i] = 0.0f;
        if (frame < static_cast<long long>(npy_frames_)) {
          const float* row = npy_data_.data() + frame * npy_channels_;
          for (int i = 0; i < 4; ++i) {
            size_t channel = npy_channels_ == 1 ? 0 : (npy_channels_ == 2 ? (i % 2) : i);
            lanes[i] = row[channel];
          }
        }
        return;
      }

      float value = 0.0f;
      if (type_ == "sine")
        value = refsig::periodic(0, frame, sample_rate_, freq_, phase_);
      else if (type_ == "saw")
        value = refsig::periodic(1, frame, sample_rate_, freq_, phase_);
      else if (type_ == "square")
        value = refsig::periodic(2, frame, sample_rate_, freq_, phase_);
      else if (type_ == "impulse")
        value = frame == impulse_at_ ? 1.0f : 0.0f;
      else if (type_ == "ramp")
        value = refsig::ramp(from_, to_, frame, total_frames_);
      else if (type_ == "noise") {
        std::uniform_real_distribution<float> distribution(-1.0f, 1.0f);
        value = distribution(noise_engine_);
        if (stereo_) {
          float right = distribution(noise_engine_);
          lanes[0] = lanes[2] = offset_ + amp_ * value;
          lanes[1] = lanes[3] = offset_ + amp_ * right;
          return;
        }
      }

      if (type_ == "ramp") {
        for (int i = 0; i < 4; ++i)
          lanes[i] = value;
        return;
      }
      for (int i = 0; i < 4; ++i)
        lanes[i] = offset_ + amp_ * value;
    }

    bool isTrigger() const { return type_ == "trigger"; }

    // Applies trigger events that fall inside [block_start, block_start + n).
    void applyTriggers(vital::Output* output, long long block_start, int n) {
      output->clearTrigger();
      for (const TriggerEvent& event : triggers_) {
        if (event.sample >= block_start && event.sample < block_start + n) {
          vital::poly_mask mask(event.mask[0], event.mask[1], event.mask[2], event.mask[3]);
          output->trigger(mask, event.value, static_cast<int>(event.sample - block_start));
        }
      }
    }

  private:
    struct TriggerEvent {
      long long sample;
      float value;
      std::array<int, 4> mask;
    };

    std::string type_;
    std::vector<float> constant_;
    std::vector<float> npy_data_;
    size_t npy_frames_ = 0;
    size_t npy_channels_ = 1;
    std::vector<TriggerEvent> triggers_;
    int sample_rate_ = 44100;
    long long total_frames_ = 0;
    float amp_ = 1.0f;
    float offset_ = 0.0f;
    double freq_ = 440.0;
    double phase_ = 0.0;
    float from_ = 0.0f;
    float to_ = 1.0f;
    long long impulse_at_ = 0;
    bool stereo_ = false;
    std::mt19937 noise_engine_;
};

// __FILE__ is absolute; show it relative to the repository.
std::string relativeSource(const std::string& path) {
  size_t found = path.find("migration/reference/");
  return found == std::string::npos ? path : path.substr(found);
}

int fail(const std::string& message) {
  std::cerr << "ref-module: " << message << std::endl;
  return 2;
}

} // namespace

int main(int argc, const char* argv[]) {
  harness::pinLibmVariants(argc, argv);
  harness::Args args(argc, argv, { "list", "help" });
  if (args.has("help") || argc == 1) {
    std::cout << kUsage;
    return args.has("help") ? 0 : 2;
  }

  std::vector<ModuleDef> modules = refmod::registry();
  std::sort(modules.begin(), modules.end(),
            [](const ModuleDef& a, const ModuleDef& b) { return a.name < b.name; });
  for (size_t i = 1; i < modules.size(); ++i) {
    if (modules[i].name == modules[i - 1].name)
      return fail("module '" + modules[i].name + "' is registered twice: " + relativeSource(modules[i - 1].source_file) +
                  " and " + relativeSource(modules[i].source_file));
  }
  if (args.has("list")) {
    json listing = json::array();
    for (const ModuleDef& module : modules) {
      json entry;
      entry["module"] = module.name;
      entry["description"] = module.description;
      entry["source"] = relativeSource(module.source_file);
      entry["processor"] = static_cast<bool>(module.create);
      entry["artifacts"] = static_cast<bool>(module.artifacts);
      for (const InputDef& input : module.inputs) {
        const char* rate = input.rate == Rate::kAudio ? "audio" : (input.rate == Rate::kControl ? "control" : "trigger");
        entry["inputs"].push_back({ { "name", input.name }, { "default", input.default_value }, { "rate", rate } });
      }
      for (const OutputDef& output : module.outputs)
        entry["outputs"].push_back(output.name);
      listing.push_back(entry);
    }
    std::cout << listing.dump(2) << std::endl;
    return 0;
  }

  if (args.positional().empty())
    return fail("a spec.json path is required");
  std::string out_dir = args.get("out-dir");
  if (out_dir.empty())
    return fail("--out-dir is required");

  std::string spec_text;
  if (!harness::readTextFile(args.positional()[0], spec_text))
    return fail("cannot read " + args.positional()[0]);

  json spec;
  try {
    spec = json::parse(spec_text);
  }
  catch (const json::exception& e) {
    return fail(std::string("invalid spec JSON: ") + e.what());
  }

  std::string module_name = spec.value("module", std::string());
  const ModuleDef* module = nullptr;
  for (const ModuleDef& candidate : modules) {
    if (candidate.name == module_name)
      module = &candidate;
  }
  if (module == nullptr)
    return fail("unknown module '" + module_name + "' (see --list)");

  const bool has_processor = static_cast<bool>(module->create);
  if (!has_processor && !module->artifacts)
    return fail("module '" + module_name + "' has neither a processor nor an artifact hook");
  if (!has_processor && (!module->inputs.empty() || !module->outputs.empty()))
    return fail("artifact-only module '" + module_name + "' cannot declare processor inputs or outputs");

  const int sample_rate = spec.value("sample_rate", 44100);
  const int oversample = spec.value("oversample", 1);
  const int block_size = spec.value("block_size", 64);
  const long long num_samples = spec.value("num_samples", has_processor ? 44100LL : 0LL);
  const json params = spec.value("params", json::object());
  const json inputs_spec = spec.value("inputs", json::object());

  if (oversample < 1 || oversample > 16)
    return fail("oversample must be in [1, 16]");
  if (block_size < 1 || block_size > vital::kMaxBufferSize * oversample)
    return fail("block_size must be in [1, " + std::to_string(vital::kMaxBufferSize * oversample) + "]");
  if (has_processor && num_samples <= 0)
    return fail("num_samples must be positive");

  for (auto it = inputs_spec.begin(); it != inputs_spec.end(); ++it) {
    bool known = false;
    for (const InputDef& input : module->inputs)
      known = known || input.name == it.key();
    if (!known)
      return fail("module '" + module_name + "' has no input '" + it.key() + "'");
  }

  harness::isolateUserEnvironment();
  ScopedJuceInitialiser_GUI juce_init;
  if (spec.count("seed"))
    vital::utils::RandomGenerator::next_seed_ = spec["seed"].get<int>();

  std::unique_ptr<vital::Processor> processor;
  if (has_processor) {
    processor.reset(module->create(params));
    if (!processor)
      return fail("could not create module with params " + params.dump());
  }

  const int internal_rate = sample_rate * oversample;
  std::vector<std::unique_ptr<vital::Output>> sources;
  std::vector<Signal> signals(module->inputs.size());
  std::vector<std::vector<float>> input_capture(module->inputs.size());

  for (size_t i = 0; i < module->inputs.size(); ++i) {
    const InputDef& input = module->inputs[i];
    std::string error;
    json input_spec = inputs_spec.count(input.name) ? inputs_spec[input.name] : json();
    if (!signals[i].init(input_spec, input.default_value, internal_rate, error))
      return fail("input '" + input.name + "': " + error);
    signals[i].setTotalFrames(num_samples);

    Rate rate = input.rate;
    if (input_spec.is_object() && input_spec.count("rate")) {
      std::string requested = input_spec["rate"].get<std::string>();
      if (requested == "audio") rate = Rate::kAudio;
      else if (requested == "control") rate = Rate::kControl;
      else return fail("input '" + input.name + "': rate must be audio or control");
    }
    if (signals[i].isTrigger())
      rate = Rate::kTrigger;

    if (rate == Rate::kAudio)
      sources.push_back(std::make_unique<vital::Output>(vital::kMaxBufferSize, oversample));
    else
      sources.push_back(std::make_unique<vital::cr::Output>());

    processor->plug(sources.back().get(), input.index);
    input_capture[i].reserve(static_cast<size_t>(num_samples) * 4);
  }

  std::vector<std::vector<float>> output_capture(module->outputs.size());
  if (processor) {
    processor->setSampleRate(sample_rate);
    processor->setOversampleAmount(oversample);
    processor->hardReset();
    for (auto& capture : output_capture)
      capture.reserve(static_cast<size_t>(num_samples) * 4);
  }

  for (long long block_start = 0; processor && block_start < num_samples; block_start += block_size) {
    int n = static_cast<int>(std::min<long long>(block_size, num_samples - block_start));

    for (size_t i = 0; i < module->inputs.size(); ++i) {
      vital::Output* source = sources[i].get();
      float lanes[4];
      if (source->isControlRate()) {
        signals[i].frame(block_start, lanes);
        source->buffer[0] = vital::poly_float(lanes[0], lanes[1], lanes[2], lanes[3]);
        if (signals[i].isTrigger())
          signals[i].applyTriggers(source, block_start, n);
        for (int s = 0; s < n; ++s) {
          if (s > 0)
            signals[i].frame(block_start + s, lanes);
          input_capture[i].insert(input_capture[i].end(), lanes, lanes + 4);
        }
      }
      else {
        source->clearTrigger();
        for (int s = 0; s < n; ++s) {
          signals[i].frame(block_start + s, lanes);
          source->buffer[s] = vital::poly_float(lanes[0], lanes[1], lanes[2], lanes[3]);
          input_capture[i].insert(input_capture[i].end(), lanes, lanes + 4);
        }
      }
    }

    processor->process(n);

    for (size_t o = 0; o < module->outputs.size(); ++o) {
      const vital::Output* output = processor->output(module->outputs[o].index);
      for (int s = 0; s < n; ++s) {
        const vital::poly_float& value = output->isControlRate() ? output->buffer[0] : output->buffer[s];
        for (int lane = 0; lane < 4; ++lane)
          output_capture[o].push_back(value[lane]);
      }
    }
  }

  refmod::ArtifactSink sink;
  if (module->artifacts) {
    refmod::ArtifactContext context{ spec, args.get("corpus-dir") };
    std::string error;
    if (!module->artifacts(context, sink, error))
      return fail("artifact hook of '" + module_name + "' failed: " + error);
  }

  if (!harness::ensureDirectory(out_dir))
    return fail("cannot create " + out_dir);

  json manifest;
  manifest["tool"] = "ref-module";
  manifest["build"] = harness::buildInfo();
  manifest["spec"] = spec;
  manifest["layout"] = "float32 [frames, 4]; lanes = voice0 L, voice0 R, voice1 L, voice1 R";
  manifest["internal_sample_rate"] = internal_rate;
  manifest["frames"] = num_samples;

  size_t non_finite = 0;
  auto dump = [&](const std::string& file_name, const std::vector<float>& data, const char* kind,
                  const std::string& name, size_t channels = 4) -> bool {
    std::string error;
    std::string path = out_dir + "/" + file_name;
    if (channels == 0 || data.size() % channels != 0) {
      std::cerr << "ref-module: " << name << ": " << data.size() << " values do not divide into "
                << channels << " channels" << std::endl;
      return false;
    }
    if (!harness::writeNpyF32(path, data.data(), { data.size() / channels, channels }, error)) {
      std::cerr << "ref-module: " << error << std::endl;
      return false;
    }
    for (float value : data)
      non_finite += std::isfinite(value) ? 0 : 1;
    manifest[kind][name] = {
      { "file", file_name },
      { "fnv1a64", harness::hex64(harness::fnv1a64(data.data(), data.size() * sizeof(float))) },
    };
    return true;
  };

  for (size_t i = 0; i < module->inputs.size(); ++i) {
    if (!dump("in_" + module->inputs[i].name + ".npy", input_capture[i], "inputs", module->inputs[i].name))
      return 2;
  }
  for (size_t o = 0; o < module->outputs.size(); ++o) {
    const vital::Output* output = processor->output(module->outputs[o].index);
    if (!dump("out_" + module->outputs[o].name + ".npy", output_capture[o], "outputs", module->outputs[o].name))
      return 2;
    manifest["outputs"][module->outputs[o].name]["rate"] = output->isControlRate() ? "control" : "audio";
  }

  auto valid_name = [](const std::string& name) {
    if (name.empty() || name[0] == '.')
      return false;
    for (char c : name) {
      if (!(std::isalnum(static_cast<unsigned char>(c)) || c == '_' || c == '-' || c == '.'))
        return false;
    }
    return true;
  };
  for (const auto& array : sink.arrays()) {
    if (!valid_name(array.name) || manifest["outputs"].count(array.name))
      return fail("invalid or duplicate artifact array name '" + array.name + "'");
    if (!dump("out_" + array.name + ".npy", array.data, "outputs", array.name, array.channels))
      return 2;
    manifest["outputs"][array.name]["rate"] = "artifact";
    manifest["outputs"][array.name]["channels"] = array.channels;
  }
  for (const auto& text : sink.texts()) {
    if (!valid_name(text.name) || manifest["artifacts"].count(text.name))
      return fail("invalid or duplicate artifact name '" + text.name + "'");
    std::string file_name = "artifact_" + text.name;
    if (!harness::writeTextFile(out_dir + "/" + file_name, text.content))
      return fail("cannot write " + file_name);
    manifest["artifacts"][text.name] = {
      { "file", file_name },
      { "bytes", text.content.size() },
      { "fnv1a64", harness::hex64(harness::fnv1a64(text.content.data(), text.content.size())) },
    };
  }
  manifest["non_finite_samples"] = non_finite;

  if (!harness::writeTextFile(out_dir + "/capture.json", manifest.dump(2) + "\n"))
    return fail("cannot write capture.json");

  std::cout << out_dir << ": " << module_name << ", " << num_samples << " frames" << std::endl;
  return non_finite ? 2 : 0;
}
