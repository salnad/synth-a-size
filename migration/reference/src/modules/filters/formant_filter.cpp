// Module "formant_filter": FormantFilter(style), initialised by the factory.
// Ports: port/crates/filters (src/character/formant_filter.rs,
// formant_manager.rs).
//
// params.style: 0 (AOIE) or 1 (AIUO). Only the inputs FormantFilter::init
// connects are registered. interpolate_x/y and transpose are audio rate, as
// FormantModule's audio-rate controls feed them.

#include "module_registry.h"

#include "digital_svf.h"
#include "formant_filter.h"

using refmod::Rate;
using F = vital::FormantFilter;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "formant_filter",
  "FormantFilter(style), initialised. params.style: 0|1",
  [](const nlohmann::json& params) -> vital::Processor* {
    int style = params.value("style", 0);
    if (style < 0 || style >= F::kNumFormantStyles)
      return nullptr;
    F* filter = new F(style);
    filter->init();
    return filter;
  },
  {
    { "audio", F::kAudio, 0.0f, Rate::kAudio },
    { "reset", F::kReset, 0.0f, Rate::kTrigger },
    { "resonance", F::kResonance, 0.85f, Rate::kControl },
    { "interpolate_x", F::kInterpolateX, 0.5f, Rate::kAudio },
    { "interpolate_y", F::kInterpolateY, 0.5f, Rate::kAudio },
    { "transpose", F::kTranspose, 0.0f, Rate::kAudio },
    { "spread", F::kSpread, 0.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
