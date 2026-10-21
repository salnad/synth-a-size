// Module "reverb": Reverb. Ports: port/crates/effects.

#include "module_registry.h"

#include "memory.h"
#include "reverb.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "reverb",
  "Reverb. Inputs take the values ReverbModule's controls produce (decay in seconds, cutoffs in MIDI notes, "
  "gains in dB, chorus frequency in Hz, delay in seconds)",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Reverb(); },
  {
    { "audio", vital::Reverb::kAudio, 0.0f, Rate::kAudio },
    { "decay_time", vital::Reverb::kDecayTime, 1.0f, Rate::kControl },
    { "pre_low_cutoff", vital::Reverb::kPreLowCutoff, 0.0f, Rate::kControl },
    { "pre_high_cutoff", vital::Reverb::kPreHighCutoff, 110.0f, Rate::kControl },
    { "low_cutoff", vital::Reverb::kLowCutoff, 0.0f, Rate::kControl },
    { "low_gain", vital::Reverb::kLowGain, 0.0f, Rate::kControl },
    { "high_cutoff", vital::Reverb::kHighCutoff, 90.0f, Rate::kControl },
    { "high_gain", vital::Reverb::kHighGain, -1.0f, Rate::kControl },
    { "chorus_amount", vital::Reverb::kChorusAmount, 0.05f, Rate::kControl },
    { "chorus_frequency", vital::Reverb::kChorusFrequency, 0.25f, Rate::kControl },
    { "size", vital::Reverb::kSize, 0.5f, Rate::kControl },
    { "delay", vital::Reverb::kDelay, 0.0f, Rate::kControl },
    { "wet", vital::Reverb::kWet, 0.25f, Rate::kControl },
  },
  { { "audio", 0 } },
});
