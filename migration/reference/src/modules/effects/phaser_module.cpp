// Module "phaser_module": PhaserModule driven like the engine drives it (see
// effect_module_harness.h). Ports: port/crates/effects.

#include "effect_module_harness.h"

// phaser_filter.h uses futils without including it (upstream includes it first).
#include "futils.h"
#include "phaser.h"
#include "phaser_module.h"

namespace {
  const std::vector<std::pair<std::string, float>> kControls = {
    { "phaser_frequency", -3.0f },
    { "phaser_sync", 1.0f },
    { "phaser_tempo", 3.0f },
    { "phaser_feedback", 0.5f },
    { "phaser_dry_wet", 1.0f },
    { "phaser_center", 80.0f },
    { "phaser_mod_depth", 24.0f },
    { "phaser_phase_offset", 0.33333333f },
    { "phaser_blend", 1.0f },
  };
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "phaser_module",
  "PhaserModule in effect_module_harness.h: controls set per block, 'on' toggles enable()",
  [](const nlohmann::json&) -> vital::Processor* {
    return new effects_harness::EffectModuleHarness(
        [](const vital::Output* beats_per_second) { return new vital::PhaserModule(beats_per_second); },
        effects_harness::controlNames(kControls));
  },
  effects_harness::harnessInputs(kControls),
  { { "audio", 0 } },
});
