// Modules "op_*": the framework operators (operators.h), each driven
// directly. Ports: port/crates/dsp-core (tests/parity.rs mirrors these
// definitions by name).

#include "module_registry.h"

#include "operators.h"

using refmod::Rate;

namespace {
  float param(const nlohmann::json& params, const char* name, float fallback) {
    return params.value(name, fallback);
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_clamp", "Clamp(min, max); params min, max",
  [](const nlohmann::json& p) -> vital::Processor* {
    return new vital::Clamp(param(p, "min", -1.0f), param(p, "max", 1.0f));
  },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_negate", "Negate",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Negate(); },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_inverse", "Inverse",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Inverse(); },
  { { "audio", 0, 1.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_linear_scale", "LinearScale(scale); param scale",
  [](const nlohmann::json& p) -> vital::Processor* { return new vital::LinearScale(param(p, "scale", 1.0f)); },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_square", "Square",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Square(); },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_add", "Add",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Add(); },
  { { "a", 0, 0.0f, Rate::kAudio }, { "b", 1, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_subtract", "Subtract",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Subtract(); },
  { { "a", 0, 0.0f, Rate::kAudio }, { "b", 1, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_multiply", "Multiply",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Multiply(); },
  { { "a", 0, 0.0f, Rate::kAudio }, { "b", 1, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_variable_add", "VariableAdd(3)",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::VariableAdd(3); },
  { { "a", 0, 0.0f, Rate::kAudio }, { "b", 1, 0.0f, Rate::kAudio }, { "c", 2, 0.0f, Rate::kAudio } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_smooth_multiply", "SmoothMultiply",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::SmoothMultiply(); },
  {
    { "audio", vital::SmoothMultiply::kAudioRate, 0.0f, Rate::kAudio },
    { "multiply", vital::SmoothMultiply::kControlRate, 0.0f, Rate::kControl },
    { "reset", vital::SmoothMultiply::kReset, 0.0f, Rate::kTrigger },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_smooth_volume", "SmoothVolume(max_db); param max_db",
  [](const nlohmann::json& p) -> vital::Processor* {
    return new vital::SmoothVolume(param(p, "max_db", vital::SmoothVolume::kDefaultMaxDb));
  },
  {
    { "audio", vital::SmoothVolume::kAudioRate, 0.0f, Rate::kAudio },
    { "db", vital::SmoothVolume::kDb, 0.0f, Rate::kControl },
    { "reset", vital::SmoothVolume::kReset, 0.0f, Rate::kTrigger },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_interpolate", "Interpolate",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::Interpolate(); },
  {
    { "from", vital::Interpolate::kFrom, 0.0f, Rate::kAudio },
    { "to", vital::Interpolate::kTo, 0.0f, Rate::kAudio },
    { "fractional", vital::Interpolate::kFractional, 0.0f, Rate::kAudio },
    { "reset", vital::Interpolate::kReset, 0.0f, Rate::kTrigger },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_bilinear_interpolate", "BilinearInterpolate",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::BilinearInterpolate(); },
  {
    { "top_left", vital::BilinearInterpolate::kTopLeft, 0.0f, Rate::kControl },
    { "top_right", vital::BilinearInterpolate::kTopRight, 0.0f, Rate::kControl },
    { "bottom_left", vital::BilinearInterpolate::kBottomLeft, 0.0f, Rate::kControl },
    { "bottom_right", vital::BilinearInterpolate::kBottomRight, 0.0f, Rate::kControl },
    { "x", vital::BilinearInterpolate::kXPosition, 0.0f, Rate::kAudio },
    { "y", vital::BilinearInterpolate::kYPosition, 0.0f, Rate::kAudio },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_sample_and_hold", "SampleAndHoldBuffer",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::SampleAndHoldBuffer(); },
  { { "audio", 0, 0.0f, Rate::kControl } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_stereo_encoder", "StereoEncoder(decoding); param decoding (0/1)",
  [](const nlohmann::json& p) -> vital::Processor* {
    return new vital::StereoEncoder(param(p, "decoding", 0.0f) != 0.0f);
  },
  {
    { "audio", vital::StereoEncoder::kAudio, 0.0f, Rate::kAudio },
    { "encoding", vital::StereoEncoder::kEncodingValue, 0.0f, Rate::kControl },
    { "mode", vital::StereoEncoder::kMode, 0.0f, Rate::kControl },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_tempo_chooser", "TempoChooser",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::TempoChooser(); },
  {
    { "frequency", vital::TempoChooser::kFrequency, 1.0f, Rate::kControl },
    { "tempo_index", vital::TempoChooser::kTempoIndex, 0.0f, Rate::kControl },
    { "beats_per_second", vital::TempoChooser::kBeatsPerSecond, 2.0f, Rate::kControl },
    { "sync", vital::TempoChooser::kSync, 0.0f, Rate::kControl },
    { "midi", vital::TempoChooser::kMidi, 0.0f, Rate::kControl },
    { "keytrack_transpose", vital::TempoChooser::kKeytrackTranspose, 0.0f, Rate::kControl },
    { "keytrack_tune", vital::TempoChooser::kKeytrackTune, 0.0f, Rate::kControl },
  },
  { { "out", 0 } },
});

#define CR_UNARY(name, desc, expression) \
  REGISTER_REF_MODULE(refmod::ModuleDef{ \
    name, desc, \
    [](const nlohmann::json& p) -> vital::Processor* { (void)p; return expression; }, \
    { { "in", 0, 0.0f, Rate::kControl } }, \
    { { "out", 0 } }, \
  })

CR_UNARY("op_cr_clamp", "cr::Clamp(min, max)", new vital::cr::Clamp(param(p, "min", -1.0f), param(p, "max", 1.0f)));
CR_UNARY("op_cr_lower_bound", "cr::LowerBound(min)", new vital::cr::LowerBound(param(p, "min", 0.0f)));
CR_UNARY("op_cr_upper_bound", "cr::UpperBound(max)", new vital::cr::UpperBound(param(p, "max", 0.0f)));
CR_UNARY("op_cr_square", "cr::Square", new vital::cr::Square());
CR_UNARY("op_cr_cube", "cr::Cube", new vital::cr::Cube());
CR_UNARY("op_cr_quart", "cr::Quart", new vital::cr::Quart());
CR_UNARY("op_cr_quadratic", "cr::Quadratic(offset)", new vital::cr::Quadratic(param(p, "offset", 0.0f)));
CR_UNARY("op_cr_cubic", "cr::Cubic(offset)", new vital::cr::Cubic(param(p, "offset", 0.0f)));
CR_UNARY("op_cr_quartic", "cr::Quartic(offset)", new vital::cr::Quartic(param(p, "offset", 0.0f)));
CR_UNARY("op_cr_root", "cr::Root(offset)", new vital::cr::Root(param(p, "offset", 0.0f)));
CR_UNARY("op_cr_exponential_scale", "cr::ExponentialScale(min, max, scale, offset)",
         new vital::cr::ExponentialScale(param(p, "min", 0.0f), param(p, "max", 1.0f),
                                         param(p, "scale", 1.0f), param(p, "offset", 0.0f)));
CR_UNARY("op_cr_frequency_to_phase", "cr::FrequencyToPhase", new vital::cr::FrequencyToPhase());
CR_UNARY("op_cr_frequency_to_samples", "cr::FrequencyToSamples", new vital::cr::FrequencyToSamples());
CR_UNARY("op_cr_time_to_samples", "cr::TimeToSamples", new vital::cr::TimeToSamples());
CR_UNARY("op_cr_magnitude_scale", "cr::MagnitudeScale", new vital::cr::MagnitudeScale());
CR_UNARY("op_cr_midi_scale", "cr::MidiScale", new vital::cr::MidiScale());

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_cr_add", "cr::Add",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::cr::Add(); },
  { { "a", 0, 0.0f, Rate::kControl }, { "b", 1, 0.0f, Rate::kControl } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_cr_multiply", "cr::Multiply",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::cr::Multiply(); },
  { { "a", 0, 0.0f, Rate::kControl }, { "b", 1, 0.0f, Rate::kControl } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_cr_interpolate", "cr::Interpolate",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::cr::Interpolate(); },
  {
    { "from", vital::cr::Interpolate::kFrom, 0.0f, Rate::kControl },
    { "to", vital::cr::Interpolate::kTo, 0.0f, Rate::kControl },
    { "fractional", vital::cr::Interpolate::kFractional, 0.0f, Rate::kControl },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_cr_variable_add", "cr::VariableAdd(3)",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::cr::VariableAdd(3); },
  { { "a", 0, 0.0f, Rate::kControl }, { "b", 1, 0.0f, Rate::kControl }, { "c", 2, 0.0f, Rate::kControl } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "op_cr_bilinear_interpolate", "cr::BilinearInterpolate",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::cr::BilinearInterpolate(); },
  {
    { "top_left", vital::cr::BilinearInterpolate::kTopLeft, 0.0f, Rate::kControl },
    { "top_right", vital::cr::BilinearInterpolate::kTopRight, 0.0f, Rate::kControl },
    { "bottom_left", vital::cr::BilinearInterpolate::kBottomLeft, 0.0f, Rate::kControl },
    { "bottom_right", vital::cr::BilinearInterpolate::kBottomRight, 0.0f, Rate::kControl },
    { "x", vital::cr::BilinearInterpolate::kXPosition, 0.0f, Rate::kControl },
    { "y", vital::cr::BilinearInterpolate::kYPosition, 0.0f, Rate::kControl },
  },
  { { "out", 0 } },
});
