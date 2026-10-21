// Module "synth_oscillator": SynthOscillator on a rendered wavetable.
// Ports: port/crates/oscillators (synth_oscillator).
//
// params.wavetable: {"preset": "init" | "predefined"} or
// {"creator": "<creator JSON>"} ({"$wave": ...} recipes expanded, see
// ../wavetable/wave_recipes.h). The wavetable is rendered before the
// oscillator is constructed, as an engine loads a preset before voices run.
//
// The extra audio input "modulation" is the buffer the FM/RM distortion
// types read: it is set as the first and second oscillator output and as the
// sample output (ProducersModule wires those to the other oscillators and to
// the sampler).
//
// Module "synth_oscillator_pitch" runs the same oscillator and captures the
// per-sample phase increment it computed for the block (phase_inc_buffer_,
// frequency * kPhaseMult / sampleRate) times 1 / kPhaseMult: the consumer's
// normalized form of the pitch (frequency / sample rate), so transpose,
// tune, snapping and pitch sweeps are compared without the phase drift the
// audio output accumulates (see LEDGER.md, M4b SynthOscillator).

#include "module_registry.h"

#include "../wavetable/wave_recipes.h"
#include "synth_constants.h"
#include "synth_oscillator.h"
#include "wavetable.h"
#include "wavetable_creator.h"

#include <memory>

namespace {

using refmod::Rate;
using O = vital::SynthOscillator;

constexpr int kModulationInput = O::kNumInputs;
constexpr int kPhaseIncOutput = O::kNumOutputs;

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

std::unique_ptr<vital::Wavetable> renderWavetable(const nlohmann::json& params) {
  auto wavetable = std::make_unique<vital::Wavetable>(vital::kNumOscillatorWaveFrames);
  WavetableCreator creator(wavetable.get());
  const nlohmann::json spec = params.value("wavetable", nlohmann::json::object());
  if (spec.count("creator"))
    creator.jsonToState(wave_recipes::expand(nlohmann::json::parse(spec["creator"].get<std::string>())));
  else if (spec.value("preset", std::string("init")) == "init")
    creator.init();
  else if (spec.value("preset", std::string()) == "predefined")
    creator.initPredefinedWaves();
  else
    return nullptr;
  return wavetable;
}

struct WavetableHolder {
  explicit WavetableHolder(std::unique_ptr<vital::Wavetable> table) : wavetable(std::move(table)) { }
  std::unique_ptr<vital::Wavetable> wavetable;
};

class OscillatorProbe : private WavetableHolder, public O {
  public:
    explicit OscillatorProbe(std::unique_ptr<vital::Wavetable> table) :
        WavetableHolder(std::move(table)), O(wavetable.get()) {
      addInput();
    }

    void process(int num_samples) override {
      const vital::Output* modulation = input(kModulationInput)->source;
      setFirstOscillatorOutput(const_cast<vital::Output*>(modulation));
      setSecondOscillatorOutput(const_cast<vital::Output*>(modulation));
      setSampleOutput(const_cast<vital::Output*>(modulation));
      O::process(num_samples);
    }
};

class OscillatorPitchProbe : public OscillatorProbe {
  public:
    explicit OscillatorPitchProbe(std::unique_ptr<vital::Wavetable> table) :
        OscillatorProbe(std::move(table)),
        phase_inc_(std::make_shared<vital::Output>(vital::kMaxBufferSize, vital::kMaxOversample)) {
      registerOutput(phase_inc_.get());
    }

    void process(int num_samples) override {
      OscillatorProbe::process(num_samples);
      const vital::poly_float* increments = (this->*get(PhaseIncBufferTag()))->buffer;
      for (int i = 0; i < num_samples; ++i)
        phase_inc_->buffer[i] = increments[i] * (1.0f / 4294967296.0f);
    }

  private:
    std::shared_ptr<vital::Output> phase_inc_;
};

template <class Probe>
vital::Processor* createProbe(const nlohmann::json& params) {
  std::unique_ptr<vital::Wavetable> wavetable;
  try {
    wavetable = renderWavetable(params);
  }
  catch (const std::exception&) {
    return nullptr;
  }
  if (!wavetable)
    return nullptr;
  return new Probe(std::move(wavetable));
}

const std::vector<refmod::InputDef> kInputs = {
  { "wave_frame", O::kWaveFrame, 0.0f, Rate::kControl },
  { "midi_note", O::kMidiNote, 60.0f, Rate::kControl },
  { "midi_track", O::kMidiTrack, 1.0f, Rate::kControl },
  { "smooth_interpolate", O::kSmoothlyInterpolate, 0.0f, Rate::kControl },
  { "transpose", O::kTranspose, 0.0f, Rate::kAudio },
  { "transpose_quantize", O::kTransposeQuantize, 0.0f, Rate::kControl },
  { "tune", O::kTune, 0.0f, Rate::kAudio },
  { "amplitude", O::kAmplitude, 1.0f, Rate::kAudio },
  { "pan", O::kPan, 0.0f, Rate::kControl },
  { "unison_voices", O::kUnisonVoices, 1.0f, Rate::kControl },
  { "unison_detune", O::kUnisonDetune, 0.0f, Rate::kControl },
  { "phase", O::kPhase, 0.5f, Rate::kAudio },
  { "distortion_phase", O::kDistortionPhase, 0.5f, Rate::kControl },
  { "random_phase", O::kRandomPhase, 0.0f, Rate::kControl },
  { "blend", O::kBlend, 0.8f, Rate::kControl },
  { "stereo_spread", O::kStereoSpread, 1.0f, Rate::kControl },
  { "stack_style", O::kStackStyle, 0.0f, Rate::kControl },
  { "detune_power", O::kDetunePower, 1.5f, Rate::kControl },
  { "detune_range", O::kDetuneRange, 2.0f, Rate::kControl },
  { "frame_spread", O::kUnisonFrameSpread, 0.0f, Rate::kControl },
  { "distortion_spread", O::kUnisonDistortionSpread, 0.0f, Rate::kControl },
  { "spectral_morph_spread", O::kUnisonSpectralMorphSpread, 0.0f, Rate::kControl },
  { "spectral_morph_type", O::kSpectralMorphType, 0.0f, Rate::kControl },
  { "spectral_morph_amount", O::kSpectralMorphAmount, 0.5f, Rate::kControl },
  { "spectral_unison", O::kSpectralUnison, 1.0f, Rate::kControl },
  { "distortion_type", O::kDistortionType, 0.0f, Rate::kControl },
  { "distortion_amount", O::kDistortionAmount, 0.5f, Rate::kControl },
  { "active_voices", O::kActiveVoices, 1.0f, Rate::kControl },
  { "reset", O::kReset, 0.0f, Rate::kTrigger },
  { "retrigger", O::kRetrigger, 0.0f, Rate::kTrigger },
  { "modulation", kModulationInput, 0.0f, Rate::kAudio },
};

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "synth_oscillator",
  "SynthOscillator on params.wavetable ({preset: init|predefined} or {creator: JSON}); "
  "input 'modulation' feeds the FM/RM distortion types",
  createProbe<OscillatorProbe>,
  kInputs,
  { { "raw", O::kRaw }, { "levelled", O::kLevelled } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "synth_oscillator_pitch",
  "SynthOscillator as in synth_oscillator; output 'phase_inc' is the block's per-sample phase increment "
  "/ 2^32 (frequency / sample rate)",
  createProbe<OscillatorPitchProbe>,
  kInputs,
  { { "phase_inc", kPhaseIncOutput } },
});
