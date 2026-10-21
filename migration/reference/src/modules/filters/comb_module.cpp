// Module "comb_module": CombModule (init() called by the factory). Ports:
// port/crates/filters (src/character/comb_module.rs).

#include "module_registry.h"

#include "comb_module.h"

using refmod::Rate;
using M = vital::CombModule;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "comb_module",
  "CombModule, initialised",
  [](const nlohmann::json&) -> vital::Processor* {
    M* module = new M();
    module->init();
    return module;
  },
  {
    { "audio", M::kAudio, 0.0f, Rate::kAudio },
    { "reset", M::kReset, 0.0f, Rate::kTrigger },
    { "midi_cutoff", M::kMidiCutoff, 60.0f, Rate::kAudio },
    { "midi_blend_transpose", M::kMidiBlendTranspose, 42.0f, Rate::kControl },
    { "filter_cutoff_blend", M::kFilterCutoffBlend, 1.0f, Rate::kControl },
    { "style", M::kStyle, 0.0f, Rate::kControl },
    { "resonance", M::kResonance, 0.5f, Rate::kControl },
    { "midi", M::kMidi, 60.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
