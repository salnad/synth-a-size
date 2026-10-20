// Module "dc_filter": DcFilter. Ports: port/crates/filters.

#include "module_registry.h"

#include "dc_filter.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "dc_filter",
  "DcFilter",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::DcFilter(); },
  {
    { "audio", vital::DcFilter::kAudio, 0.0f, refmod::Rate::kAudio },
    { "reset", vital::DcFilter::kReset, 0.0f, refmod::Rate::kTrigger },
  },
  { { "audio", 0 } },
});
