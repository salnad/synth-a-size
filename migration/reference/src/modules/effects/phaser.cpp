// Module "phaser": Phaser (init() called in the factory, as PhaserModule does).
// Ports: port/crates/effects.

#include "module_registry.h"

// phaser_filter.h uses futils without including it (upstream includes it first).
#include "futils.h"
#include "phaser.h"
#include "phaser_filter.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "phaser",
  "Phaser. Inputs take the values PhaserModule's controls produce (rate in Hz, center in MIDI notes)",
  [](const nlohmann::json&) -> vital::Processor* {
    vital::Phaser* phaser = new vital::Phaser();
    phaser->init();
    return phaser;
  },
  {
    { "audio", vital::Phaser::kAudio, 0.0f, Rate::kAudio },
    { "mix", vital::Phaser::kMix, 1.0f, Rate::kControl },
    { "rate", vital::Phaser::kRate, 0.125f, Rate::kControl },
    { "feedback_gain", vital::Phaser::kFeedbackGain, 0.5f, Rate::kControl },
    { "center", vital::Phaser::kCenter, 80.0f, Rate::kAudio },
    { "mod_depth", vital::Phaser::kModDepth, 24.0f, Rate::kControl },
    { "phase_offset", vital::Phaser::kPhaseOffset, 0.33333333f, Rate::kControl },
    { "blend", vital::Phaser::kBlend, 1.0f, Rate::kControl },
  },
  { { "audio", vital::Phaser::kAudioOutput }, { "cutoff", vital::Phaser::kCutoffOutput } },
});
