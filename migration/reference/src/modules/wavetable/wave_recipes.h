// Authored test waveforms for the wavetable modules. A JSON object
// {"$wave": recipe} anywhere in an input is replaced by the base64 buffer the
// recipe describes, so case files stay small. The port's tests expand the same
// recipes (port/crates/wavetable/tests/support/mod.rs); every value is computed
// with integer phase arithmetic and single float operations, which fast-math
// cannot reorder, so both sides produce identical bits.
//
// recipe: {"shape": "saw"|"square"|"triangle"|"mix", "length": 2048,
//          "period": length, "shift": 0, "amp": 1.0, "duty": period / 2,
//          "parts": [recipe...], "weights": [float...],
//          "encoding": "f32" | "pcm"}
#pragma once

#include "JuceHeader.h"
#include "json/json.h"

#include <cstdint>
#include <string>
#include <vector>

namespace wave_recipes {
  inline std::vector<float> samples(const nlohmann::json& recipe) {
    int length = recipe.value("length", 2048);
    int period = recipe.value("period", length);
    int shift = recipe.value("shift", 0);
    float amp = recipe.value("amp", 1.0f);
    std::string shape = recipe.value("shape", std::string("saw"));
    std::vector<float> out(length, 0.0f);

    if (shape == "mix") {
      const nlohmann::json& parts = recipe.at("parts");
      const nlohmann::json& weights = recipe.at("weights");
      for (size_t p = 0; p < parts.size(); ++p) {
        nlohmann::json part = parts[p];
        part["length"] = length;
        std::vector<float> part_samples = samples(part);
        float weight = weights[p];
        for (int i = 0; i < length; ++i)
          out[i] += weight * part_samples[i];
      }
      return out;
    }

    // Slopes are powers of two so the phase-to-value step is exact.
    int half = period / 2;
    int duty = recipe.value("duty", half);
    for (int i = 0; i < length; ++i) {
      int phase = ((i + shift) % period + period) % period;
      float value = 0.0f;
      if (shape == "saw")
        value = static_cast<float>(phase - half) * recipe.value("slope", 1.0f / 1024.0f);
      else if (shape == "square")
        value = phase < duty ? 1.0f : -1.0f;
      else if (shape == "triangle")
        value = static_cast<float>(phase < half ? phase * 2 - half : 3 * half - phase * 2) * recipe.value("slope", 1.0f / 1024.0f);
      out[i] = amp * value;
    }
    return out;
  }

  inline std::string encode(const nlohmann::json& recipe) {
    std::vector<float> values = samples(recipe);
    if (recipe.value("encoding", std::string("f32")) == "pcm") {
      std::vector<int16_t> pcm(values.size());
      for (size_t i = 0; i < values.size(); ++i)
        pcm[i] = static_cast<int16_t>(values[i] * 32767.0f);
      return juce::Base64::toBase64(pcm.data(), pcm.size() * sizeof(int16_t)).toStdString();
    }
    return juce::Base64::toBase64(values.data(), values.size() * sizeof(float)).toStdString();
  }

  inline nlohmann::json expand(const nlohmann::json& data) {
    if (data.is_object()) {
      if (data.count("$wave"))
        return encode(data.at("$wave"));
      nlohmann::json result = nlohmann::json::object();
      for (auto it = data.begin(); it != data.end(); ++it)
        result[it.key()] = expand(it.value());
      return result;
    }
    if (data.is_array()) {
      nlohmann::json result = nlohmann::json::array();
      for (const nlohmann::json& item : data)
        result.push_back(expand(item));
      return result;
    }
    return data;
  }
} // namespace wave_recipes
