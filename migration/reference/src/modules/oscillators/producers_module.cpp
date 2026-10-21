// Module "producers_module": ProducersModule (three oscillators and the
// sampler, FM/RM order, destination routing).
// Ports: port/crates/oscillators (producers_module).
//
// params.wavetables: {"osc_1": spec, "osc_2": spec, "osc_3": spec} (see
// module_content.h; missing ones keep the default wavetable), params.sample
// (optional, see module_content.h), params.controls: {name: value}. All are
// applied after init(), as a preset load does.

#include "module_registry.h"

#include "module_content.h"
#include "producers_module.h"

namespace {

using refmod::Rate;
using M = vital::ProducersModule;

bool loadContent(M* module, const nlohmann::json& params) {
  const nlohmann::json wavetables = params.value("wavetables", nlohmann::json::object());
  for (int i = 0; i < vital::kNumOscillators; ++i) {
    std::string name = "osc_" + std::to_string(i + 1);
    if (wavetables.count(name) && !module_content::renderWavetable(module->getWavetable(i), wavetables[name]))
      return false;
  }
  if (params.count("sample"))
    module_content::loadSample(module->getSample(), params["sample"]);
  return module_content::setControls(module, params.value("controls", nlohmann::json::object()));
}

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "producers_module",
  "ProducersModule: params.wavetables {osc_N: spec}, params.sample, params.controls {name: value}",
  [](const nlohmann::json& params) -> vital::Processor* {
    M* module = new M();
    module->init();
    try {
      if (!loadContent(module, params)) {
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
    { "note_count", M::kNoteCount, 1.0f, Rate::kControl },
  },
  { { "to_filter1", M::kToFilter1 }, { "to_filter2", M::kToFilter2 },
    { "raw_out", M::kRawOut }, { "direct_out", M::kDirectOut } },
});
