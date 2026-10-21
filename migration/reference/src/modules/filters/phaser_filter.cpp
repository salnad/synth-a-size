// Module "phaser_filter": PhaserFilter(clean). Ports: port/crates/filters
// (src/character/phaser_filter.rs).
//
// params.clean: false is the filter-section model (SynthFilter "phase"),
// true is the configuration the phaser effect uses.

#include "module_registry.h"

// phaser_filter.h uses futils without including it (upstream's unity build
// includes it first).
#include "futils.h"
#include "phaser_filter.h"

using refmod::Rate;
using F = vital::PhaserFilter;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "phaser_filter",
  "PhaserFilter(clean). params.clean: bool (default false)",
  [](const nlohmann::json& params) -> vital::Processor* {
    return new vital::PhaserFilter(params.value("clean", false));
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
    { "transpose", F::kTranspose, 0.0f, Rate::kControl },
    { "spread", F::kSpread, 0.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
