// Module "wavetable_render": WavetableCreator::render() into a vital::Wavetable.
// Artifact-only. Ports: port/crates/wavetable (creator::render, wavetable).
//
// params.creator: creator JSON as a string ({"$wave": ...} recipes expanded,
// see wave_recipes.h), or params.preset: "init" / "predefined".
// Arrays (compared with tolerances):
//   wave               [frames * 2048, 1]  wave_data (time domain)
//   coefficients       [frames * 2050, 1]  amplitude / 1024 x normalized phasor
//                                          (frequency_amplitudes and
//                                          normalized_frequencies, as the
//                                          oscillator combines them),
//                                          interleaved real, imaginary
//   phase_coefficients [frames * 2050, 1]  amplitude / 1024 x (cos, sin)(phase)
// The complex pairs share one channel: the residual is then relative to the
// whole spectrum. As two channels, an all-real spectrum (cleared phases) has
// an imaginary channel of pure rounding noise that no residual limit fits.
//   lanes              [3, 1]              over all frames: harmonics whose odd
//                                          lane [2h + 1] differs from [2h] in
//                                          frequency_amplitudes, the same for
//                                          phases, and non-zero padding floats
//                                          after the last harmonic in the three
//                                          arrays (bit comparisons; exact)
//   info               [4, 1]              num_frames / kNumOscillatorWaveFrames,
//                                          frequency_ratio,
//                                          sample_rate / kDefaultSampleRate,
//                                          shepard flag (counts and rates are
//                                          normalized: the absolute module
//                                          gate does not fit their scale)
// Raw amplitudes (up to ~1e3) and unit phasors of silent harmonics are not
// compared directly; see LEDGER.md (M4a rendering).

#include "module_registry.h"

#include "wave_recipes.h"
#include "wavetable.h"
#include "wavetable_creator.h"
#include "synth_constants.h"

#include <cmath>
#include <cstring>

REGISTER_REF_MODULE(refmod::ModuleDef{
  "wavetable_render",
  "WavetableCreator jsonToState(params.creator) (or params.preset init/predefined) -> render(); "
  "arrays wave, coefficients, phase_coefficients, info",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    vital::Wavetable wavetable(vital::kNumOscillatorWaveFrames);
    WavetableCreator creator(&wavetable);
    try {
      if (params.count("creator"))
        creator.jsonToState(wave_recipes::expand(nlohmann::json::parse(params["creator"].get<std::string>())));
      else if (params.value("preset", std::string()) == "init")
        creator.init();
      else if (params.value("preset", std::string()) == "predefined")
        creator.initPredefinedWaves();
      else {
        error = "params needs creator or preset";
        return false;
      }
    }
    catch (const std::exception& e) {
      error = std::string("creator JSON: ") + e.what();
      return false;
    }

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
    return true;
  },
});
