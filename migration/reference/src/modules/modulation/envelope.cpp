// Module "envelope": Envelope (DAHDSR). Ports: port/crates/modulation.

#include "module_registry.h"

#include "envelope.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "envelope",
  "Envelope (DAHDSR). Drive 'trigger' with voice_on / voice_off events. params.control_rate: bool",
  [](const nlohmann::json& params) -> vital::Processor* {
    vital::Envelope* envelope = new vital::Envelope();
    envelope->setControlRate(params.value("control_rate", false));
    return envelope;
  },
  {
    { "delay", vital::Envelope::kDelay, 0.0f, Rate::kControl },
    { "attack", vital::Envelope::kAttack, 0.1f, Rate::kControl },
    { "attack_power", vital::Envelope::kAttackPower, 0.0f, Rate::kControl },
    { "hold", vital::Envelope::kHold, 0.0f, Rate::kControl },
    { "decay", vital::Envelope::kDecay, 0.5f, Rate::kControl },
    { "decay_power", vital::Envelope::kDecayPower, -2.0f, Rate::kControl },
    { "sustain", vital::Envelope::kSustain, 0.7f, Rate::kControl },
    { "release", vital::Envelope::kRelease, 0.3f, Rate::kControl },
    { "release_power", vital::Envelope::kReleasePower, -2.0f, Rate::kControl },
    { "trigger", vital::Envelope::kTrigger, 0.0f, Rate::kTrigger },
  },
  { { "value", vital::Envelope::kValue }, { "phase", vital::Envelope::kPhase } },
});
