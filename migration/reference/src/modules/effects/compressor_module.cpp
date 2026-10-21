// Module "compressor_module": CompressorModule driven like the engine drives
// it (see effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

#include "compressor.h"
#include "compressor_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "compressor_attack", 0.5f },
    { "compressor_release", 0.5f },
    { "compressor_low_gain", 16.3f },
    { "compressor_band_gain", 11.7f },
    { "compressor_high_gain", 16.3f },
    { "compressor_enabled_bands", 0.0f },
    { "compressor_low_upper_ratio", 0.9f },
    { "compressor_band_upper_ratio", 0.857f },
    { "compressor_high_upper_ratio", 1.0f },
    { "compressor_low_lower_ratio", 0.8f },
    { "compressor_band_lower_ratio", 0.8f },
    { "compressor_high_lower_ratio", 0.8f },
    { "compressor_low_upper_threshold", -28.0f },
    { "compressor_band_upper_threshold", -25.0f },
    { "compressor_high_upper_threshold", -30.0f },
    { "compressor_low_lower_threshold", -35.0f },
    { "compressor_band_lower_threshold", -36.0f },
    { "compressor_high_lower_threshold", -35.0f },
    { "compressor_mix", 1.0f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "compressor_module",
  "CompressorModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output*) { return new vital::CompressorModule(); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
