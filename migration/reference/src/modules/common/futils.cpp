// Module "futils_scalar": the scalar (mono_float) math helpers non-processor code
// uses, over a fixed input grid. Artifact-only.
// Ports: port/crates/common (scalar.rs). The grid is defined identically in
// port/crates/common/tests/parity.rs.

#include "module_registry.h"

#include "futils.h"
#include "matrix.h"
#include "poly_utils.h"
#include "utils.h"

#include <cmath>
#include <cstring>

namespace {
  // Grid: x = i * 0.01 for i in [-4000, 4000], then 20000 bit patterns from a
  // 32-bit LCG kept when their exponent field is in [100, 133] (|x| in about
  // [2^-27, 2^7)); filtered with integer ops so fast-math cannot fold it.
  std::vector<float> grid() {
    std::vector<float> xs;
    for (int i = -4000; i <= 4000; ++i)
      xs.push_back(i * 0.01f);
    unsigned state = 12345u;
    for (int i = 0; i < 20000; ++i) {
      state = state * 1664525u + 1013904223u;
      unsigned exponent = (state >> 23) & 0xffu;
      if (exponent >= 100u && exponent <= 133u) {
        float value;
        memcpy(&value, &state, sizeof(value));
        xs.push_back(value);
      }
    }
    return xs;
  }

  // Wide-range results as mantissa in [1, 2) (sign kept) and integer
  // exponent: both exact, and each fits the absolute module gate.
  void pushSplit(std::vector<float>& mantissas, std::vector<float>& exponents, float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof(bits));
    exponents.push_back(static_cast<float>(static_cast<int>((bits >> 23) & 0xffu) - 127));
    bits = (bits & 0x807fffffu) | (127u << 23);
    float mantissa;
    memcpy(&mantissa, &bits, sizeof(mantissa));
    mantissas.push_back(mantissa);
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "futils_scalar",
  "Scalar futils/utils over a fixed grid: arrays exp2 (of e = x * 0.1, scaled by 2^-floor(e)), log2 (of |x|, x != 0), tanh, "
  "power_scale (101 values x 81 powers), catmull (65 t on a fixed 4-sample buffer), "
  "exp (of x, |x| <= 80) and pow (|x|, x != 0, to 8 exponents) as mantissa/exponent pairs, log (of |x|, x != 0)",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext&, refmod::ArtifactSink& sink, std::string&) {
    std::vector<float> xs = grid();
    std::vector<float> exp2, log2, tanh;
    for (float x : xs) {
      // Scaled by an exact power of two into about [1, 2): raw values reach
      // ~6e3, where one float ulp exceeds the absolute module gate.
      float e = x * 0.1f;
      exp2.push_back(ldexpf(vital::futils::exp2(e), -static_cast<int>(std::floor(e))));
      tanh.push_back(vital::futils::tanh(x));
      if (x != 0.0f)
        log2.push_back(vital::futils::log2(fabsf(x)));
    }
    sink.array("exp2", exp2, 1);
    sink.array("log2", log2, 1);
    sink.array("tanh", tanh, 1);

    std::vector<float> power_scale;
    for (int i = 0; i <= 100; ++i) {
      for (int p = -40; p <= 40; ++p)
        power_scale.push_back(vital::futils::powerScale(i / 100.0f, p * 0.25f));
    }
    sink.array("power_scale", power_scale, 1);

    std::vector<float> catmull;
    float buffer[4] = { 0.3f, -0.7f, 0.9f, 0.1f };
    for (int i = 0; i <= 64; ++i) {
      vital::matrix weights = vital::utils::getCatmullInterpolationMatrix(i / 64.0f);
      vital::matrix values = vital::utils::getValueMatrix(buffer, vital::poly_int(0));
      values.transpose();
      catmull.push_back(weights.multiplyAndSumRows(values)[0]);
    }
    sink.array("catmull", catmull, 1);

    const float kPowExponents[] = { -2.5f, -1.0f, -0.5f, 0.25f, 0.5f, 1.0f, 1.5f, 3.0f };
    std::vector<float> exp_mantissa, exp_exponent, log, pow_mantissa, pow_exponent;
    for (float x : xs) {
      if (fabsf(x) <= 80.0f)
        pushSplit(exp_mantissa, exp_exponent, vital::futils::exp(x));
      if (x != 0.0f) {
        log.push_back(vital::futils::log(fabsf(x)));
        for (float exponent : kPowExponents)
          pushSplit(pow_mantissa, pow_exponent, vital::futils::pow(fabsf(x), exponent));
      }
    }
    sink.array("exp_mantissa", exp_mantissa, 1);
    sink.array("exp_exponent", exp_exponent, 1);
    sink.array("log", log, 1);
    sink.array("pow_mantissa", pow_mantissa, 1);
    sink.array("pow_exponent", pow_exponent, 1);
    return true;
  },
});
