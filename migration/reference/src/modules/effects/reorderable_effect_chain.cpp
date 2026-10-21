// Module "reorderable_effect_chain": ReorderableEffectChain driven like
// SoundEngine drives it. Ports: port/crates/effects (Rust mirror:
// port/crates/effects/tests/effect_chain.rs).
//
// - The chain is built with beats-per-second and keytrack control outputs
//   and init()ed; its order input is a control value.
// - Once per block, before processing, beats_per_second, keytrack, the
//   order and every named control are set from their inputs (lane 0), as
//   SynthBase sets parameter values with control->set(value).
// - The chain then runs processWithInput on the audio input.
//
// FilterFxModule is defined in reorderable_effect_chain.cpp, so it is
// captured through the chain (filter_fx_on with the other effects off).
//
// Inputs: audio (0), order (1), beats_per_second (2), keytrack (3), then the
// controls in kControls order.

#include "module_registry.h"

// phaser_filter.h uses futils without including it (upstream includes it first).
#include "futils.h"
#include "reorderable_effect_chain.h"
#include "synth_module.h"
#include "value.h"

#include <map>
#include <memory>
#include <string>
#include <vector>

namespace {
  enum { kAudio, kOrder, kBeatsPerSecond, kKeytrack, kFirstControl };

  const std::vector<std::pair<std::string, float>> kControls = {
    { "chorus_on", 0.0f },
    { "compressor_on", 0.0f },
    { "delay_on", 0.0f },
    { "distortion_on", 0.0f },
    { "eq_on", 0.0f },
    { "filter_fx_on", 0.0f },
    { "flanger_on", 0.0f },
    { "phaser_on", 0.0f },
    { "reverb_on", 0.0f },
    { "filter_fx_mix", 1.0f },
    { "filter_fx_cutoff", 60.0f },
    { "filter_fx_resonance", 0.5f },
    { "filter_fx_drive", 0.0f },
    { "filter_fx_blend", 0.0f },
    { "filter_fx_style", 0.0f },
    { "filter_fx_model", 0.0f },
    { "filter_fx_blend_transpose", 42.0f },
    { "filter_fx_keytrack", 0.0f },
    { "filter_fx_formant_x", 0.5f },
    { "filter_fx_formant_y", 0.5f },
    { "filter_fx_formant_transpose", 0.0f },
    { "filter_fx_formant_resonance", 0.85f },
    { "filter_fx_formant_spread", 0.0f },
  };

  class ChainHarness : public vital::Processor {
    public:
      ChainHarness() :
          vital::Processor(kFirstControl + static_cast<int>(kControls.size()), 1),
          order_(0.0f), beats_per_second_(2.0f), keytrack_(0.0f) {
        chain_ = std::make_unique<vital::ReorderableEffectChain>(beats_per_second_.output(),
                                                                 keytrack_.output());
        chain_->plug(&order_, vital::ReorderableEffectChain::kOrder);
        chain_->init();

        std::map<std::string, vital::Value*> controls = chain_->getControls();
        for (const auto& control : kControls)
          controls_.push_back(controls.at(control.first));
      }

      void setSampleRate(int sample_rate) override {
        vital::Processor::setSampleRate(sample_rate);
        chain_->setSampleRate(sample_rate);
      }

      void setOversampleAmount(int oversample) override {
        vital::Processor::setOversampleAmount(oversample);
        chain_->setOversampleAmount(oversample);
      }

      void hardReset() override { chain_->hardReset(); }

      void process(int num_samples) override {
        beats_per_second_.set(input(kBeatsPerSecond)->at(0));
        keytrack_.set(input(kKeytrack)->at(0));
        order_.set(input(kOrder)->at(0));
        for (size_t i = 0; i < controls_.size(); ++i)
          controls_[i]->set(input(kFirstControl + static_cast<int>(i))->at(0)[0]);

        chain_->processWithInput(input(kAudio)->source->buffer, num_samples);
        vital::utils::copyBuffer(output()->buffer, chain_->output()->buffer, num_samples);
      }

      vital::Processor* clone() const override { return nullptr; }

    private:
      vital::cr::Value order_;
      vital::cr::Value beats_per_second_;
      vital::cr::Value keytrack_;
      std::unique_ptr<vital::ReorderableEffectChain> chain_;
      std::vector<vital::Value*> controls_;
  };

  std::vector<refmod::InputDef> chainInputs() {
    std::vector<refmod::InputDef> inputs = {
      { "audio", kAudio, 0.0f, refmod::Rate::kAudio },
      { "order", kOrder, 0.0f, refmod::Rate::kControl },
      { "beats_per_second", kBeatsPerSecond, 2.0f, refmod::Rate::kControl },
      { "keytrack", kKeytrack, 0.0f, refmod::Rate::kControl },
    };
    int index = kFirstControl;
    for (const auto& control : kControls)
      inputs.push_back({ control.first, index++, control.second, refmod::Rate::kControl });
    return inputs;
  }
}

REGISTER_REF_MODULE(refmod::ModuleDef{
  "reorderable_effect_chain",
  "ReorderableEffectChain (all nine effects, FilterFxModule included): order, keytrack and controls set per block",
  [](const nlohmann::json&) -> vital::Processor* { return new ChainHarness(); },
  chainInputs(),
  { { "audio", 0 } },
});
