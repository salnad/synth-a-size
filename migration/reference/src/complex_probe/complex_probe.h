#pragma once

#include <cstdint>
#include <vector>

#define COMPLEX_OPS_DECLARE(ns)                                                                     \
  namespace ns {                                                                                   \
    void stdRealDivide(float value, const float* c, const float* d, float* re, float* im, int n);  \
    void stdDivide(const float* a, const float* b, const float* c, const float* d, float* re,      \
                   float* im, int n);                                                              \
    void builtinRealDivide(float value, const float* c, const float* d, float* re, float* im,      \
                           int n);                                                                 \
    void builtinDivide(const float* a, const float* b, const float* c, const float* d, float* re,  \
                       float* im, int n);                                                          \
  }

COMPLEX_OPS_DECLARE(fast_ops)
COMPLEX_OPS_DECLARE(strict_ops)

// Built strict (-fno-fast-math -ffp-contract=off), so inputs and the
// candidate algorithms are the same on every machine.
namespace complex_algos {
  struct Inputs {
    std::vector<float> a, b, c, d;
  };

  // (c, d) on the unit circle: c uniform in [-1, 1], d = ±sqrt(1 - c²).
  Inputs unitCircle(uint64_t seed, int n);
  // Each part uniform in [-2, 2] (|z| near 1, like products of unit shifts).
  Inputs nearUnit(uint64_t seed, int n);
  // Random sign and mantissa, exponents uniform in [min_exp, max_exp]
  // (-150 gives subnormals).
  Inputs randomExponent(uint64_t seed, int n, int min_exp, int max_exp);
  // Every combination of 0, -0, subnormal, FLT_MIN, 1, -3, FLT_MAX, inf, nan.
  Inputs specialGrid();
  // Uniform floats in [lo, hi].
  std::vector<float> uniform(uint64_t seed, int n, float lo, float hi);

  // Candidate algorithms for (a + bi) / (c + di).
  void naive(float a, float b, float c, float d, float& x, float& y);
  void smith(float a, float b, float c, float d, float& x, float& y);
  void logbScalbn(float a, float b, float c, float d, float& x, float& y);
  void wideDouble(float a, float b, float c, float d, float& x, float& y);
} // namespace complex_algos
