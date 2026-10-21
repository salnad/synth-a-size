// Module "multiband_compressor": MultibandCompressor (enabled_bands 3, single
// band, runs one Compressor alone). Ports: port/crates/effects.

#include "module_registry.h"

#include "compressor.h"
#include "linkwitz_riley_filter.h"

using refmod::Rate;
using vital::MultibandCompressor;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "multiband_compressor",
  "MultibandCompressor. 'enabled_bands': 0 multiband, 1 low band, 2 high band, 3 single band",
  [](const nlohmann::json&) -> vital::Processor* { return new MultibandCompressor(); },
  {
    { "audio", MultibandCompressor::kAudio, 0.0f, Rate::kAudio },
    { "low_upper_ratio", MultibandCompressor::kLowUpperRatio, 0.9f, Rate::kControl },
    { "band_upper_ratio", MultibandCompressor::kBandUpperRatio, 0.857f, Rate::kControl },
    { "high_upper_ratio", MultibandCompressor::kHighUpperRatio, 1.0f, Rate::kControl },
    { "low_lower_ratio", MultibandCompressor::kLowLowerRatio, 0.8f, Rate::kControl },
    { "band_lower_ratio", MultibandCompressor::kBandLowerRatio, 0.8f, Rate::kControl },
    { "high_lower_ratio", MultibandCompressor::kHighLowerRatio, 0.8f, Rate::kControl },
    { "low_upper_threshold", MultibandCompressor::kLowUpperThreshold, -28.0f, Rate::kControl },
    { "band_upper_threshold", MultibandCompressor::kBandUpperThreshold, -25.0f, Rate::kControl },
    { "high_upper_threshold", MultibandCompressor::kHighUpperThreshold, -30.0f, Rate::kControl },
    { "low_lower_threshold", MultibandCompressor::kLowLowerThreshold, -35.0f, Rate::kControl },
    { "band_lower_threshold", MultibandCompressor::kBandLowerThreshold, -36.0f, Rate::kControl },
    { "high_lower_threshold", MultibandCompressor::kHighLowerThreshold, -35.0f, Rate::kControl },
    { "low_output_gain", MultibandCompressor::kLowOutputGain, 16.3f, Rate::kControl },
    { "band_output_gain", MultibandCompressor::kBandOutputGain, 11.7f, Rate::kControl },
    { "high_output_gain", MultibandCompressor::kHighOutputGain, 16.3f, Rate::kControl },
    { "attack", MultibandCompressor::kAttack, 0.5f, Rate::kControl },
    { "release", MultibandCompressor::kRelease, 0.5f, Rate::kControl },
    { "enabled_bands", MultibandCompressor::kEnabledBands, 0.0f, Rate::kControl },
    { "mix", MultibandCompressor::kMix, 1.0f, Rate::kControl },
  },
  {
    { "audio", MultibandCompressor::kAudioOut },
    { "low_input_ms", MultibandCompressor::kLowInputMeanSquared },
    { "band_input_ms", MultibandCompressor::kBandInputMeanSquared },
    { "high_input_ms", MultibandCompressor::kHighInputMeanSquared },
    { "low_output_ms", MultibandCompressor::kLowOutputMeanSquared },
    { "band_output_ms", MultibandCompressor::kBandOutputMeanSquared },
    { "high_output_ms", MultibandCompressor::kHighOutputMeanSquared },
  },
});
