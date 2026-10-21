// Module "trigger_random": TriggerRandom (per-voice random value on note-on). Ports: port/crates/modulation.

#include "module_registry.h"

#include "trigger_random.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "trigger_random",
  "TriggerRandom. Set the spec seed. params: use_clone: bool (capture a clone(), which takes a new seed)",
  [](const nlohmann::json& params) -> vital::Processor* {
    vital::Processor* random = new vital::TriggerRandom();
    if (params.value("use_clone", false)) {
      vital::Processor* copy = random->clone();
      delete random;
      random = copy;
    }
    return random;
  },
  {
    { "reset", vital::TriggerRandom::kReset, 0.0f, Rate::kTrigger },
  },
  { { "value", 0 } },
});
