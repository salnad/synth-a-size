// Module "line_map": LineMap (control-rate phase through a line shape). Ports: port/crates/modulation.

#include "module_registry.h"

#include "line_map.h"
#include "line_shape.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "line_map",
  "LineMap. params: shape (see line_shape.h, default triangle)",
  [](const nlohmann::json& params) -> vital::Processor* {
    LineGenerator* line = refmod_modulation::createLineShape(params);
    if (line == nullptr)
      return nullptr;
    return new vital::LineMap(line);
  },
  {
    { "phase", 0, 0.0f, Rate::kControl },
  },
  { { "value", vital::LineMap::kValue }, { "phase", vital::LineMap::kPhase } },
});
