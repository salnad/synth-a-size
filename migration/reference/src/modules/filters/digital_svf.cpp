// Module "digital_svf": DigitalSvf with the per-instance settings the
// equalizer (setBasic, setDriveCompensation) and the formant manager
// (setResonanceBounds) use. Ports: port/crates/filters (digital_svf.rs).
//
// params: basic (bool, default false), drive_compensation (bool, default
// true), min_resonance / max_resonance (default kDefaultMin/MaxResonance).

#include "module_registry.h"

#include "digital_svf.h"

namespace {
using refmod::Rate;
using F = vital::DigitalSvf;
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "digital_svf",
  "DigitalSvf. params: basic, drive_compensation, min_resonance, max_resonance",
  [](const nlohmann::json& params) -> vital::Processor* {
    F* svf = new F();
    svf->setBasic(params.value("basic", false));
    svf->setDriveCompensation(params.value("drive_compensation", true));
    float default_min = F::kDefaultMinResonance;
    float default_max = F::kDefaultMaxResonance;
    svf->setResonanceBounds(params.value("min_resonance", default_min),
                            params.value("max_resonance", default_max));
    return svf;
  },
  {
    { "audio", F::kAudio, 0.0f, Rate::kAudio },
    { "reset", F::kReset, 0.0f, Rate::kTrigger },
    { "midi_cutoff", F::kMidiCutoff, 60.0f, Rate::kAudio },
    { "resonance", F::kResonance, 0.5f, Rate::kControl },
    { "drive_gain", F::kDriveGain, 0.0f, Rate::kControl },
    { "gain", F::kGain, 0.0f, Rate::kControl },
    { "style", F::kStyle, 0.0f, Rate::kControl },
    { "pass_blend", F::kPassBlend, 0.0f, Rate::kControl },
  },
  { { "audio", 0 } },
});
