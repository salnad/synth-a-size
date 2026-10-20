// Module "filter": every SynthFilter model via SynthFilter::createFilter.
// Ports: port/crates/filters (core and character filter threads).

#include "module_registry.h"

#include "synth_constants.h"
#include "synth_filter.h"

#include <map>

namespace {

int filterModelFromName(const std::string& name) {
  static const std::map<std::string, int> kModels = {
    { "analog", vital::constants::kAnalog }, { "dirty", vital::constants::kDirty },
    { "ladder", vital::constants::kLadder }, { "digital", vital::constants::kDigital },
    { "diode", vital::constants::kDiode }, { "formant", vital::constants::kFormant },
    { "comb", vital::constants::kComb }, { "phase", vital::constants::kPhase },
  };
  auto found = kModels.find(name);
  return found == kModels.end() ? -1 : found->second;
}

using refmod::Rate;
using F = vital::SynthFilter;

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "filter",
  "SynthFilter::createFilter(model). params.model: analog|dirty|ladder|digital|diode|formant|comb|phase",
  [](const nlohmann::json& params) -> vital::Processor* {
    int model = filterModelFromName(params.value("model", std::string("digital")));
    if (model < 0)
      return nullptr;
    vital::SynthFilter* filter = vital::SynthFilter::createFilter(static_cast<vital::constants::FilterModel>(model));
    return dynamic_cast<vital::Processor*>(filter);
  },
  {
    { "audio", F::kAudio, 0.0f, Rate::kAudio },
    { "reset", F::kReset, 0.0f, Rate::kTrigger },
    { "midi_cutoff", F::kMidiCutoff, 60.0f, Rate::kAudio },
    { "resonance", F::kResonance, 0.5f, Rate::kControl },
    { "drive_gain", F::kDriveGain, 0.0f, Rate::kControl },
    { "gain", F::kGain, 0.0f, Rate::kControl },
    { "style", F::kStyle, 0.0f, Rate::kControl },
    { "pass_blend", F::kPassBlend, 0.0f, Rate::kControl },
    { "interpolate_x", F::kInterpolateX, 0.5f, Rate::kControl },
    { "interpolate_y", F::kInterpolateY, 0.5f, Rate::kControl },
    { "transpose", F::kTranspose, 0.0f, Rate::kControl },
    { "spread", F::kSpread, 0.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
