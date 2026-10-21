// ref-complex-probe: std::complex<float> division on this machine
// (migration/decisions/reference-arm.md, A10). Runs value / z and z / w
// through std::complex and the compiler's _Complex division, each compiled
// with the reference flags (fast) and strict, over unit-circle, near-unit,
// wide-exponent and special inputs. Each result is compared on this machine
// against four algorithms (naive, Smith, logb/scalbn, naive in double) and
// hashed for cross-machine comparison. Also hashes upstream's
// PhaseModifierKeyframe::render, whose kEvenOdd / kHarmonicEvenOdd styles
// compute 1.0f / phase_shift. Linked against the engine with its flags.

#include "JuceHeader.h"
#include "complex_probe.h"
#include "harness_io.h"
#include "phase_modifier.h"
#include "wave_frame.h"

#include <complex>
#include <cstdio>
#include <cstring>
#include <functional>
#include <string>

namespace {

uint32_t bitsOf(float value) {
  uint32_t bits;
  std::memcpy(&bits, &value, sizeof(bits));
  return bits;
}

bool isNanBits(float value) {
  return (bitsOf(value) & 0x7FFFFFFFu) > 0x7F800000u;
}

// Bit-equal, treating every NaN as one value (payloads and NaN signs vary).
bool sameValue(float x, float y) {
  if (isNanBits(x) || isNanBits(y))
    return isNanBits(x) && isNanBits(y);
  return bitsOf(x) == bitsOf(y);
}

long long ulpDistance(float x, float y) {
  if (isNanBits(x) || isNanBits(y))
    return sameValue(x, y) ? 0 : -1;
  auto ordered = [](float value) {
    int64_t bits = static_cast<int32_t>(bitsOf(value));
    return bits < 0 ? static_cast<int64_t>(INT32_MIN) - bits : bits;
  };
  int64_t distance = ordered(x) - ordered(y);
  return distance < 0 ? -distance : distance;
}

uint64_t hashResult(const std::vector<float>& re, const std::vector<float>& im) {
  uint64_t hash = 1469598103934665603ULL;
  for (size_t i = 0; i < re.size(); ++i) {
    // Canonical NaN so payload differences don't count.
    float pair[2] = { isNanBits(re[i]) ? NAN : re[i], isNanBits(im[i]) ? NAN : im[i] };
    hash = harness::fnv1a64(pair, sizeof(pair), hash);
  }
  return hash;
}

using Algorithm = void (*)(float, float, float, float, float&, float&);

const std::pair<const char*, Algorithm> kAlgorithms[] = {
  { "naive", complex_algos::naive },
  { "smith", complex_algos::smith },
  { "logb_scalbn", complex_algos::logbScalbn },
  { "wide_double", complex_algos::wideDouble },
};

struct Variant {
  const char* name;
  std::function<void(const complex_algos::Inputs&, float*, float*, int)> run;
};

void probeCategory(harness::json& result, const std::string& category, const complex_algos::Inputs& in,
                   bool real_numerator) {
  int n = static_cast<int>(in.c.size());
  std::vector<Variant> variants;
  if (real_numerator) {
    variants = {
      { "std_fast", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          fast_ops::stdRealDivide(1.0f, v.c.data(), v.d.data(), re, im, count); } },
      { "std_strict", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          strict_ops::stdRealDivide(1.0f, v.c.data(), v.d.data(), re, im, count); } },
      { "builtin_fast", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          fast_ops::builtinRealDivide(1.0f, v.c.data(), v.d.data(), re, im, count); } },
      { "builtin_strict", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          strict_ops::builtinRealDivide(1.0f, v.c.data(), v.d.data(), re, im, count); } },
    };
  }
  else {
    variants = {
      { "std_fast", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          fast_ops::stdDivide(v.a.data(), v.b.data(), v.c.data(), v.d.data(), re, im, count); } },
      { "std_strict", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          strict_ops::stdDivide(v.a.data(), v.b.data(), v.c.data(), v.d.data(), re, im, count); } },
      { "builtin_fast", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          fast_ops::builtinDivide(v.a.data(), v.b.data(), v.c.data(), v.d.data(), re, im, count); } },
      { "builtin_strict", [](const complex_algos::Inputs& v, float* re, float* im, int count) {
          strict_ops::builtinDivide(v.a.data(), v.b.data(), v.c.data(), v.d.data(), re, im, count); } },
    };
  }

  std::vector<std::vector<float>> expected_re(4, std::vector<float>(n)), expected_im(4, std::vector<float>(n));
  for (int k = 0; k < 4; ++k) {
    for (int i = 0; i < n; ++i)
      kAlgorithms[k].second(in.a[i], in.b[i], in.c[i], in.d[i], expected_re[k][i], expected_im[k][i]);
  }

  result["inputs"][category] = n;
  std::vector<float> re(n), im(n);
  for (const Variant& variant : variants) {
    variant.run(in, re.data(), im.data(), n);
    std::string key = category + "/" + variant.name;
    result["hashes"][key] = harness::hex64(hashResult(re, im));
    harness::json stats;
    for (int k = 0; k < 4; ++k) {
      long long equal = 0;
      long long max_ulp = 0;
      long long nan_mismatch = 0;
      for (int i = 0; i < n; ++i) {
        bool same = sameValue(re[i], expected_re[k][i]) && sameValue(im[i], expected_im[k][i]);
        equal += same;
        if (!same) {
          long long d_re = ulpDistance(re[i], expected_re[k][i]);
          long long d_im = ulpDistance(im[i], expected_im[k][i]);
          if (d_re < 0 || d_im < 0)
            ++nan_mismatch;
          else
            max_ulp = std::max(max_ulp, std::max(d_re, d_im));
        }
      }
      stats[std::string("equal_") + kAlgorithms[k].first] = equal;
      stats[std::string("max_ulp_") + kAlgorithms[k].first] = max_ulp;
      stats[std::string("nan_mismatch_") + kAlgorithms[k].first] = nan_mismatch;
    }
    result["stats"][key] = stats;
  }
}

void probePhaseModifier(harness::json& result) {
  std::vector<float> phases = complex_algos::uniform(77, 62, -vital::kPi, vital::kPi);
  phases.push_back(0.5f);
  phases.push_back(vital::kPi / 3.0f);
  std::vector<float> spectrum = complex_algos::uniform(78, 2 * vital::WaveFrame::kWaveformSize, -1.0f, 1.0f);

  const std::pair<const char*, PhaseModifier::PhaseStyle> styles[] = {
    { "normal", PhaseModifier::kNormal },
    { "even_odd", PhaseModifier::kEvenOdd },
    { "harmonic", PhaseModifier::kHarmonic },
    { "harmonic_even_odd", PhaseModifier::kHarmonicEvenOdd },
  };
  for (const auto& style : styles) {
    for (float mix : { 1.0f, 0.37f }) {
      uint64_t hash = 1469598103934665603ULL;
      for (float phase : phases) {
        vital::WaveFrame frame;
        std::memcpy(frame.frequency_domain, spectrum.data(), spectrum.size() * sizeof(float));
        PhaseModifier::PhaseModifierKeyframe keyframe;
        keyframe.setPhase(phase);
        keyframe.setMix(mix);
        keyframe.setPhaseStyle(style.second);
        keyframe.render(&frame);
        hash = harness::fnv1a64(frame.frequency_domain, sizeof(frame.frequency_domain), hash);
      }
      std::string key = std::string("phase_modifier/") + style.first + (mix == 1.0f ? "_mix1" : "_mix0.37");
      result["hashes"][key] = harness::hex64(hash);
    }
  }

  // std::polar's cosf/sinf come from the platform libm; hashed separately so
  // a render difference can be split into polar vs division.
  uint64_t polar_hash = 1469598103934665603ULL;
  complex_algos::Inputs shifts;
  for (float phase : phases) {
    std::complex<float> shift = std::polar(1.0f, -phase);
    polar_hash = harness::fnv1a64(&shift, sizeof(shift), polar_hash);
    shifts.a.push_back(1.0f);
    shifts.b.push_back(0.0f);
    shifts.c.push_back(shift.real());
    shifts.d.push_back(shift.imag());
  }
  result["hashes"]["phase_modifier/polar_shifts"] = harness::hex64(polar_hash);
  probeCategory(result, "phase_modifier_odd_shift", shifts, true);
}

} // namespace

int main() {
  harness::json result;
  const int kCount = 1 << 20;
  probeCategory(result, "unit_circle_real", complex_algos::unitCircle(1, kCount), true);
  complex_algos::Inputs near_unit_real = complex_algos::nearUnit(2, kCount);
  near_unit_real.a.assign(kCount, 1.0f);
  near_unit_real.b.assign(kCount, 0.0f);
  probeCategory(result, "near_unit_real", near_unit_real, true);
  probeCategory(result, "near_unit", complex_algos::nearUnit(3, kCount), false);
  probeCategory(result, "exp_m20_20", complex_algos::randomExponent(4, kCount, -20, 20), false);
  probeCategory(result, "exp_full_range", complex_algos::randomExponent(5, kCount, -150, 127), false);
  probeCategory(result, "special_grid", complex_algos::specialGrid(), false);
  probePhaseModifier(result);
  result["build"] = harness::buildInfo();
  std::printf("%s\n", result.dump(2).c_str());
  return 0;
}
