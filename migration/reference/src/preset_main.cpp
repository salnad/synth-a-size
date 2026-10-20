// ref-preset: dumps the upstream parameter registry, the engine tables the
// preset loader consults (controls, modulation sources and destinations) and
// the preset state after a load, for the M2 preset round-trip tests. See
// migration/reference/README.md ("ref-preset").

#include "JuceHeader.h"
#include "harness_io.h"
#include "load_save.h"
#include "modulation_connection_processor.h"
#include "sound_engine.h"
#include "synth_base.h"
#include "synth_parameters.h"
#include "synth_strings.h"
#include "utils.h"

#include <iostream>

using harness::json;

namespace {

const char* kUsage = R"(usage: ref-preset registry [-o out.json] [--seed N]
       ref-preset state (<preset.vital> | --init) [-o out.json] [--seed N]

registry  Every ValueDetails entry in Parameters::getDetails(index) order, the
          string tables they reference, the engine's control names, modulation
          sources, mono/poly modulation destinations, the destinations each
          modulation slot may not target, and the RandomGenerator seed the init
          preset's sample uses for the given --seed.
state     SynthBase::loadFromFile (or loadInitPreset for --init) on a freshly
          constructed synth, then SynthBase::saveToJson(), compact. A failed
          load writes {"load_error": "<message>"} and exits 3.

--seed N  set RandomGenerator::next_seed_ before the synth is constructed
          (default 0).
)";

class PresetSynth : public HeadlessSynth {
  public:
    vital::SoundEngine* engine() { return engine_.get(); }
    json state() { return saveToJson(); }
};

struct StringTable {
  const char* name;
  const std::string* values;
  size_t size;
};

#define TABLE(table) { #table, strings::table, sizeof(strings::table) / sizeof(std::string) }

const StringTable kStringTables[] = {
  TABLE(kOffOnNames),
  TABLE(kDelayStyleNames),
  TABLE(kFrequencySyncNames),
  TABLE(kSyncedFrequencyNames),
  TABLE(kDistortionTypeNames),
  TABLE(kDistortionFilterOrderNames),
  TABLE(kStereoModeNames),
  TABLE(kDestinationNames),
  TABLE(kCompressorBandNames),
  TABLE(kEqLowModeNames),
  TABLE(kEqBandModeNames),
  TABLE(kEqHighModeNames),
  TABLE(kVoicePriorityNames),
  TABLE(kVoiceOverrideNames),
  TABLE(kOversamplingNames),
  TABLE(kSyncNames),
  TABLE(kRandomNames),
  TABLE(kFilterStyleNames),
  TABLE(kFilterModelNames),
  TABLE(kUnisonStackNames),
  TABLE(kPhaseDistortionNames),
  TABLE(kSpectralMorphNames),
};

#undef TABLE

const char* kScaleNames[] = { "indexed", "linear", "quadratic", "cubic", "quartic", "square_root", "exponential" };

int fail(const std::string& message) {
  std::cerr << "ref-preset: " << message << std::endl;
  return 2;
}

bool emit(const harness::Args& args, const json& data) {
  std::string text = data.dump() + "\n";
  if (!args.has("out")) {
    std::cout << text;
    return true;
  }
  return harness::writeTextFile(args.get("out"), text);
}

// The tables in synth_strings.h have internal linkage, so the registry's
// pointers address another translation unit's copies. Match by contents,
// reading index k only while every remaining candidate (one of which is the
// real table) has more than k entries.
json stringTableName(const std::string* lookup) {
  if (lookup == nullptr)
    return nullptr;

  std::vector<const StringTable*> candidates;
  for (const StringTable& table : kStringTables)
    candidates.push_back(&table);

  for (size_t k = 0; candidates.size() > 1 || k == 0; ++k) {
    for (const StringTable* candidate : candidates) {
      if (candidate->size <= k)
        return "<ambiguous>";
    }
    std::vector<const StringTable*> remaining;
    for (const StringTable* candidate : candidates) {
      if (candidate->values[k] == lookup[k])
        remaining.push_back(candidate);
    }
    candidates = remaining;
    if (candidates.empty())
      return "<unknown>";
  }
  return candidates[0]->name;
}

json registry(int seed) {
  json data;

  json parameters = json::array();
  for (int i = 0; i < vital::Parameters::getNumParameters(); ++i) {
    const vital::ValueDetails* details = vital::Parameters::getDetails(i);
    const vital::ValueDetails& by_name = vital::Parameters::getDetails(details->name);
    json entry;
    entry["name"] = details->name;
    entry["version_added"] = details->version_added;
    entry["min"] = details->min;
    entry["max"] = details->max;
    // getDetails(name) is what the loader uses for defaults; it can differ from
    // the list entry (see the osc/filter default overrides in synth_parameters.cpp).
    entry["default_value"] = by_name.default_value;
    entry["post_offset"] = details->post_offset;
    entry["display_multiply"] = details->display_multiply;
    entry["value_scale"] = kScaleNames[details->value_scale];
    entry["display_invert"] = details->display_invert;
    entry["display_units"] = details->display_units;
    entry["display_name"] = details->display_name;
    entry["local_description"] = details->local_description;
    entry["string_table"] = stringTableName(details->string_lookup);
    parameters.push_back(entry);
  }
  data["parameters"] = parameters;

  json tables;
  for (const StringTable& table : kStringTables) {
    json values = json::array();
    for (size_t i = 0; i < table.size; ++i)
      values.push_back(table.values[i]);
    tables[table.name] = values;
  }
  data["string_tables"] = tables;

  vital::utils::RandomGenerator::next_seed_ = seed;
  std::unique_ptr<PresetSynth> synth = std::make_unique<PresetSynth>();
  vital::SoundEngine* engine = synth->engine();

  json controls = json::array();
  for (auto& control : engine->getControls())
    controls.push_back(control.first);
  data["controls"] = controls;

  json sources = json::array();
  for (auto& source : engine->getModulationSources())
    sources.push_back(source.first);
  data["modulation_sources"] = sources;

  json mono_destinations = json::array();
  for (auto& destination : engine->getMonoModulationDestinations())
    mono_destinations.push_back(destination.first);
  data["mono_modulation_destinations"] = mono_destinations;

  json poly_destinations = json::array();
  for (auto& destination : engine->getPolyModulationDestinations())
    poly_destinations.push_back(destination.first);
  data["poly_modulation_destinations"] = poly_destinations;

  // SynthBase::isInvalidConnection: a slot may not modulate a destination
  // whose poly processor lives in that slot's own ModulationConnectionProcessor.
  vital::ModulationConnectionBank& bank = synth->getModulationBank();
  json self_destinations = json::array();
  for (int i = 0; i < vital::kMaxModulationConnections; ++i) {
    json names = json::array();
    vital::Processor* processor = bank.atIndex(i)->modulation_processor.get();
    for (auto& destination : engine->getPolyModulationDestinations()) {
      if (destination.second->router() == processor)
        names.push_back(destination.first);
    }
    self_destinations.push_back(names);
  }
  data["slot_self_destinations"] = self_destinations;

  int before = vital::utils::RandomGenerator::next_seed_;
  synth->loadInitPreset();
  int after = vital::utils::RandomGenerator::next_seed_;
  json init;
  init["seed"] = seed;
  init["random_generators_constructed"] = after - before;
  init["sample_seed"] = before;
  data["init"] = init;

  data["synth_version"] = ProjectInfo::versionString;
  data["num_parameters"] = vital::Parameters::getNumParameters();
  return data;
}

} // namespace

int main(int argc, const char* argv[]) {
  harness::pinLibmVariants(argc, argv);
  harness::Args args(argc, argv, { "init", "keep-home", "help" });
  std::vector<std::string> positional = args.positional();
  if (args.has("help") || positional.empty()) {
    std::cout << kUsage;
    return args.has("help") ? 0 : 2;
  }
  if (!args.unknown().empty())
    return fail("missing value for " + args.unknown()[0]);
  if (!args.has("keep-home"))
    harness::isolateUserEnvironment();

  ScopedJuceInitialiser_GUI juce_init;
  const int seed = static_cast<int>(args.getInt("seed", 0));
  const std::string command = positional[0];

  if (command == "registry") {
    if (!emit(args, registry(seed)))
      return fail("cannot write output");
    return 0;
  }

  if (command != "state")
    return fail("unknown command " + command);

  bool use_init = args.has("init");
  std::string preset_path = positional.size() > 1 ? positional[1] : "";
  if (preset_path.empty() == !use_init)
    return fail("state needs exactly one of <preset.vital> or --init");

  vital::utils::RandomGenerator::next_seed_ = seed;
  std::unique_ptr<PresetSynth> synth = std::make_unique<PresetSynth>();

  if (use_init) {
    synth->loadInitPreset();
  }
  else {
    File preset = File::getCurrentWorkingDirectory().getChildFile(preset_path);
    if (!preset.existsAsFile())
      return fail("preset not found: " + preset_path);
    std::string error;
    if (!synth->loadFromFile(preset, error)) {
      json failure;
      failure["load_error"] = error;
      if (!emit(args, failure))
        return fail("cannot write output");
      return 3;
    }
  }

  if (!emit(args, synth->state()))
    return fail("cannot write output");
  return 0;
}
