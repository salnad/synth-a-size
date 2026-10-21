// Module "producers_module": ProducersModule (three oscillators and the
// sampler, FM/RM order, destination routing).
// Ports: port/crates/oscillators (producers_module).
//
// params.wavetables: {"osc_1": spec, "osc_2": spec, "osc_3": spec} (see
// module_content.h; missing ones keep the default wavetable), params.sample
// (optional, see module_content.h), params.controls: {name: value}. All are
// applied after init(), as a preset load does.
//
// params.filter1_on / params.filter2_on (optional): a Value plugged in
// through setFilter1On / setFilter2On, as the engine plugs its filter_N_on
// controls. Without them the filters count as on (the pointers stay null).
//
// params.control_changes (optional): [{"sample": n, "name": control,
// "value": v}], applied before the first block that starts at or after
// internal-rate frame n. "name" is a module control, or filter1_on /
// filter2_on when that param is set.
//
// Output "sample_phase" is samplePhaseOutput() (the sampler's phase output),
// copied after each block.

#include "module_registry.h"

#include "module_content.h"
#include "producers_module.h"

#include <algorithm>
#include <map>
#include <memory>
#include <vector>

namespace {

using refmod::Rate;
using M = vital::ProducersModule;

constexpr int kSamplePhaseOutput = M::kNumOutputs;

bool loadContent(M* module, const nlohmann::json& params) {
  const nlohmann::json wavetables = params.value("wavetables", nlohmann::json::object());
  for (int i = 0; i < vital::kNumOscillators; ++i) {
    std::string name = "osc_" + std::to_string(i + 1);
    if (wavetables.count(name) && !module_content::renderWavetable(module->getWavetable(i), wavetables[name]))
      return false;
  }
  if (params.count("sample"))
    module_content::loadSample(module->getSample(), params["sample"]);
  return module_content::setControls(module, params.value("controls", nlohmann::json::object()));
}

class ProducersProbe : public M {
  public:
    ProducersProbe() : sample_phase_(std::make_shared<vital::cr::Output>()), frame_(0) {
      registerOutput(sample_phase_.get());
    }

    bool configure(const nlohmann::json& params) {
      std::map<std::string, vital::Value*> targets;
      for (auto& control : getControls())
        targets[control.first] = control.second;

      const char* names[] = { "filter1_on", "filter2_on" };
      for (int i = 0; i < 2; ++i) {
        if (params.count(names[i]) == 0)
          continue;
        filter_on_[i] = std::make_unique<vital::Value>(params[names[i]].get<float>());
        targets[names[i]] = filter_on_[i].get();
      }
      setFilter1On(filter_on_[0].get());
      setFilter2On(filter_on_[1].get());

      for (const auto& change : params.value("control_changes", nlohmann::json::array())) {
        std::string name = change.at("name").get<std::string>();
        if (targets.count(name) == 0)
          return false;
        changes_.push_back({ change.at("sample").get<int>(), targets[name], change.at("value").get<float>() });
      }
      std::stable_sort(changes_.begin(), changes_.end(),
                       [](const Change& a, const Change& b) { return a.frame < b.frame; });
      return true;
    }

    void process(int num_samples) override {
      for (; next_change_ < changes_.size() && changes_[next_change_].frame <= frame_; ++next_change_)
        changes_[next_change_].target->set(changes_[next_change_].value);
      M::process(num_samples);
      sample_phase_->buffer[0] = samplePhaseOutput()->buffer[0];
      frame_ += num_samples;
    }

  private:
    struct Change {
      int frame;
      vital::Value* target;
      float value;
    };

    std::shared_ptr<vital::cr::Output> sample_phase_;
    std::unique_ptr<vital::Value> filter_on_[2];
    std::vector<Change> changes_;
    size_t next_change_ = 0;
    int frame_;
};

} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "producers_module",
  "ProducersModule: params.wavetables {osc_N: spec}, params.sample, params.controls {name: value}, "
  "params.filter1_on / filter2_on (Values plugged into setFilterNOn), params.control_changes "
  "[{sample, name, value}] applied at block starts; output 'sample_phase' is samplePhaseOutput()",
  [](const nlohmann::json& params) -> vital::Processor* {
    ProducersProbe* module = new ProducersProbe();
    module->init();
    try {
      if (!loadContent(module, params) || !module->configure(params)) {
        delete module;
        return nullptr;
      }
    }
    catch (const std::exception&) {
      delete module;
      return nullptr;
    }
    return module;
  },
  {
    { "reset", M::kReset, 0.0f, Rate::kTrigger },
    { "retrigger", M::kRetrigger, 0.0f, Rate::kTrigger },
    { "midi", M::kMidi, 60.0f, Rate::kControl },
    { "active_voices", M::kActiveVoices, 1.0f, Rate::kControl },
    { "note_count", M::kNoteCount, 1.0f, Rate::kControl },
  },
  { { "to_filter1", M::kToFilter1 }, { "to_filter2", M::kToFilter2 },
    { "raw_out", M::kRawOut }, { "direct_out", M::kDirectOut },
    { "sample_phase", kSamplePhaseOutput } },
});
