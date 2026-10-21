// Module "linkwitz_riley": LinkwitzRileyFilter(cutoff), both bands.
// Ports: port/crates/filters (linkwitz_riley_filter.rs).
//
// params: cutoff (Hz, default 120, the compressor's low band).

#include "module_registry.h"

#include "linkwitz_riley_filter.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "linkwitz_riley",
  "LinkwitzRileyFilter(cutoff). params: cutoff (Hz)",
  [](const nlohmann::json& params) -> vital::Processor* {
    return new vital::LinkwitzRileyFilter(params.value("cutoff", 120.0f));
  },
  {
    { "audio", vital::LinkwitzRileyFilter::kAudio, 0.0f, refmod::Rate::kAudio },
  },
  {
    { "low", vital::LinkwitzRileyFilter::kAudioLow },
    { "high", vital::LinkwitzRileyFilter::kAudioHigh },
  },
});
