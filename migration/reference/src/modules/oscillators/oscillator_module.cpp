// Module "oscillator_module": OscillatorModule("osc_1") with its controls.
// Ports: port/crates/oscillators (oscillator_module).
//
// params.wavetable: see module_content.h (rendered after init(), as a
// preset load does); params.controls: {"osc_1_...": value}.
//
// Module "oscillator_module_pitch" runs the same module and captures its
// oscillator's per-sample phase increment for the block (phase_inc_buffer_,
// frequency * kPhaseMult / sampleRate) times 1 / kPhaseMult, the normalized
// pitch, so the transpose/tune/snapping controls are compared without the
// phase drift the audio accumulates (see synth_oscillator.cpp and LEDGER.md).

#include "module_registry.h"

#include "module_content.h"
#include "oscillator_module.h"
#include "synth_oscillator.h"

#include <memory>

namespace {

using refmod::Rate;
using M = vital::OscillatorModule;
using O = vital::SynthOscillator;

constexpr int kPhaseIncOutput = M::kNumOutputs;

// Access to the private SynthOscillator::phase_inc_buffer_ through an
// explicit template instantiation, which C++ exempts from access checks.
struct PhaseIncBufferTag {
  typedef std::shared_ptr<vital::Output> O::*type;
  friend type get(PhaseIncBufferTag);
};

template <typename Tag, typename Tag::type kMember>
struct PrivateMember {
  friend typename Tag::type get(Tag) { return kMember; }
};

template struct PrivateMember<PhaseIncBufferTag, &O::phase_inc_buffer_>;

class OscillatorModulePitchProbe : public M {
  public:
    OscillatorModulePitchProbe() :
        M("osc_1"), phase_inc_(std::make_shared<vital::Output>(vital::kMaxBufferSize, vital::kMaxOversample)) {
      registerOutput(phase_inc_.get());
    }

    void process(int num_samples) override {
      M::process(num_samples);
      const vital::poly_float* increments = (oscillator()->*get(PhaseIncBufferTag()))->buffer;
      for (int i = 0; i < num_samples; ++i)
        phase_inc_->buffer[i] = increments[i] * (1.0f / 4294967296.0f);
    }

  private:
    std::shared_ptr<vital::Output> phase_inc_;
};

template <class Module>
vital::Processor* createModule(const nlohmann::json& params) {
  Module* module = new Module();
  module->init();
  try {
    if (!module_content::renderWavetable(module->getWavetable(),
                                         params.value("wavetable", nlohmann::json::object())) ||
        !module_content::setControls(module, params.value("controls", nlohmann::json::object()))) {
      delete module;
      return nullptr;
    }
  }
  catch (const std::exception&) {
    delete module;
    return nullptr;
  }
  return module;
}

struct OscillatorModuleOsc1 : M {
  OscillatorModuleOsc1() : M("osc_1") { }
};

const std::vector<refmod::InputDef> kInputs = {
  { "reset", M::kReset, 0.0f, Rate::kTrigger },
  { "retrigger", M::kRetrigger, 0.0f, Rate::kTrigger },
  { "midi", M::kMidi, 60.0f, Rate::kControl },
  { "active_voices", M::kActiveVoices, 1.0f, Rate::kControl },
};

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "oscillator_module",
  "OscillatorModule(\"osc_1\"): params.wavetable ({preset} or {creator}), params.controls {name: value}",
  createModule<OscillatorModuleOsc1>,
  kInputs,
  { { "raw", M::kRaw }, { "levelled", M::kLevelled } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "oscillator_module_pitch",
  "OscillatorModule as in oscillator_module; output 'phase_inc' is its oscillator's per-sample phase "
  "increment / 2^32 (frequency / sample rate)",
  createModule<OscillatorModulePitchProbe>,
  kInputs,
  { { "phase_inc", kPhaseIncOutput } },
});
