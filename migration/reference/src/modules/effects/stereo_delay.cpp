// Module "stereo_delay": Delay<StereoMemory>. Ports: port/crates/effects.

#include "module_registry.h"

#include "delay.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "stereo_delay",
  "Delay<StereoMemory>(params.size, default 88200 samples). 'style': 0 mono, 1 stereo, 2 ping pong, 3 mid ping pong",
  [](const nlohmann::json& params) -> vital::Processor* {
    return new vital::StereoDelay(params.value("size", 88200));
  },
  {
    { "audio", vital::StereoDelay::kAudio, 0.0f, Rate::kAudio },
    { "wet", vital::StereoDelay::kWet, 0.5f, Rate::kControl },
    { "frequency", vital::StereoDelay::kFrequency, 2.0f, Rate::kControl },
    { "frequency_aux", vital::StereoDelay::kFrequencyAux, 2.0f, Rate::kControl },
    { "feedback", vital::StereoDelay::kFeedback, 0.5f, Rate::kControl },
    { "damping", vital::StereoDelay::kDamping, 0.5f, Rate::kControl },
    { "style", vital::StereoDelay::kStyle, 0.0f, Rate::kControl },
    { "filter_cutoff", vital::StereoDelay::kFilterCutoff, 60.0f, Rate::kControl },
    { "filter_spread", vital::StereoDelay::kFilterSpread, 1.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
