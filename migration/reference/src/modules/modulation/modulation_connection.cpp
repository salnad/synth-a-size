// Module "modulation_connection": ModulationConnectionProcessor (one modulation matrix slot). Ports: port/crates/modulation.

#include "module_registry.h"

#include "line_shape.h"
#include "modulation_connection_processor.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "modulation_connection",
  "ModulationConnectionProcessor. params: index (default 0), destination_scale (default 1), "
  "control_rate: bool (default true; audio-rate paths need control_rate false and an audio-rate source), "
  "map: remap line (see line_shape.h, default linear), controls: { <control name>: value } set after init()",
  [](const nlohmann::json& params) -> vital::Processor* {
    vital::ModulationConnectionProcessor* processor =
        new vital::ModulationConnectionProcessor(params.value("index", 0));
    processor->init();
    if (!refmod_modulation::applyLineShape(processor->lineMapGenerator(), params, "map")) {
      delete processor;
      return nullptr;
    }
    vital::control_map controls = processor->getControls();
    if (params.count("controls")) {
      for (auto& control : params["controls"].items()) {
        if (controls.count(control.key()) == 0) {
          delete processor;
          return nullptr;
        }
        controls[control.key()]->set(control.value().get<float>());
      }
    }
    processor->setDestinationScale(params.value("destination_scale", 1.0f));
    processor->setControlRate(params.value("control_rate", true));
    return processor;
  },
  {
    { "modulation_input", vital::ModulationConnectionProcessor::kModulationInput, 0.0f, Rate::kAudio },
    { "modulation_amount", vital::ModulationConnectionProcessor::kModulationAmount, 0.0f, Rate::kControl },
    { "modulation_power", vital::ModulationConnectionProcessor::kModulationPower, 0.0f, Rate::kControl },
    { "reset", vital::ModulationConnectionProcessor::kReset, 0.0f, Rate::kTrigger },
  },
  {
    { "modulation_output", vital::ModulationConnectionProcessor::kModulationOutput },
    { "modulation_pre_scale", vital::ModulationConnectionProcessor::kModulationPreScale },
    { "modulation_source", vital::ModulationConnectionProcessor::kModulationSource },
  },
});
