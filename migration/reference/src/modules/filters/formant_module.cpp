// Module "formant_module": FormantModule("filter_1"), initialised by the
// factory. Ports: port/crates/filters (src/character/formant_module.rs,
// vocal_tract.rs).
//
// params.controls: { "<control name>": value } applied with Value::set after
// init (for example "filter_1_formant_x"); unknown names fail the factory.
// The module reads its formant X/Y, transpose, resonance and spread from
// these controls, not from inputs.

#include "module_registry.h"

#include "formant_module.h"

using refmod::Rate;
using M = vital::FormantModule;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "formant_module",
  "FormantModule(\"filter_1\"), initialised. params.controls: {name: value}",
  [](const nlohmann::json& params) -> vital::Processor* {
    M* module = new M("filter_1");
    module->init();
    if (params.count("controls")) {
      vital::control_map controls = module->getControls();
      for (auto& control : params["controls"].items()) {
        auto found = controls.find(control.key());
        if (found == controls.end()) {
          delete module;
          return nullptr;
        }
        found->second->set(control.value().get<float>());
      }
    }
    return module;
  },
  {
    { "audio", M::kAudio, 0.0f, Rate::kAudio },
    { "reset", M::kReset, 0.0f, Rate::kTrigger },
    { "resonance", M::kResonance, 0.0f, Rate::kControl },
    { "blend", M::kBlend, 0.0f, Rate::kControl },
    { "style", M::kStyle, 0.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
