// Module "sample_source": SampleSource (the sample player) and Sample.
// Ports: port/crates/oscillators (sample_source).
//
// Without params.sample the player keeps its constructor's sample: 44100
// samples of seeded white noise. params.sample loads a sample through
// Sample::jsonToState, as a preset does:
//   {"length": n, "sample_rate": hz, "recipe": {...}, "recipe_stereo": {...}}
// The recipes are ../wavetable/wave_recipes.h waveforms, PCM-encoded with
// the given length ("samples" / "samples_stereo" in the preset JSON).
//
// Output "phase" is SampleSource::getPhaseOutput() (the encoded phase and
// voice that the sample-phase modulation source reads), copied after each
// block.
//
// Module "sample_source_pitch" runs the same player and captures its
// playback increment for the block (the private phase_inc_, upsampled
// sample positions per output sample) divided by upsampleLength(), the
// active length outside process() (which marks the sample unused): the
// increment in sample lengths per output sample, normalized like the phase
// output, so playback ratios are compared without the position drift the
// audio accumulates (see LEDGER.md, M4b SampleSource).

#include "module_registry.h"

#include "../wavetable/wave_recipes.h"
#include "sample_source.h"

#include <memory>

namespace {

using refmod::Rate;
using S = vital::SampleSource;

constexpr int kPhaseOutput = S::kNumOutputs;
constexpr int kPhaseIncOutput = S::kNumOutputs;

// Access to the private SampleSource::phase_inc_ through an explicit
// template instantiation, which C++ exempts from access checks.
struct PhaseIncTag {
  typedef vital::poly_float S::*type;
  friend type get(PhaseIncTag);
};

template <typename Tag, typename Tag::type kMember>
struct PrivateMember {
  friend typename Tag::type get(Tag) { return kMember; }
};

template struct PrivateMember<PhaseIncTag, &S::phase_inc_>;

nlohmann::json sampleJson(const nlohmann::json& spec) {
  int length = spec.at("length").get<int>();
  nlohmann::json data;
  data["name"] = "S";
  data["length"] = length;
  data["sample_rate"] = spec.at("sample_rate").get<int>();
  nlohmann::json recipe = spec.at("recipe");
  recipe["length"] = length;
  recipe["encoding"] = "pcm";
  data["samples"] = wave_recipes::encode(recipe);
  if (spec.count("recipe_stereo")) {
    nlohmann::json stereo = spec.at("recipe_stereo");
    stereo["length"] = length;
    stereo["encoding"] = "pcm";
    data["samples_stereo"] = wave_recipes::encode(stereo);
  }
  return data;
}

class SampleSourceProbe : public S {
  public:
    SampleSourceProbe() : phase_(std::make_shared<vital::cr::Output>()) {
      registerOutput(phase_.get());
    }

    void process(int num_samples) override {
      S::process(num_samples);
      output(kPhaseOutput)->buffer[0] = getPhaseOutput()->buffer[0];
    }

  private:
    std::shared_ptr<vital::cr::Output> phase_;
};

class SampleSourcePitchProbe : public S {
  public:
    SampleSourcePitchProbe() : increment_(std::make_shared<vital::cr::Output>()) {
      registerOutput(increment_.get());
    }

    void process(int num_samples) override {
      S::process(num_samples);
      increment_->buffer[0] = this->*get(PhaseIncTag()) * (1.0f / getSample()->upsampleLength());
    }

  private:
    std::shared_ptr<vital::cr::Output> increment_;
};

template <class Probe>
vital::Processor* createProbe(const nlohmann::json& params) {
  Probe* source = new Probe();
  if (params.count("sample")) {
    try {
      source->getSample()->jsonToState(sampleJson(params["sample"]));
    }
    catch (const std::exception&) {
      delete source;
      return nullptr;
    }
  }
  return source;
}

const std::vector<refmod::InputDef> kInputs = {
  { "reset", S::kReset, 0.0f, Rate::kTrigger },
  { "midi", S::kMidi, 60.0f, Rate::kControl },
  { "keytrack", S::kKeytrack, 1.0f, Rate::kControl },
  { "level", S::kLevel, 1.0f, Rate::kAudio },
  { "random_phase", S::kRandomPhase, 0.0f, Rate::kControl },
  { "transpose", S::kTranspose, 0.0f, Rate::kControl },
  { "transpose_quantize", S::kTransposeQuantize, 0.0f, Rate::kControl },
  { "tune", S::kTune, 0.0f, Rate::kControl },
  { "loop", S::kLoop, 1.0f, Rate::kControl },
  { "bounce", S::kBounce, 0.0f, Rate::kControl },
  { "pan", S::kPan, 0.0f, Rate::kControl },
  { "note_count", S::kNoteCount, 1.0f, Rate::kControl },
};

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "sample_source",
  "SampleSource; params.sample {length, sample_rate, recipe, recipe_stereo} loads a sample through "
  "Sample::jsonToState (default: the constructor's white noise); output 'phase' is the phase output",
  createProbe<SampleSourceProbe>,
  kInputs,
  { { "raw", S::kRaw }, { "levelled", S::kLevelled }, { "phase", kPhaseOutput } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "sample_source_pitch",
  "SampleSource as in sample_source; output 'phase_inc' is the block's playback increment / upsampleLength()",
  createProbe<SampleSourcePitchProbe>,
  kInputs,
  { { "phase_inc", kPhaseIncOutput } },
});
