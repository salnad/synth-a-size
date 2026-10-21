// Module "flanger_module": FlangerModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "flanger_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "flanger_frequency", 2.0f },
    { "flanger_sync", 1.0f },
    { "flanger_tempo", 4.0f },
    { "flanger_center", 64.0f },
    { "flanger_feedback", 0.5f },
    { "flanger_dry_wet", 0.5f },
    { "flanger_mod_depth", 0.5f },
    { "flanger_phase_offset", 0.33333333f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "flanger_module",
  "FlangerModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output* beats_per_second) { return new vital::FlangerModule(beats_per_second); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
