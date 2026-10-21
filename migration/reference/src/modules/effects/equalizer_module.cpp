// Module "equalizer_module": EqualizerModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "memory.h"
#include "equalizer_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "eq_low_mode", 0.0f },
    { "eq_low_cutoff", 40.0f },
    { "eq_low_gain", 0.0f },
    { "eq_low_resonance", 0.3163f },
    { "eq_band_mode", 0.0f },
    { "eq_band_cutoff", 80.0f },
    { "eq_band_gain", 0.0f },
    { "eq_band_resonance", 0.4473f },
    { "eq_high_mode", 0.0f },
    { "eq_high_cutoff", 100.0f },
    { "eq_high_gain", 0.0f },
    { "eq_high_resonance", 0.3163f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "equalizer_module",
  "EqualizerModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output*) { return new vital::EqualizerModule(); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
