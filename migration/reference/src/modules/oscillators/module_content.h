// Test content for the oscillator module captures: wavetables and samples
// described by case params, and parameter values set on module controls.
#pragma once

#include "../wavetable/wave_recipes.h"
#include "sample_source.h"
#include "synth_module.h"
#include "wavetable.h"
#include "wavetable_creator.h"

#include <string>

namespace module_content {
  // {"preset": "init" | "predefined"} or {"creator": "<creator JSON>"}.
  inline bool renderWavetable(vital::Wavetable* wavetable, const nlohmann::json& spec) {
    WavetableCreator creator(wavetable);
    if (spec.count("creator"))
      creator.jsonToState(wave_recipes::expand(nlohmann::json::parse(spec["creator"].get<std::string>())));
    else if (spec.value("preset", std::string("init")) == "init")
      creator.init();
    else if (spec.value("preset", std::string()) == "predefined")
      creator.initPredefinedWaves();
    else
      return false;
    return true;
  }

  // {"length": n, "sample_rate": hz, "recipe": {...}, "recipe_stereo": {...}}
  // loaded through Sample::jsonToState with PCM-encoded recipes.
  inline void loadSample(vital::Sample* sample, const nlohmann::json& spec) {
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
    sample->jsonToState(data);
  }

  // params.controls: {"name": value}, set on the module's controls (the
  // values a preset or the host would set), in name order.
  inline bool setControls(vital::SynthModule* module, const nlohmann::json& controls) {
    vital::control_map map = module->getControls();
    for (auto it = controls.begin(); it != controls.end(); ++it) {
      if (map.count(it.key()) == 0)
        return false;
      map[it.key()]->set(it.value().get<float>());
    }
    return true;
  }
} // namespace module_content
