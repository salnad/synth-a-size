// The render arrays of a rendered table, for the wavetable_edit module: the
// same arrays, computed the same way, as render.cpp emits (see there for
// what each holds). render.cpp keeps its own copy so its hashes stay put.

#pragma once

#include "module_registry.h"

#include "wavetable.h"
#include "synth_constants.h"

#include <cmath>
#include <cstring>

namespace render_arrays {

inline void emit(vital::Wavetable& wavetable, refmod::ArtifactSink& sink) {
  const vital::Wavetable::WavetableData* data = wavetable.getAllData();
  constexpr int kHarmonics = vital::Wavetable::kNumHarmonics;
  constexpr int kFrequencyFloats = sizeof(data->frequency_amplitudes[0]) / sizeof(float);
  auto bits = [](float value) {
    uint32_t result;
    memcpy(&result, &value, sizeof(result));
    return result;
  };
  float amplitude_lane_mismatches = 0.0f, phase_lane_mismatches = 0.0f, nonzero_padding = 0.0f;
  std::vector<float> wave, coefficients, phase_coefficients;
  for (int w = 0; w < data->num_frames; ++w) {
    wave.insert(wave.end(), data->wave_data[w], data->wave_data[w] + vital::WaveFrame::kWaveformSize);
    const float* amplitudes = reinterpret_cast<const float*>(data->frequency_amplitudes[w]);
    const float* normalized = reinterpret_cast<const float*>(data->normalized_frequencies[w]);
    const float* phases = reinterpret_cast<const float*>(data->phases[w]);
    for (int h = 0; h < kHarmonics; ++h) {
      float amplitude = amplitudes[2 * h] / 1024.0f;
      coefficients.push_back(amplitude * normalized[2 * h]);
      coefficients.push_back(amplitude * normalized[2 * h + 1]);
      phase_coefficients.push_back(amplitude * cosf(phases[2 * h]));
      phase_coefficients.push_back(amplitude * sinf(phases[2 * h]));
      amplitude_lane_mismatches += bits(amplitudes[2 * h + 1]) != bits(amplitudes[2 * h]) ? 1.0f : 0.0f;
      phase_lane_mismatches += bits(phases[2 * h + 1]) != bits(phases[2 * h]) ? 1.0f : 0.0f;
    }
    for (int i = 2 * kHarmonics; i < kFrequencyFloats; ++i) {
      nonzero_padding += bits(amplitudes[i]) != 0 ? 1.0f : 0.0f;
      nonzero_padding += bits(normalized[i]) != 0 ? 1.0f : 0.0f;
      nonzero_padding += bits(phases[i]) != 0 ? 1.0f : 0.0f;
    }
  }
  sink.array("wave", wave, 1);
  sink.array("coefficients", coefficients, 1);
  sink.array("phase_coefficients", phase_coefficients, 1);
  sink.array("lanes", { amplitude_lane_mismatches, phase_lane_mismatches, nonzero_padding }, 1);
  sink.array("info", { data->num_frames / static_cast<float>(vital::kNumOscillatorWaveFrames),
                       data->frequency_ratio, data->sample_rate / vital::kDefaultSampleRate,
                       wavetable.isShepardTable() ? 1.0f : 0.0f }, 1);
}

} // namespace render_arrays
