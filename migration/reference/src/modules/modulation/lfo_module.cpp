// Module "lfo_module": LfoModule (SynthLfo with registry controls and tempo sync). Ports: port/crates/modulation.

#include "module_registry.h"

#include "lfo_module.h"
#include "line_shape.h"
#include "synth_lfo.h"
#include "value.h"

using refmod::Rate;

namespace {
  // osc_frequency (Hz) is captured as the per-sample phase increment SynthLfo
  // itself uses (synth_lfo.cpp:411-413): frequency * (1.0f / getSampleRate()).
  class PhaseIncrementLfoModule : public vital::LfoModule {
    public:
      using vital::LfoModule::LfoModule;
      vital::Processor* clone() const override { return new PhaseIncrementLfoModule(*this); }

      void process(int num_samples) override {
        vital::LfoModule::process(num_samples);
        float tick_time = 1.0f / getSampleRate();
        output(kOscFrequency)->buffer[0] = output(kOscFrequency)->buffer[0] * tick_time;
      }
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "lfo_module",
  "LfoModule. params: prefix (default lfo_1), shape (see line_shape.h), beats_per_second (default 2), "
  "control_rate: bool (default true), sync_seconds, controls: { <control name>: value } set after init(). "
  "Output osc_frequency is the phase increment frequency / sample rate",
  [](const nlohmann::json& params) -> vital::Processor* {
    LineGenerator* line = refmod_modulation::createLineShape(params);
    if (line == nullptr)
      return nullptr;
    // Never freed, like the line: the module keeps a pointer to its output.
    vital::cr::Value* beats_per_second = new vital::cr::Value(params.value("beats_per_second", 2.0f));
    std::string prefix = params.value("prefix", std::string("lfo_1"));
    PhaseIncrementLfoModule* module = new PhaseIncrementLfoModule(prefix, line, beats_per_second->output());
    module->init();
    module->setControlRate(params.value("control_rate", true));
    module->correctToTime(params.value("sync_seconds", 0.0));
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
    { "note_trigger", vital::LfoModule::kNoteTrigger, 0.0f, Rate::kTrigger },
    { "note_count", vital::LfoModule::kNoteCount, 0.0f, Rate::kControl },
    { "midi", vital::LfoModule::kMidi, 60.0f, Rate::kControl },
  },
  {
    { "value", vital::LfoModule::kValue },
    { "osc_phase", vital::LfoModule::kOscPhase },
    { "osc_frequency", vital::LfoModule::kOscFrequency },
  },
});
