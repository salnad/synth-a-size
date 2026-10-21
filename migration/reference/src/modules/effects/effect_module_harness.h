// Drives one effect SynthModule the way the engine does, for the
// effects/<effect>_module captures. Ports: port/crates/effects (the Rust
// mirror is port/crates/effects/tests/common/harness.rs).
//
// - The module is built with a beats-per-second control output (as
//   SoundEngine passes to ReorderableEffectChain) and init()ed.
// - Once per block, before processing, every named control is set to the
//   value of its input (lane 0), as SynthBase sets parameter values with
//   control->set(value). Tempo-sync "<name>_sync" controls are named like any
//   other.
// - ReorderableEffectChain::processWithInput: when "on" changes, the module is
//   enable()d or disabled; an enabled module processes the audio, a disabled
//   one passes it through.
//
// Inputs: audio (index 0), on (1), beats_per_second (2), then the controls in
// the order given.
#pragma once

#include "module_registry.h"

#include "synth_module.h"
#include "value.h"

#include <functional>
#include <memory>
#include <string>
#include <vector>

namespace effects_harness {

class EffectModuleHarness : public vital::Processor {
  public:
    enum { kAudio, kOn, kBeatsPerSecond, kFirstControl };

    using Factory = std::function<vital::SynthModule*(const vital::Output* beats_per_second)>;

    EffectModuleHarness(const Factory& create, std::vector<std::string> control_names) :
        vital::Processor(kFirstControl + static_cast<int>(control_names.size()), 1),
        beats_per_second_(2.0f) {
      module_.reset(create(beats_per_second_.output()));
      module_->init();

      std::map<std::string, vital::Value*> controls = module_->getControls();
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
      beats_per_second_.set(input(kBeatsPerSecond)->at(0));
      for (size_t i = 0; i < controls_.size(); ++i)
        controls_[i]->set(input(kFirstControl + static_cast<int>(i))->at(0)[0]);

      const vital::poly_float* audio_in = input(kAudio)->source->buffer;
      bool on = input(kOn)->at(0)[0];
      if (on != module_->enabled())
        module_->enable(on);

      const vital::poly_float* result = audio_in;
      if (on) {
        module_->processWithInput(audio_in, num_samples);
        result = module_->output(0)->buffer;
      }
      vital::utils::copyBuffer(output()->buffer, result, num_samples);
    }

    vital::Processor* clone() const override { return nullptr; }

  private:
    vital::cr::Value beats_per_second_;
    std::unique_ptr<vital::SynthModule> module_;
    std::vector<vital::Value*> controls_;
};

// The harness inputs: audio, on, beats_per_second, then `controls` (name and
// default value).
inline std::vector<refmod::InputDef> harnessInputs(const std::vector<std::pair<std::string, float>>& controls) {
  std::vector<refmod::InputDef> inputs = {
    { "audio", EffectModuleHarness::kAudio, 0.0f, refmod::Rate::kAudio },
    { "on", EffectModuleHarness::kOn, 1.0f, refmod::Rate::kControl },
    { "beats_per_second", EffectModuleHarness::kBeatsPerSecond, 2.0f, refmod::Rate::kControl },
  };
  int index = EffectModuleHarness::kFirstControl;
  for (const auto& control : controls)
    inputs.push_back({ control.first, index++, control.second, refmod::Rate::kControl });
  return inputs;
}

inline std::vector<std::string> controlNames(const std::vector<std::pair<std::string, float>>& controls) {
  std::vector<std::string> names;
  for (const auto& control : controls)
    names.push_back(control.first);
  return names;
}

} // namespace effects_harness
