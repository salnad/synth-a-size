// Module "oscillator_module": OscillatorModule("osc_1") with its controls.
// Ports: port/crates/oscillators (oscillator_module).
//
// params.wavetable: see module_content.h (rendered after init(), as a
// preset load does); params.controls: {"osc_1_...": value}.

#include "module_registry.h"

#include "module_content.h"
#include "oscillator_module.h"

namespace {

using refmod::Rate;
using M = vital::OscillatorModule;

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "oscillator_module",
  "OscillatorModule(\"osc_1\"): params.wavetable ({preset} or {creator}), params.controls {name: value}",
  [](const nlohmann::json& params) -> vital::Processor* {
    M* module = new M("osc_1");
    module->init();
    try {
      if (!module_content::renderWavetable(module->getWavetable(),
                                           params.value("wavetable", nlohmann::json::object())) ||
          !module_content::setControls(module, params.value("controls", nlohmann::json::object()))) {
        delete module;
        return nullptr;
      }
    }
    catch (const std::exception&) {
      delete module;
      return nullptr;
    }
    return module;
  },
  {
    { "reset", M::kReset, 0.0f, Rate::kTrigger },
    { "retrigger", M::kRetrigger, 0.0f, Rate::kTrigger },
    { "midi", M::kMidi, 60.0f, Rate::kControl },
    { "active_voices", M::kActiveVoices, 1.0f, Rate::kControl },
  },
  { { "raw", M::kRaw }, { "levelled", M::kLevelled } },
});
