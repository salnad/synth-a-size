// Module "random_lfo_module": RandomLfoModule (RandomLfo with registry controls and tempo sync). Ports: port/crates/modulation.

#include "module_registry.h"

#include "random_lfo.h"
#include "random_lfo_module.h"
#include "value.h"

using refmod::Rate;

namespace {
  // With params.clock_start_seconds, calls correctToTime(start + frames / rate)
  // before each block, as the engine does every block (tempo sync follows it).
  class ClockedRandomLfoModule : public vital::RandomLfoModule {
    public:
      ClockedRandomLfoModule(const std::string& prefix, const vital::Output* beats_per_second,
                             bool clocked, double start) :
          vital::RandomLfoModule(prefix, beats_per_second), clocked_(clocked), start_(start), frames_(0) { }
      vital::Processor* clone() const override { return new ClockedRandomLfoModule(*this); }

      void process(int num_samples) override {
        if (clocked_)
          correctToTime(start_ + static_cast<double>(frames_) / getSampleRate());
        vital::RandomLfoModule::process(num_samples);
        frames_ += num_samples;
      }

    private:
      bool clocked_;
      double start_;
      long long frames_;
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "random_lfo_module",
  "RandomLfoModule. Set the spec seed. params: prefix (default random_1), beats_per_second (default 2), "
  "clock_start_seconds (calls correctToTime before each block), controls: { <control name>: value } set after init()",
  [](const nlohmann::json& params) -> vital::Processor* {
    // Never freed: the module keeps a pointer to its output.
    vital::cr::Value* beats_per_second = new vital::cr::Value(params.value("beats_per_second", 2.0f));
    std::string prefix = params.value("prefix", std::string("random_1"));
    ClockedRandomLfoModule* module = new ClockedRandomLfoModule(
        prefix, beats_per_second->output(), params.count("clock_start_seconds") > 0,
        params.value("clock_start_seconds", 0.0));
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
    { "note_trigger", vital::RandomLfoModule::kNoteTrigger, 0.0f, Rate::kTrigger },
    { "midi", vital::RandomLfoModule::kMidi, 60.0f, Rate::kControl },
  },
  { { "value", 0 } },
});
