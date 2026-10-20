// Module "wavetable_creator_json": WavetableCreator load -> save, the exact
// wavetable JSON presets embed. Artifact-only. Ports: port/crates/wavetable
// (creator).
//
// params.creator: the creator JSON as a string ({"$wave": ...} recipes are
// expanded first, see wave_recipes.h). Texts saved.json
// (jsonToState -> stateToJson().dump()) and upgraded.json
// (updateJson(input).dump()).
// params.preset: "init" (init()) or "predefined" (initPredefinedWaves() named
// "Sub", as LoadSave's 0.5.0 upgrade builds it). Their keyframe waveforms are
// computed (not loaded), and the two reference builds differ in their last
// bits, so these emit the keyframes' time-domain samples as array
// keyframe_waves (compared with tolerances); params.saved_json = true also
// emits their saved.json (used by the pending cases, see LEDGER.md).

#include "module_registry.h"

#include "wave_recipes.h"
#include "wave_source.h"
#include "wavetable.h"
#include "wavetable_creator.h"
#include "synth_constants.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "wavetable_creator_json",
  "WavetableCreator jsonToState(params.creator) -> stateToJson: texts saved.json and upgraded.json. "
  "params.preset init/predefined: array keyframe_waves",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    vital::Wavetable wavetable(vital::kNumOscillatorWaveFrames);
    WavetableCreator creator(&wavetable);
    try {
      if (params.count("creator")) {
        nlohmann::json input = wave_recipes::expand(nlohmann::json::parse(params["creator"].get<std::string>()));
        sink.text("upgraded.json", creator.updateJson(input).dump());
        creator.jsonToState(input);
      }
      else if (params.count("preset")) {
        if (params["preset"] == "init")
          creator.init();
        else {
          creator.initPredefinedWaves();
          creator.setName("Sub");
        }
        WaveSource* source = dynamic_cast<WaveSource*>(creator.getGroup(0)->getComponent(0));
        std::vector<float> waves;
        for (int k = 0; k < source->numFrames(); ++k) {
          const float* time_domain = source->getWaveFrame(k)->time_domain;
          waves.insert(waves.end(), time_domain, time_domain + vital::WaveFrame::kWaveformSize);
        }
        sink.array("keyframe_waves", waves, 1);
        if (params.value("saved_json", false))
          sink.text("saved.json", creator.stateToJson().dump());
        return true;
      }
      else {
        error = "params needs creator or preset";
        return false;
      }
    }
    catch (const std::exception& e) {
      error = std::string("creator JSON: ") + e.what();
      return false;
    }
    sink.text("saved.json", creator.stateToJson().dump());
    return true;
  },
});
