// Module "envelope_module": EnvelopeModule (Envelope with registry controls). Ports: port/crates/modulation.

#include "module_registry.h"

#include "envelope.h"
#include "envelope_module.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "envelope_module",
  "EnvelopeModule. Drive 'trigger' with voice events. params: prefix (default env_1), "
  "force_audio_rate: bool, controls: { <control name>: value } set after init()",
  [](const nlohmann::json& params) -> vital::Processor* {
    std::string prefix = params.value("prefix", std::string("env_1"));
    vital::EnvelopeModule* module = new vital::EnvelopeModule(prefix, params.value("force_audio_rate", false));
    module->init();
    vital::control_map controls = module->getControls();
    if (params.count("controls")) {
      for (auto& control : params["controls"].items()) {
        if (controls.count(control.key()) == 0) {
          delete module;
          return nullptr;
        }
        controls[control.key()]->set(control.value().get<float>());
      }
    }
    return module;
  },
  {
    { "trigger", vital::EnvelopeModule::kTrigger, 0.0f, Rate::kTrigger },
  },
  { { "value", vital::EnvelopeModule::kValue }, { "phase", vital::EnvelopeModule::kPhase } },
});
