// Module "distortion_module": DistortionModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "distortion_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "distortion_type", 0.0f },
    { "distortion_drive", 0.0f },
    { "distortion_mix", 1.0f },
    { "distortion_filter_order", 0.0f },
    { "distortion_filter_cutoff", 80.0f },
    { "distortion_filter_resonance", 0.5f },
    { "distortion_filter_blend", 0.0f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "distortion_module",
  "DistortionModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output*) { return new vital::DistortionModule(); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
