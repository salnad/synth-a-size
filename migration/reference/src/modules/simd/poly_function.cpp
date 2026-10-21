// Module "poly_function": applies one upstream poly_float / poly_utils /
// futils / scalar utils function to its inputs, sample by sample.
// Ports: port/crates/simd (tests/parity.rs mirrors the table below by name).
//
// The functions are upstream's own inline code (poly_values.h, poly_utils.h,
// futils.h, utils.h), compiled with the reference flags as part of ref-module.
// Mask results are written as 1.0/0.0, integer results through toFloat, and
// scalar results broadcast to all four lanes, so every output is an audio
// buffer that audio-diff can compare.

#include "module_registry.h"

#include "futils.h"
#include "poly_utils.h"
#include "processor.h"

#include <map>
#include <string>

namespace {
  using vital::matrix;
  using vital::poly_float;
  using vital::poly_int;
  using vital::poly_mask;
  namespace utils = vital::utils;
  namespace futils = vital::futils;

  using PolyFunction = poly_float (*)(poly_float a, poly_float b);

  poly_float maskValue(poly_mask mask) { return poly_float(1.0f) & mask; }

  // Upstream's lane order with both inputs mixed in: [a0, a1, b0, b1].
  poly_float mixed(poly_float a, poly_float b) { return utils::compactFirstVoices(a, b); }

  // Applies a mono function to each lane.
  template<float (*func)(float)>
  poly_float lanes(poly_float value) {
    poly_float result;
    for (int i = 0; i < poly_float::kSize; ++i)
      result.set(i, func(value[i]));
    return result;
  }

  float monoPowerScaleOf(float value, float power) { return futils::powerScale(value, power); }

#define FN(name, expression) \
  { name, [](poly_float a, poly_float b) -> poly_float { (void)a; (void)b; return (expression); } }

  const std::map<std::string, PolyFunction>& functions() {
    static const std::map<std::string, PolyFunction> table = {
      // poly_values.h
      FN("add", a + b),
      FN("sub", a - b),
      FN("mul", a * b),
      FN("div", a / b),
      FN("mul_scalar", a * 0.3f),
      FN("neg", -a),
      FN("max", poly_float::max(a, b)),
      FN("min", poly_float::min(a, b)),
      FN("abs", poly_float::abs(a)),
      FN("mul_add", utils::mulAdd(a, b, utils::swapStereo(b))),
      FN("mul_sub", utils::mulSub(a, b, utils::swapStereo(b))),
      FN("sum", poly_float(mixed(a, b).sum())),
      FN("equal", maskValue(poly_float::equal(utils::round(a), utils::round(b)))),
      FN("not_equal", maskValue(poly_float::notEqual(utils::round(a), utils::round(b)))),
      FN("greater_than", maskValue(poly_float::greaterThan(a, b))),
      FN("less_than_or_equal", maskValue(poly_float::lessThanOrEqual(a, b))),
      FN("sign_mask", maskValue(poly_float::sign_mask(a))),
      // poly_utils.h: conversions and rounding
      FN("to_int", utils::toFloat(utils::toInt(a))),
      FN("trunc", utils::trunc(a)),
      FN("floor", utils::floor(a)),
      FN("ceil", utils::ceil(a)),
      FN("round", utils::round(a)),
      FN("round_to_int", utils::toFloat(utils::roundToInt(a))),
      FN("floor_to_int", utils::toFloat(utils::floorToInt(a))),
      // reference-arm.md A7 (signed vs unsigned) and A8 (byte mask vs lane max).
      FN("int_min", utils::toFloat(poly_int::min(utils::roundToInt(a), utils::roundToInt(b)))),
      FN("any_mask", poly_float(static_cast<float>(poly_float::lessThan(a, b).anyMask()))),
      FN("mod", utils::mod(a)),
      FN("mod_once", utils::modOnce(a)),
      FN("sqrt", utils::sqrt(a)),
      FN("pow2_to_float", utils::pow2ToFloat(utils::roundToInt(a))),
      // poly_utils.h: shuffles and stereo
      FN("swap_stereo", utils::swapStereo(mixed(a, b))),
      FN("swap_voices", utils::swapVoices(mixed(a, b))),
      FN("swap_inner", utils::swapInner(mixed(a, b))),
      FN("reverse", utils::reverse(mixed(a, b))),
      FN("consolidate_audio", utils::consolidateAudio(a, b)),
      FN("compact_first_voices", utils::compactFirstVoices(a, b)),
      FN("sum_split_audio", utils::sumSplitAudio(mixed(a, b))),
      FN("max_float", poly_float(utils::maxFloat(mixed(a, b)))),
      FN("min_float", poly_float(utils::minFloat(mixed(a, b)))),
      FN("encode_mid_side", utils::encodeMidSide(mixed(a, b))),
      FN("decode_mid_side", utils::decodeMidSide(mixed(a, b))),
      FN("transpose_row0", ([](poly_float x, poly_float y) {
        matrix m(x, y, x * y, x - y); m.transpose(); return m.row0; })(mixed(a, b), mixed(b, a))),
      FN("transpose_row3", ([](poly_float x, poly_float y) {
        matrix m(x, y, x * y, x - y); m.transpose(); return m.row3; })(mixed(a, b), mixed(b, a))),
      // poly_utils.h: interpolation and shaping
      FN("interpolate", utils::interpolate(a, b, utils::mod(a + b))),
      FN("perlin_interpolate", utils::perlinInterpolate(a, b, utils::mod(a + b))),
      FN("triangle_wave", utils::triangleWave(a)),
      FN("flt_scale", utils::fltScale(a, b)),
      FN("mask_load", utils::maskLoad(a, b, poly_float::greaterThan(a, b))),
      FN("clamp", utils::clamp(a, -1.5f, 2.0f)),
      FN("snap_transpose", utils::snapTranspose(a, 0xab5)),
      FN("encode_phase_and_voice", utils::encodePhaseAndVoice(a, b)),
      FN("cubic_values", utils::getCubicInterpolationValues(a[0])),
      FN("optimal_values", utils::getOptimalInterpolationValues(a[1])),
      FN("catmull_row0", utils::getCatmullInterpolationMatrix(a).row0),
      FN("catmull_row1", utils::getCatmullInterpolationMatrix(a).row1),
      FN("catmull_row2", utils::getCatmullInterpolationMatrix(a).row2),
      FN("catmull_row3", utils::getCatmullInterpolationMatrix(a).row3),
      FN("polynomial_row0", utils::getPolynomialInterpolationMatrix(a).row0),
      FN("polynomial_row1", utils::getPolynomialInterpolationMatrix(a).row1),
      FN("polynomial_row2", utils::getPolynomialInterpolationMatrix(a).row2),
      FN("polynomial_row3", utils::getPolynomialInterpolationMatrix(a).row3),
      FN("polynomial_sum_rows", utils::getPolynomialInterpolationMatrix(a).sumRows()),
      FN("matrix_multiply_and_sum_rows",
         utils::getCatmullInterpolationMatrix(a).multiplyAndSumRows(utils::getPolynomialInterpolationMatrix(b))),
      // poly_utils.h: libm maps
      FN("poly_cents_to_ratio", utils::centsToRatio(a)),
      FN("poly_midi_note_to_frequency", utils::midiNoteToFrequency(a)),
      // Normalized as the filters consume it, frequency * (1.0f / sample rate)
      // (digital_svf.cpp:81); b carries the sample rate.
      FN("poly_midi_note_to_normalized_frequency", utils::midiNoteToFrequency(a) * (1.0f / b[0])),
      FN("poly_frequency_to_midi_note", utils::frequencyToMidiNote(a)),
      FN("poly_db_to_magnitude", utils::dbToMagnitude(a)),
      FN("poly_magnitude_to_db", utils::magnitudeToDb(a)),
      FN("poly_sin", utils::sin(a)),
      FN("poly_cos", utils::cos(a)),
      FN("poly_tan", utils::tan(a)),
      FN("poly_pow", utils::pow(a, b)),
      // futils.h (poly)
      FN("exp2", futils::exp2(a)),
      FN("log2", futils::log2(a)),
      FN("cheap_exp2", futils::cheapExp2(a)),
      FN("cheap_log2", futils::cheapLog2(a)),
      FN("exp", futils::exp(a)),
      FN("log", futils::log(a)),
      FN("exp_half", futils::exp_half(a)),
      FN("pow", futils::pow(a, b)),
      FN("cheap_pow", futils::cheapPow(a, b)),
      FN("midi_offset_to_ratio", futils::midiOffsetToRatio(a)),
      FN("midi_note_to_frequency", futils::midiNoteToFrequency(a)),
      FN("magnitude_to_db", futils::magnitudeToDb(a)),
      FN("db_to_magnitude", futils::dbToMagnitude(a)),
      FN("quick_tanh", futils::quickTanh(a)),
      FN("quick_tanh_derivative", futils::quickTanhDerivative(a)),
      FN("algebraic_sat", futils::algebraicSat(a)),
      FN("quadratic_inv_sat", futils::quadraticInvSat(a)),
      FN("bump_sat", futils::bumpSat(a)),
      FN("bump_sat2", futils::bumpSat2(a)),
      FN("algebraic_sat_derivative", futils::algebraicSatDerivative(a)),
      FN("tanh", futils::tanh(a)),
      FN("hard_tanh", futils::hardTanh(a)),
      FN("tanh_derivative_fast", futils::tanhDerivativeFast(a)),
      FN("quick_sin", futils::quickSin(a)),
      FN("sin", futils::sin(a)),
      FN("quick_sin1", futils::quickSin1(a)),
      FN("sin1", futils::sin1(a)),
      FN("sin_interpolate", futils::sinInterpolate(a, b, utils::mod(a + b))),
      FN("equal_power_fade", futils::equalPowerFade(a)),
      FN("equal_power_fade_inverse", futils::equalPowerFadeInverse(a)),
      FN("pan_amplitude", futils::panAmplitude(a)),
      FN("power_scale", futils::powerScale(a, b)),
      // futils.h (mono overloads, per lane)
      FN("mono_exp2", lanes<futils::exp2>(a)),
      FN("mono_log2", lanes<futils::log2>(a)),
      FN("mono_exp", lanes<futils::exp>(a)),
      FN("mono_log", lanes<futils::log>(a)),
      FN("mono_magnitude_to_db", lanes<futils::magnitudeToDb>(a)),
      FN("mono_db_to_magnitude", lanes<futils::dbToMagnitude>(a)),
      FN("mono_quick_tanh", lanes<futils::quickTanh>(a)),
      FN("mono_quick_tanh_derivative", lanes<futils::quickTanhDerivative>(a)),
      FN("mono_reciprocal_sat", lanes<futils::reciprocalSat>(a)),
      FN("mono_algebraic_sat", lanes<futils::algebraicSat>(a)),
      FN("mono_algebraic_sat_derivative", lanes<futils::algebraicSatDerivative>(a)),
      FN("mono_tanh", lanes<futils::tanh>(a)),
      FN("mono_quick_sin", lanes<futils::quickSin>(a)),
      FN("mono_sin", lanes<futils::sin>(a)),
      FN("mono_quick_sin1", lanes<futils::quickSin1>(a)),
      FN("mono_sin1", lanes<futils::sin1>(a)),
      FN("mono_power_scale", ([](poly_float x, poly_float y) {
        poly_float result;
        for (int i = 0; i < poly_float::kSize; ++i)
          result.set(i, monoPowerScaleOf(x[i], y[i]));
        return result; })(a, b)),
      // utils.h (scalar, per lane)
      FN("mono_cents_to_ratio", lanes<utils::centsToRatio>(a)),
      FN("mono_note_offset_to_ratio", lanes<utils::noteOffsetToRatio>(a)),
      FN("mono_ratio_to_midi_transpose", lanes<utils::ratioToMidiTranspose>(a)),
      FN("mono_midi_note_to_frequency", lanes<utils::midiNoteToFrequency>(a)),
      FN("mono_midi_note_to_normalized_frequency", lanes<utils::midiNoteToFrequency>(a) * (1.0f / b[0])),
      FN("mono_frequency_to_midi_note", lanes<utils::frequencyToMidiNote>(a)),
      FN("mono_utils_db_to_magnitude", lanes<utils::dbToMagnitude>(a)),
      FN("mono_utils_magnitude_to_db", lanes<utils::magnitudeToDb>(a)),
      FN("mono_inverse_flt_scale", lanes<utils::inverseFltScale>(a)),
    };
    return table;
  }

#undef FN

  class PolyFunctionProbe : public vital::Processor {
    public:
      enum { kA, kB, kNumInputs };

      explicit PolyFunctionProbe(PolyFunction function) : vital::Processor(kNumInputs, 1), function_(function) { }

      vital::Processor* clone() const override { return new PolyFunctionProbe(*this); }

      void process(int num_samples) override {
        const poly_float* a = input(kA)->source->buffer;
        const poly_float* b = input(kB)->source->buffer;
        poly_float* out = output()->buffer;
        for (int i = 0; i < num_samples; ++i)
          out[i] = function_(a[i], b[i]);
      }

    private:
      PolyFunction function_;
  };
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "poly_function",
  "One upstream poly_float/poly_utils/futils/utils function applied to inputs a and b. Param 'function': the name.",
  [](const nlohmann::json& params) -> vital::Processor* {
    auto found = functions().find(params.value("function", std::string()));
    if (found == functions().end())
      return nullptr;
    return new PolyFunctionProbe(found->second);
  },
  {
    { "a", PolyFunctionProbe::kA, 0.0f, refmod::Rate::kAudio },
    { "b", PolyFunctionProbe::kB, 0.0f, refmod::Rate::kAudio },
  },
  { { "out", 0 } },
});
