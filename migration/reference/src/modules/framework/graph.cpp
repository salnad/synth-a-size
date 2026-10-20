// Modules "graph_*": small ProcessorRouter graphs that check routing order,
// feedback insertion and ModulationSum against upstream's processor_router.
// Ports: port/crates/dsp-core (tests/parity.rs builds the same graphs).
//
// Router inputs reach the graph through Passthrough nodes (a plain Processor
// that copies its input), because ref-module's sources have no owner and
// upstream dereferences source owners (ModulationSum) and only enables
// operators whose inputs were connected when they were plugged.

#include "module_registry.h"

#include "operators.h"
#include "processor_router.h"
#include "value.h"

using refmod::Rate;

namespace {
  class Passthrough : public vital::Processor {
    public:
      explicit Passthrough(bool control_rate) : vital::Processor(1, 1, control_rate) { }

      vital::Processor* clone() const override { return new Passthrough(*this); }

      void process(int num_samples) override {
        const vital::poly_float* source = input(0)->source->buffer;
        vital::poly_float* dest = output(0)->buffer;
        int samples = isControlRate() ? 1 : num_samples;
        for (int i = 0; i < samples; ++i)
          dest[i] = source[i];
      }
  };

  // ModulationSum fed by one control-rate and one audio-rate source.
  vital::Processor* createModulationSumGraph(const nlohmann::json&) {
    vital::ProcessorRouter* router = new vital::ProcessorRouter(3, 1);
    Passthrough* control = new Passthrough(true);
    Passthrough* audio = new Passthrough(false);
    control->useInput(router->input(1), 0);
    audio->useInput(router->input(2), 0);

    vital::ModulationSum* sum = new vital::ModulationSum();
    sum->useInput(router->input(0), vital::ModulationSum::kReset);
    sum->plugNext(control);
    sum->plugNext(audio);
    sum->useOutput(router->output(0), 0);

    router->addProcessor(control);
    router->addProcessor(audio);
    router->addProcessor(sum);
    return router;
  }

  // Control-rate cycle add -> multiply -> add, closed after the processors are
  // added, so the router inserts a cr::Feedback. Processors are added in
  // reverse dependency order: the router keeps its insertion order.
  vital::Processor* createFeedbackCycleGraph(const nlohmann::json& params) {
    vital::ProcessorRouter* router = new vital::ProcessorRouter(1, 1, true);
    Passthrough* in = new Passthrough(true);
    Passthrough* out = new Passthrough(true);
    vital::cr::Add* add = new vital::cr::Add();
    vital::cr::Multiply* multiply = new vital::cr::Multiply();
    vital::cr::Value* gain = new vital::cr::Value(params.value("gain", 0.5f));

    in->useInput(router->input(0), 0);
    out->useOutput(router->output(0), 0);

    router->addProcessor(out);
    router->addProcessor(multiply);
    router->addProcessor(add);
    router->addProcessor(in);
    router->addIdleProcessor(gain);

    add->plug(in, 0);
    multiply->plug(add, 0);
    multiply->plug(gain, 1);
    add->plug(multiply, 1);
    out->plug(add, 0);
    return router;
  }

  // Audio-rate chain added in reverse order: in -> negate -> scale -> add(scale, in) -> out.
  vital::Processor* createAudioChainGraph(const nlohmann::json& params) {
    vital::ProcessorRouter* router = new vital::ProcessorRouter(1, 1);
    Passthrough* in = new Passthrough(false);
    Passthrough* out = new Passthrough(false);
    vital::Negate* negate = new vital::Negate();
    vital::LinearScale* scale = new vital::LinearScale(params.value("scale", 0.5f));
    vital::Add* add = new vital::Add();

    in->useInput(router->input(0), 0);
    out->useOutput(router->output(0), 0);

    router->addProcessor(out);
    router->addProcessor(add);
    router->addProcessor(scale);
    router->addProcessor(negate);
    router->addProcessor(in);

    negate->plug(in, 0);
    scale->plug(negate, 0);
    add->plug(scale, 0);
    add->plug(in, 1);
    out->plug(add, 0);
    return router;
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "graph_modulation_sum", "ProcessorRouter: ModulationSum(reset, control source, audio source)",
  createModulationSumGraph,
  {
    { "reset", 0, 0.0f, Rate::kTrigger },
    { "control", 1, 0.0f, Rate::kControl },
    { "audio", 2, 0.0f, Rate::kAudio },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "graph_feedback_cycle", "Control-rate ProcessorRouter with an add/multiply cycle (cr::Feedback); param gain",
  createFeedbackCycleGraph,
  { { "in", 0, 0.0f, Rate::kControl } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "graph_audio_chain", "Audio-rate ProcessorRouter chain added in reverse order; param scale",
  createAudioChainGraph,
  { { "in", 0, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});
