// Modules "oversampling_round_trip" and "fir_halfband_decimator": the
// up- and downsamplers. Ports: port/crates/filters (upsampler.rs,
// decimator.rs, iir_halfband_decimator.rs, fir_halfband_decimator.rs).
//
// The decimators read twice as many input samples as they write, and the
// Decimator picks its stage count from its input owner's sample rate, so
// both are driven inside small probes that run at the case's internal rate
// (sample_rate * oversample) and hold each decimated sample for
// `oversample` frames, as the Upsampler does.

#include "module_registry.h"

#include "decimator.h"
#include "fir_halfband_decimator.h"
#include "processor_router.h"
#include "upsampler.h"

using refmod::Rate;

namespace {
  class Passthrough : public vital::Processor {
    public:
      Passthrough() : vital::Processor(1, 1) { }

      vital::Processor* clone() const override { return new Passthrough(*this); }

      void process(int num_samples) override {
        const vital::poly_float* source = input(0)->source->buffer;
        vital::poly_float* dest = output(0)->buffer;
        for (int i = 0; i < num_samples; ++i)
          dest[i] = source[i];
      }
  };

  // Passthrough (oversampled) -> Decimator(max_stages) -> Upsampler, wired as
  // SoundEngine (decimator after the oversampled output) and EffectsEngine
  // (an idle Upsampler driven with processWithInput) wire them.
  class RoundTripProbe : public vital::ProcessorRouter {
    public:
      explicit RoundTripProbe(int max_stages) : vital::ProcessorRouter(1, 1) {
        source_ = new Passthrough();
        decimator_ = new vital::Decimator(max_stages);
        upsampler_ = new vital::Upsampler();

        source_->useInput(input(0), 0);
        decimator_->plug(source_, vital::Decimator::kAudio);
        upsampler_->useOutput(output(0), 0);

        addProcessor(source_);
        addProcessor(decimator_);
        addIdleProcessor(upsampler_);
        decimator_->init();
      }

      vital::Processor* clone() const override { VITAL_ASSERT(false); return nullptr; }

      void process(int num_samples) override {
        vital::ProcessorRouter::process(num_samples);
        upsampler_->processWithInput(decimator_->output()->buffer, num_samples / getOversampleAmount());
      }

    private:
      Passthrough* source_;
      vital::Decimator* decimator_;
      vital::Upsampler* upsampler_;
  };

  // FirHalfbandDecimator on the probe's (2x) input; output held for 2 frames.
  class FirProbe : public vital::Processor {
    public:
      FirProbe() : vital::Processor(1, 1) {
        fir_ = new vital::FirHalfbandDecimator();
        fir_->useInput(input(0), vital::FirHalfbandDecimator::kAudio);
      }

      vital::Processor* clone() const override { VITAL_ASSERT(false); return nullptr; }

      void reset(vital::poly_mask reset_mask) override {
        static_cast<vital::Processor*>(fir_)->reset(reset_mask);
      }

      void process(int num_samples) override {
        int decimated = num_samples / 2;
        fir_->process(decimated);
        const vital::poly_float* fir_out = fir_->output()->buffer;
        vital::poly_float* dest = output(0)->buffer;
        for (int i = 0; i < num_samples; ++i)
          dest[i] = fir_out[i / 2];
      }

    private:
      vital::FirHalfbandDecimator* fir_;
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "oversampling_round_trip",
  "Passthrough at the internal rate -> Decimator(max_stages) -> Upsampler. params: max_stages (default 3)",
  [](const nlohmann::json& params) -> vital::Processor* {
    return new RoundTripProbe(params.value("max_stages", 3));
  },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "audio", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "fir_halfband_decimator",
  "FirHalfbandDecimator at 2x (oversample must be 2, block_size > 32); output held for 2 frames",
  [](const nlohmann::json&) -> vital::Processor* { return new FirProbe(); },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "audio", 0 } },
});
