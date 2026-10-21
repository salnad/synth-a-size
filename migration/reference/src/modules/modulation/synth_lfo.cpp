// Module "synth_lfo": SynthLfo (shape LFO). Ports: port/crates/modulation.

#include "module_registry.h"

#include "line_shape.h"
#include "synth_lfo.h"

using refmod::Rate;

namespace {
  // osc_frequency (Hz) is captured as the per-sample phase increment SynthLfo
  // itself uses (synth_lfo.cpp:411-413): frequency * (1.0f / getSampleRate()).
  class PhaseIncrementLfo : public vital::SynthLfo {
    public:
      using vital::SynthLfo::SynthLfo;
      vital::Processor* clone() const override { return new PhaseIncrementLfo(*this); }

      void process(int num_samples) override {
        vital::SynthLfo::process(num_samples);
        float tick_time = 1.0f / getSampleRate();
        output(kOscFrequency)->buffer[0] = output(kOscFrequency)->buffer[0] * tick_time;
      }
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "synth_lfo",
  "SynthLfo. params: shape (see line_shape.h), control_rate: bool (default true), "
  "sync_seconds: correctToTime() value. Output osc_frequency is the phase increment frequency / sample rate",
  [](const nlohmann::json& params) -> vital::Processor* {
    LineGenerator* line = refmod_modulation::createLineShape(params);
    if (line == nullptr)
      return nullptr;
    PhaseIncrementLfo* lfo = new PhaseIncrementLfo(line);
    lfo->setControlRate(params.value("control_rate", true));
    lfo->correctToTime(params.value("sync_seconds", 0.0));
    return lfo;
  },
  {
    { "frequency", vital::SynthLfo::kFrequency, 1.0f, Rate::kControl },
    { "phase", vital::SynthLfo::kPhase, 0.0f, Rate::kControl },
    { "note_trigger", vital::SynthLfo::kNoteTrigger, 0.0f, Rate::kTrigger },
    { "sync_type", vital::SynthLfo::kSyncType, 0.0f, Rate::kControl },
    { "smooth_mode", vital::SynthLfo::kSmoothMode, 0.0f, Rate::kControl },
    { "fade", vital::SynthLfo::kFade, 0.0f, Rate::kControl },
    { "smooth_time", vital::SynthLfo::kSmoothTime, 0.0f, Rate::kControl },
    { "stereo_phase", vital::SynthLfo::kStereoPhase, 0.0f, Rate::kControl },
    { "delay", vital::SynthLfo::kDelay, 0.0f, Rate::kControl },
    { "note_count", vital::SynthLfo::kNoteCount, 0.0f, Rate::kControl },
  },
  {
    { "value", vital::SynthLfo::kValue },
    { "osc_phase", vital::SynthLfo::kOscPhase },
    { "osc_frequency", vital::SynthLfo::kOscFrequency },
  },
});
