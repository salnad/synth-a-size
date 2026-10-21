// Module "multi_delay": Delay<Memory> (MultiDelay, four independent lanes).
// Ports: port/crates/effects.

#include "module_registry.h"

#include "delay.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "multi_delay",
  "Delay<Memory>(params.size, default 3841 samples, ChorusModule's size at 48 kHz). 'style' as stereo_delay",
  [](const nlohmann::json& params) -> vital::Processor* {
    return new vital::MultiDelay(params.value("size", 3841));
  },
  {
    { "audio", vital::MultiDelay::kAudio, 0.0f, Rate::kAudio },
    { "wet", vital::MultiDelay::kWet, 1.0f, Rate::kControl },
    { "frequency", vital::MultiDelay::kFrequency, 100.0f, Rate::kControl },
    { "frequency_aux", vital::MultiDelay::kFrequencyAux, 100.0f, Rate::kControl },
    { "feedback", vital::MultiDelay::kFeedback, 0.4f, Rate::kControl },
    { "damping", vital::MultiDelay::kDamping, 0.5f, Rate::kControl },
    { "style", vital::MultiDelay::kStyle, 0.0f, Rate::kControl },
    { "filter_cutoff", vital::MultiDelay::kFilterCutoff, 60.0f, Rate::kControl },
    { "filter_spread", vital::MultiDelay::kFilterSpread, 1.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
