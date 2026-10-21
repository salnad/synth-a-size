// Probe modules for the framework classes that are not processors:
// Memory / StereoMemory (memory.h), OneDimLookup (lookup_table.h) and
// OnePoleFilter (one_pole_filter.h). Ports: port/crates/dsp-core
// (tests/parity.rs mirrors these probes by name).

#include "module_registry.h"

#include "futils.h"
#include "lookup_table.h"
#include "memory.h"
#include "one_pole_filter.h"
#include "processor.h"

#include <cmath>
#include <memory>

using refmod::Rate;

namespace {
  // Push the input, then read `delay` samples back.
  template<class MemoryType>
  class MemoryProbe : public vital::Processor {
    public:
      explicit MemoryProbe(int size) : vital::Processor(2, 1), memory_(std::make_shared<MemoryType>(size)) { }

      vital::Processor* clone() const override { return new MemoryProbe(*this); }

      void process(int num_samples) override {
        const vital::poly_float* audio = input(0)->source->buffer;
        const vital::poly_float* delay = input(1)->source->buffer;
        vital::poly_float* dest = output(0)->buffer;
        for (int i = 0; i < num_samples; ++i) {
          memory_->push(audio[i]);
          dest[i] = memory_->get(delay[i]);
        }
      }

    private:
      std::shared_ptr<MemoryType> memory_;
  };

  vital::mono_float lookupSin(vital::mono_float t) { return sinf(t); }

  constexpr int kLookupResolution = 256;
  typedef vital::OneDimLookup<lookupSin, kLookupResolution> SinLookup;

  class LookupProbe : public vital::Processor {
    public:
      explicit LookupProbe(float scale) : vital::Processor(1, 1), lookup_(std::make_shared<SinLookup>(scale)) { }

      vital::Processor* clone() const override { return new LookupProbe(*this); }

      void process(int num_samples) override {
        const vital::poly_float* source = input(0)->source->buffer;
        vital::poly_float* dest = output(0)->buffer;
        for (int i = 0; i < num_samples; ++i)
          dest[i] = lookup_->cubicLookup(source[i]);
      }

    private:
      std::shared_ptr<SinLookup> lookup_;
  };

  // mode 0 = tickBasic, 1 = tick, 2 = tickDerivative.
  template<class Filter>
  class OnePoleProbe : public vital::Processor {
    public:
      explicit OnePoleProbe(int mode) : vital::Processor(2, 1), mode_(mode) { }

      vital::Processor* clone() const override { return new OnePoleProbe(*this); }

      void reset(vital::poly_mask reset_mask) override { filter_.reset(reset_mask); }

      void process(int num_samples) override {
        const vital::poly_float* audio = input(0)->source->buffer;
        const vital::poly_float* cutoff = input(1)->source->buffer;
        vital::poly_float* dest = output(0)->buffer;
        for (int i = 0; i < num_samples; ++i) {
          vital::poly_float coefficient = Filter::computeCoefficient(cutoff[i], getSampleRate());
          if (mode_ == 0)
            dest[i] = filter_.tickBasic(audio[i], coefficient);
          else if (mode_ == 1)
            dest[i] = filter_.tick(audio[i], coefficient);
          else
            dest[i] = filter_.tickDerivative(audio[i], coefficient);
        }
      }

    private:
      Filter filter_;
      int mode_;
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "memory_probe", "Memory(size): push audio, output get(delay); param size",
  [](const nlohmann::json& p) -> vital::Processor* { return new MemoryProbe<vital::Memory>(p.value("size", 1024)); },
  { { "audio", 0, 0.0f, Rate::kAudio }, { "delay", 1, 2.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "stereo_memory_probe", "StereoMemory(size): push audio, output get(delay); param size",
  [](const nlohmann::json& p) -> vital::Processor* {
    return new MemoryProbe<vital::StereoMemory>(p.value("size", 1024));
  },
  { { "audio", 0, 0.0f, Rate::kAudio }, { "delay", 1, 2.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "lookup_probe", "OneDimLookup<sinf, 256>(scale).cubicLookup(in); param scale",
  [](const nlohmann::json& p) -> vital::Processor* { return new LookupProbe(p.value("scale", 1.0f)); },
  { { "in", 0, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "one_pole_probe",
  "OnePoleFilter<saturate>: params mode (0 tickBasic, 1 tick, 2 tickDerivative), saturate (0 pass, 1 futils::tanh)",
  [](const nlohmann::json& p) -> vital::Processor* {
    int mode = p.value("mode", 0);
    if (p.value("saturate", 0))
      return new OnePoleProbe<vital::OnePoleFilter<vital::futils::tanh>>(mode);
    return new OnePoleProbe<vital::OnePoleFilter<>>(mode);
  },
  { { "audio", 0, 0.0f, Rate::kAudio }, { "cutoff", 1, 1000.0f, Rate::kAudio } },
  { { "out", 0 } },
});
