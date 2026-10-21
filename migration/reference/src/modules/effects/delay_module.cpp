// Module "delay_module": DelayModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "delay_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "delay_frequency", 2.0f },
    { "delay_sync", 1.0f },
    { "delay_tempo", 9.0f },
    { "delay_aux_frequency", 2.0f },
    { "delay_aux_sync", 1.0f },
    { "delay_aux_tempo", 9.0f },
    { "delay_feedback", 0.5f },
    { "delay_dry_wet", 0.3334f },
    { "delay_filter_cutoff", 60.0f },
    { "delay_filter_spread", 1.0f },
    { "delay_style", 0.0f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "delay_module",
  "DelayModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output* beats_per_second) { return new vital::DelayModule(beats_per_second); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
