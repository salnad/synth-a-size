// Module "random_lfo": RandomLfo (perlin, sample and hold, sine, Lorenz). Ports: port/crates/modulation.

#include "module_registry.h"

#include "random_lfo.h"

using refmod::Rate;

namespace {
  // With params.clock_start_seconds, calls correctToTime(start + frames / rate)
  // before each block, as the engine does every block (tempo sync follows it).
  class ClockedRandomLfo : public vital::RandomLfo {
    public:
      ClockedRandomLfo(bool clocked, double start) : clocked_(clocked), start_(start), frames_(0) { }
      vital::Processor* clone() const override { return new ClockedRandomLfo(*this); }

      void process(int num_samples) override {
        if (clocked_)
          correctToTime(start_ + static_cast<double>(frames_) / getSampleRate());
        vital::RandomLfo::process(num_samples);
        frames_ += num_samples;
      }

    private:
      bool clocked_;
      double start_;
      long long frames_;
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "random_lfo",
  "RandomLfo. Set the spec seed. params: control_rate: bool, clock_start_seconds (calls correctToTime "
  "before each block), use_clone: bool (capture a clone(), which takes a new seed)",
  [](const nlohmann::json& params) -> vital::Processor* {
    vital::RandomLfo* lfo = new ClockedRandomLfo(params.count("clock_start_seconds") > 0,
                                                 params.value("clock_start_seconds", 0.0));
    if (params.value("use_clone", false)) {
      vital::Processor* copy = lfo->clone();
      delete lfo;
      lfo = static_cast<vital::RandomLfo*>(copy);
    }
    lfo->setControlRate(params.value("control_rate", false));
    return lfo;
  },
  {
    { "frequency", vital::RandomLfo::kFrequency, 1.0f, Rate::kControl },
    { "reset", vital::RandomLfo::kReset, 0.0f, Rate::kTrigger },
    { "sync", vital::RandomLfo::kSync, 0.0f, Rate::kControl },
    { "style", vital::RandomLfo::kStyle, 0.0f, Rate::kControl },
    { "stereo", vital::RandomLfo::kStereo, 0.0f, Rate::kControl },
  },
  { { "value", 0 } },
});
