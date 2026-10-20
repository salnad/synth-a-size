// Modules "preset_registry" and "preset_state": artifact-only captures for the
// M2 preset port (port/crates/preset). preset_registry dumps the parameter
// registry and the engine tables the preset loader consults; preset_state
// dumps the state SynthBase saves after loading a corpus preset (or the init
// preset), optionally after a second load -> save round trip.

#include "module_registry.h"

#include "JuceHeader.h"
#include "harness_io.h"
#include "load_save.h"
#include "modulation_connection_processor.h"
#include "sound_engine.h"
#include "synth_base.h"
#include "synth_parameters.h"
#include "synth_strings.h"
#include "utils.h"

using nlohmann::json;

namespace {

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

// Loads `path` (or the init preset) on a newly constructed synth, seeded like
// a fresh process, and returns its saved state, or false with upstream's
// error message.
bool loadState(int seed, const std::string& path, json& state, std::string& message) {
  vital::utils::RandomGenerator::next_seed_ = seed;
  std::unique_ptr<PresetSynth> synth = std::make_unique<PresetSynth>();
  if (path.empty()) {
    synth->loadInitPreset();
  }
  else if (!synth->loadFromFile(File(path), message)) {
    return false;
  }
  state = synth->state();
  return true;
}

bool presetState(const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
  const json params = context.spec.value("params", json::object());
  const int seed = params.value("seed", 0);
  const bool use_init = params.value("init", false);
  std::string path;
  std::string stem = "init";
  if (!use_init) {
    if (!params.count("preset")) {
      error = "params.preset (corpus-relative path) or params.init is required";
      return false;
    }
    path = context.corpusPath(params["preset"].get<std::string>());
    if (!File(path).existsAsFile()) {
      error = "preset not found: " + path;
      return false;
    }
    stem = File(path).getFileNameWithoutExtension().toStdString();
  }

  json state;
  std::string message;
  if (!loadState(seed, path, state, message)) {
    sink.text("load_error.txt", message + "\n");
    return true;
  }
  const std::string saved = state.dump();
  sink.text("state.json", saved + "\n");

  if (params.value("roundtrip", false)) {
    // Reload from a file named like the original: loadFromFile names the
    // preset after its file.
    TemporaryFile directory;
    File folder = directory.getFile();
    folder.createDirectory();
    File copy = folder.getChildFile(stem + ".vital");
    copy.replaceWithText(saved);
    json reloaded;
    bool ok = loadState(seed, copy.getFullPathName().toStdString(), reloaded, message);
    folder.deleteRecursively();
    if (!ok) {
      error = "reloading the saved state failed: " + message;
      return false;
    }
    sink.text("roundtrip.json", reloaded.dump() + "\n");
  }
  return true;
}

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "preset_registry",
  "Parameter registry (Parameters::getDetails order), string tables and engine tables (controls, "
  "modulation sources and destinations) as artifact registry.json. params.seed: RandomGenerator seed "
  "before the synth is constructed (default 0)",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const json params = context.spec.value("params", json::object());
    sink.text("registry.json", registry(params.value("seed", 0)).dump() + "\n");
    return true;
  },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "preset_state",
  "SynthBase::loadFromFile(params.preset) or loadInitPreset (params.init) on a fresh synth, then "
  "saveToJson().dump() as artifact state.json (load_error.txt if the load fails); params.roundtrip "
  "adds roundtrip.json, the state after saving and loading again; params.seed (default 0)",
  nullptr,
  {},
  {},
  presetState,
});
