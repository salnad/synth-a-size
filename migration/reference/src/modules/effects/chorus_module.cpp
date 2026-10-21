// Module "chorus_module": ChorusModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "chorus_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "chorus_voices", 4.0f },
    { "chorus_frequency", -3.0f },
    { "chorus_sync", 1.0f },
    { "chorus_tempo", 4.0f },
    { "chorus_feedback", 0.4f },
    { "chorus_dry_wet", 0.5f },
    { "chorus_cutoff", 60.0f },
    { "chorus_spread", 1.0f },
    { "chorus_mod_depth", 0.5f },
    { "chorus_delay_1", -9.0f },
    { "chorus_delay_2", -7.0f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "chorus_module",
  "ChorusModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output* beats_per_second) { return new vital::ChorusModule(beats_per_second); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
