// Module "distortion": Distortion. Ports: port/crates/effects.

#include "module_registry.h"

#include "distortion.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "distortion",
  "Distortion. Input 'type': 0 soft clip, 1 hard clip, 2 linear fold, 3 sin fold, 4 bit crush, 5 down sample",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Distortion(); },
  {
    { "audio", vital::Distortion::kAudio, 0.0f, Rate::kAudio },
    { "type", vital::Distortion::kType, 0.0f, Rate::kControl },
    { "drive", vital::Distortion::kDrive, 0.0f, Rate::kAudio },
  },
  { { "audio", vital::Distortion::kAudioOut }, { "drive", vital::Distortion::kDriveOut } },
});
