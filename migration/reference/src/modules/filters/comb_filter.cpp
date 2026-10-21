// Module "comb_filter": CombFilter(size). Ports: port/crates/filters
// (src/character/comb_filter.rs).
//
// params.size: memory size in samples (default 25000, CombModule's
// kMaxFeedbackSamples). SynthFilter::createFilter's CombFilter(1) is not
// captured: its 1-sample memory makes Memory::get read past the buffer.

#include "module_registry.h"

#include "comb_filter.h"
#include "comb_module.h"

using refmod::Rate;
using F = vital::CombFilter;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "comb_filter",
  "CombFilter(size). params.size: int (default CombModule::kMaxFeedbackSamples)",
  [](const nlohmann::json& params) -> vital::Processor* {
    int default_size = vital::CombModule::kMaxFeedbackSamples;
    int size = params.value("size", default_size);
    if (size < 2)
      return nullptr;
    return new vital::CombFilter(size);
  },
  {
    { "audio", F::kAudio, 0.0f, Rate::kAudio },
    { "reset", F::kReset, 0.0f, Rate::kTrigger },
    { "midi_cutoff", F::kMidiCutoff, 60.0f, Rate::kAudio },
    { "resonance", F::kResonance, 0.5f, Rate::kControl },
    { "drive_gain", F::kDriveGain, 0.0f, Rate::kControl },
    { "gain", F::kGain, 0.0f, Rate::kControl },
    { "style", F::kStyle, 0.0f, Rate::kControl },
    { "pass_blend", F::kPassBlend, 1.0f, Rate::kControl },
    { "interpolate_x", F::kInterpolateX, 0.5f, Rate::kControl },
    { "interpolate_y", F::kInterpolateY, 0.5f, Rate::kControl },
    { "transpose", F::kTranspose, 42.0f, Rate::kControl },
    { "spread", F::kSpread, 0.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
