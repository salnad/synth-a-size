// Module "reverb_module": ReverbModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "memory.h"
#include "reverb.h"
#include "reverb_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "reverb_decay_time", 0.0f },
    { "reverb_pre_low_cutoff", 0.0f },
    { "reverb_pre_high_cutoff", 110.0f },
    { "reverb_low_shelf_cutoff", 0.0f },
    { "reverb_low_shelf_gain", 0.0f },
    { "reverb_high_shelf_cutoff", 90.0f },
    { "reverb_high_shelf_gain", -1.0f },
    { "reverb_chorus_amount", 0.223607f },
    { "reverb_chorus_frequency", -2.0f },
    { "reverb_size", 0.5f },
    { "reverb_delay", 0.0f },
    { "reverb_dry_wet", 0.25f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "reverb_module",
  "ReverbModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output*) { return new vital::ReverbModule(); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
