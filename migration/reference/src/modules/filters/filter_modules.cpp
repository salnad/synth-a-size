// Modules "filter_module" and "filters_module": FilterModule("filter_1") and
// FiltersModule, driven the way the voice engine drives them. Ports:
// port/crates/filters (src/modules/filter_module.rs, filters_module.rs; the
// Rust mirror of this harness is tests/filter_modules.rs).
//
// - The module's own inputs are the harness's first inputs (useInput before
//   init(), so every sub-processor reads them), then the module is init()ed.
// - Once per block, before processing, every named control is set to the
//   value of its input (lane 0), as SynthBase sets parameter values with
//   control->set(value).
// - The module processes and its output is copied to the harness output.
//
// params.mono (bool, filter_module only): FilterModule::setMono before init.

#include "module_registry.h"

#include "filter_module.h"
#include "filters_module.h"
#include "synth_module.h"
#include "value.h"

#include <functional>
#include <memory>
#include <string>
#include <vector>

using refmod::Rate;

namespace {
  class SynthModuleHarness : public vital::Processor {
    public:
      SynthModuleHarness(vital::SynthModule* module, const std::vector<int>& module_inputs,
                         const std::vector<std::string>& control_names) :
          vital::Processor(static_cast<int>(module_inputs.size() + control_names.size()), 1),
          module_(module), num_module_inputs_(static_cast<int>(module_inputs.size())) {
        for (int i = 0; i < num_module_inputs_; ++i)
          module_->useInput(input(i), module_inputs[i]);
        module_->init();

        vital::control_map controls = module_->getControls();
        for (const std::string& name : control_names)
          controls_.push_back(controls.at(name));
      }

      void setSampleRate(int sample_rate) override {
        vital::Processor::setSampleRate(sample_rate);
        module_->setSampleRate(sample_rate);
      }

      void setOversampleAmount(int oversample) override {
        vital::Processor::setOversampleAmount(oversample);
        module_->setOversampleAmount(oversample);
      }

      void hardReset() override { module_->hardReset(); }

      void process(int num_samples) override {
        for (size_t i = 0; i < controls_.size(); ++i)
          controls_[i]->set(input(num_module_inputs_ + static_cast<int>(i))->at(0)[0]);

        module_->process(num_samples);
        vital::utils::copyBuffer(output()->buffer, module_->output(0)->buffer, num_samples);
      }

      vital::Processor* clone() const override { return nullptr; }

    private:
      std::unique_ptr<vital::SynthModule> module_;
      int num_module_inputs_;
      std::vector<vital::Value*> controls_;
  };

  // FilterModule(prefix)'s controls (its own and its FormantModule's), with
  // the registry default each input starts at.
  std::vector<std::pair<std::string, float>> filterControls(const std::string& prefix) {
    return {
      { prefix + "_on", 1.0f },
      { prefix + "_model", 0.0f },
      { prefix + "_style", 0.0f },
      { prefix + "_cutoff", 60.0f },
      { prefix + "_resonance", 0.5f },
      { prefix + "_drive", 0.0f },
      { prefix + "_blend", 0.0f },
      { prefix + "_blend_transpose", 42.0f },
      { prefix + "_keytrack", 0.0f },
      { prefix + "_mix", 1.0f },
      { prefix + "_formant_x", 0.5f },
      { prefix + "_formant_y", 0.5f },
      { prefix + "_formant_transpose", 0.0f },
      { prefix + "_formant_resonance", 0.85f },
      { prefix + "_formant_spread", 0.0f },
    };
  }

  std::vector<std::pair<std::string, float>> filtersControls() {
    std::vector<std::pair<std::string, float>> controls = {
      { "filter_1_filter_input", 0.0f },
      { "filter_2_filter_input", 0.0f },
    };
    for (const auto& control : filterControls("filter_1"))
      controls.push_back(control);
    for (const auto& control : filterControls("filter_2"))
      controls.push_back(control);
    return controls;
  }

  std::vector<std::string> names(const std::vector<std::pair<std::string, float>>& controls) {
    std::vector<std::string> result;
    for (const auto& control : controls)
      result.push_back(control.first);
    return result;
  }

  std::vector<refmod::InputDef> harnessInputs(std::vector<refmod::InputDef> module_inputs,
                                              const std::vector<std::pair<std::string, float>>& controls) {
    int index = static_cast<int>(module_inputs.size());
    for (const auto& control : controls)
      module_inputs.push_back({ control.first, index++, control.second, Rate::kControl });
    return module_inputs;
  }

  using FM = vital::FilterModule;
  using FSM = vital::FiltersModule;
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "filter_module",
  "FilterModule(\"filter_1\") in the SynthBase-style control harness. params: mono",
  [](const nlohmann::json& params) -> vital::Processor* {
    FM* module = new FM("filter_1");
    module->setMono(params.value("mono", false));
    return new SynthModuleHarness(module, { FM::kAudio, FM::kReset, FM::kKeytrack, FM::kMidi },
                                  names(filterControls("filter_1")));
  },
  harnessInputs({
    { "audio", 0, 0.0f, Rate::kAudio },
    { "reset", 1, 0.0f, Rate::kTrigger },
    { "keytrack", 2, 0.0f, Rate::kControl },
    { "midi", 3, 60.0f, Rate::kControl },
  }, filterControls("filter_1")),
  { { "audio", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "filters_module",
  "FiltersModule (both filters, parallel or serial routing) in the SynthBase-style control harness",
  [](const nlohmann::json&) -> vital::Processor* {
    FSM* module = new FSM();
    return new SynthModuleHarness(module, { FSM::kFilter1Input, FSM::kFilter2Input, FSM::kKeytrack,
                                            FSM::kMidi, FSM::kReset },
                                  names(filtersControls()));
  },
  harnessInputs({
    { "filter_1_input", 0, 0.0f, Rate::kAudio },
    { "filter_2_input", 1, 0.0f, Rate::kAudio },
    { "keytrack", 2, 0.0f, Rate::kControl },
    { "midi", 3, 60.0f, Rate::kControl },
    { "reset", 4, 0.0f, Rate::kTrigger },
  }, filtersControls()),
  { { "audio", 0 } },
});
