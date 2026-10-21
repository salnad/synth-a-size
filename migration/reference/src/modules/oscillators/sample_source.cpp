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

#include "module_registry.h"

#include "../wavetable/wave_recipes.h"
#include "sample_source.h"

#include <memory>

namespace {

using refmod::Rate;
using S = vital::SampleSource;

constexpr int kPhaseOutput = S::kNumOutputs;

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

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "sample_source",
  "SampleSource; params.sample {length, sample_rate, recipe, recipe_stereo} loads a sample through "
  "Sample::jsonToState (default: the constructor's white noise); output 'phase' is the phase output",
  [](const nlohmann::json& params) -> vital::Processor* {
    SampleSourceProbe* source = new SampleSourceProbe();
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
  },
  {
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
  },
  { { "raw", S::kRaw }, { "levelled", S::kLevelled }, { "phase", kPhaseOutput } },
});
