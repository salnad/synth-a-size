#include "complex_probe.h"

#include <cfloat>
#include <cmath>
#include <cstring>
#include <math.h>

extern "C" __complex__ float __divsc3(float a, float b, float c, float d);

namespace complex_algos {
  namespace {
    uint64_t splitmix(uint64_t& state) {
      uint64_t z = (state += 0x9E3779B97F4A7C15ULL);
      z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9ULL;
      z = (z ^ (z >> 27)) * 0x94D049BB133111EBULL;
      return z ^ (z >> 31);
    }

    float unitFloat(uint64_t& state) {
      return static_cast<float>(splitmix(state) >> 40) * (1.0f / 16777216.0f);
    }

    float fromBits(uint32_t bits) {
      float value;
      std::memcpy(&value, &bits, sizeof(value));
      return value;
    }

    float randomExponentFloat(uint64_t& state, int min_exp, int max_exp) {
      uint64_t r = splitmix(state);
      int exponent = min_exp + static_cast<int>((r >> 32) % static_cast<uint64_t>(max_exp - min_exp + 1));
      uint32_t mantissa = static_cast<uint32_t>(r) & 0x7FFFFFu;
      uint32_t sign = static_cast<uint32_t>(r >> 31) & 0x80000000u;
      if (exponent < -126)
        return fromBits(sign | (mantissa >> (-126 - exponent)));
      return fromBits(sign | (static_cast<uint32_t>(exponent + 127) << 23) | mantissa);
    }
  } // namespace

  std::vector<float> uniform(uint64_t seed, int n, float lo, float hi) {
    std::vector<float> values(n);
    for (int i = 0; i < n; ++i)
      values[i] = lo + (hi - lo) * unitFloat(seed);
    return values;
  }

  Inputs unitCircle(uint64_t seed, int n) {
    Inputs inputs;
    for (int i = 0; i < n; ++i) {
      float c = 2.0f * unitFloat(seed) - 1.0f;
      float d = std::sqrt(1.0f - c * c);
      if (splitmix(seed) & 1)
        d = -d;
      inputs.c.push_back(c);
      inputs.d.push_back(d);
      inputs.a.push_back(1.0f);
      inputs.b.push_back(0.0f);
    }
    return inputs;
  }

  Inputs nearUnit(uint64_t seed, int n) {
    Inputs inputs;
    for (int i = 0; i < n; ++i) {
      inputs.a.push_back(4.0f * unitFloat(seed) - 2.0f);
      inputs.b.push_back(4.0f * unitFloat(seed) - 2.0f);
      inputs.c.push_back(4.0f * unitFloat(seed) - 2.0f);
      inputs.d.push_back(4.0f * unitFloat(seed) - 2.0f);
    }
    return inputs;
  }

  Inputs randomExponent(uint64_t seed, int n, int min_exp, int max_exp) {
    Inputs inputs;
    for (int i = 0; i < n; ++i) {
      inputs.a.push_back(randomExponentFloat(seed, min_exp, max_exp));
      inputs.b.push_back(randomExponentFloat(seed, min_exp, max_exp));
      inputs.c.push_back(randomExponentFloat(seed, min_exp, max_exp));
      inputs.d.push_back(randomExponentFloat(seed, min_exp, max_exp));
    }
    return inputs;
  }

  Inputs specialGrid() {
    const float values[] = { 0.0f, -0.0f, fromBits(1), FLT_MIN, 1.0f, -3.0f, FLT_MAX, INFINITY, NAN };
    Inputs inputs;
    for (float a : values) {
      for (float b : values) {
        for (float c : values) {
          for (float d : values) {
            inputs.a.push_back(a);
            inputs.b.push_back(b);
            inputs.c.push_back(c);
            inputs.d.push_back(d);
          }
        }
      }
    }
    return inputs;
  }

  void naive(float a, float b, float c, float d, float& x, float& y) {
    float denominator = c * c + d * d;
    x = (a * c + b * d) / denominator;
    y = (b * c - a * d) / denominator;
  }

  // Smith (1962), all in float.
  void smith(float a, float b, float c, float d, float& x, float& y) {
    if (std::fabs(c) >= std::fabs(d)) {
      float ratio = d / c;
      float denominator = c + d * ratio;
      x = (a + b * ratio) / denominator;
      y = (b - a * ratio) / denominator;
    }
    else {
      float ratio = c / d;
      float denominator = c * ratio + d;
      x = (a * ratio + b) / denominator;
      y = (b * ratio - a) / denominator;
    }
  }

  // C99 Annex G.5.1 example: libc++ std::complex operator/ and compiler-rt
  // __divsc3.
  void logbScalbn(float a, float b, float c, float d, float& x, float& y) {
    int ilogbw = 0;
    float logbw = std::logb(std::fmax(std::fabs(c), std::fabs(d)));
    if (std::isfinite(logbw)) {
      ilogbw = static_cast<int>(logbw);
      c = std::scalbn(c, -ilogbw);
      d = std::scalbn(d, -ilogbw);
    }
    float denominator = c * c + d * d;
    x = std::scalbn((a * c + b * d) / denominator, -ilogbw);
    y = std::scalbn((b * c - a * d) / denominator, -ilogbw);
    if (std::isnan(x) && std::isnan(y)) {
      if (denominator == 0.0f && (!std::isnan(a) || !std::isnan(b))) {
        x = std::copysign(INFINITY, c) * a;
        y = std::copysign(INFINITY, c) * b;
      }
      else if ((std::isinf(a) || std::isinf(b)) && std::isfinite(c) && std::isfinite(d)) {
        a = std::copysign(std::isinf(a) ? 1.0f : 0.0f, a);
        b = std::copysign(std::isinf(b) ? 1.0f : 0.0f, b);
        x = INFINITY * (a * c + b * d);
        y = INFINITY * (b * c - a * d);
      }
      else if (std::isinf(logbw) && logbw > 0.0f && std::isfinite(a) && std::isfinite(b)) {
        c = std::copysign(std::isinf(c) ? 1.0f : 0.0f, c);
        d = std::copysign(std::isinf(d) ? 1.0f : 0.0f, d);
        x = 0.0f * (a * c + b * d);
        y = 0.0f * (b * c - a * d);
      }
    }
  }

  // Naive formula in double, rounded once to float.
  void wideDouble(float a, float b, float c, float d, float& x, float& y) {
    double denominator = static_cast<double>(c) * c + static_cast<double>(d) * d;
    x = static_cast<float>((static_cast<double>(a) * c + static_cast<double>(b) * d) / denominator);
    y = static_cast<float>((static_cast<double>(b) * c - static_cast<double>(a) * d) / denominator);
  }
  void runtimeDivide(const float* a, const float* b, const float* c, const float* d, float* re, float* im, int n) {
    for (int i = 0; i < n; ++i) {
      __complex__ float result = __divsc3(a[i], b[i], c[i], d[i]);
      re[i] = __real__ result;
      im[i] = __imag__ result;
    }
  }

  void separateSinCos(const float* x, float* c, float* s, int n) {
    float (*volatile cosine)(float) = cosf;
    float (*volatile sine)(float) = sinf;
    for (int i = 0; i < n; ++i) {
      c[i] = cosine(x[i]);
      s[i] = sine(x[i]);
    }
  }

  void reciprocal(float a, float b, float c, float d, float& x, float& y) {
    float inverse = 1.0f / (c * c + d * d);
    x = (a * c + b * d) * inverse;
    y = (b * c - a * d) * inverse;
  }
} // namespace complex_algos
