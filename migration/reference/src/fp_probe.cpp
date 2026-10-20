// ref-fp-probe: hashes floating-point results that can differ between
// machines, ISAs or math libraries, over fixed deterministic inputs:
//
//   (a) every scalar libm function the reference binary imports;
//   (b) x86: the SSE approximate instructions rcpps / rsqrtps (only specified
//       to ~12 bits, so their exact bits are implementation-defined) and the
//       glibc libmvec SSE kernels GCC calls from auto-vectorised loops;
//   (c) aarch64: FRECPE / FRSQRTE (bit-exact by the Arm architecture spec),
//       upstream's NEON mulAdd (vmlaq_f32) as compiled with the platform's
//       default FP contraction, and std::fma / unfused a + b * c controls;
//   (d) upstream's poly_float toInt semantics per ISA (_mm_cvtps_epi32 rounds
//       to nearest, vcvtq_s32_f32 truncates).
//
// Inputs come from splitmix64, not <random> distributions, so they are
// bit-identical under libstdc++ and libc++ and hashes compare across
// platforms. Built WITHOUT -ffast-math and with the compiler's default FP
// contraction, deliberately, so (c) shows what the default does.

#if defined(__SSE2__)
  #include <emmintrin.h>
  #include <xmmintrin.h>
#endif
#if defined(__aarch64__)
  #include <arm_neon.h>
#endif

#include <cinttypes>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>

#if REF_PROBE_LIBMVEC
extern "C" {
  __m128 _ZGVbN4v_sinf(__m128);
  __m128 _ZGVbN4v_cosf(__m128);
  __m128 _ZGVbN4v_tanf(__m128);
  __m128 _ZGVbN4v_asinf(__m128);
  __m128 _ZGVbN4v_acosf(__m128);
  __m128 _ZGVbN4vv_atan2f(__m128, __m128);
  __m128d _ZGVbN2v_sin(__m128d);
  __m128d _ZGVbN2v_cos(__m128d);
  __m128d _ZGVbN2v_exp(__m128d);
}
#endif

namespace {

uint64_t fnv(uint64_t hash, const void* data, size_t bytes) {
  const unsigned char* p = static_cast<const unsigned char*>(data);
  for (size_t i = 0; i < bytes; ++i) {
    hash ^= p[i];
    hash *= 1099511628211ULL;
  }
  return hash;
}

uint64_t splitmix64(uint64_t& state) {
  uint64_t z = (state += 0x9e3779b97f4a7c15ULL);
  z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
  z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
  return z ^ (z >> 31);
}

bool first_entry = true;

template<typename T, typename F>
void probe(const char* name, F function, double lo, double hi) {
  uint64_t state = 12345;
  uint64_t hash = 1469598103934665603ULL;
  const int kCount = 2000000;
  for (int i = 0; i < kCount; ++i) {
    double unit = static_cast<double>(splitmix64(state) >> 11) * (1.0 / 9007199254740992.0);
    T x = static_cast<T>(lo + (hi - lo) * unit);
    auto y = function(x);
    hash = fnv(hash, &y, sizeof(y));
  }
  std::printf("%s  \"%s\": \"%016" PRIx64 "\"", first_entry ? "" : ",\n", name, hash);
  first_entry = false;
}

// Keeps the compiler from fusing the multiply into the add.
float unfusedMulAdd(float a, float b, float c) {
  volatile float product = b * c;
  return a + product;
}

} // namespace

int main() {
  std::printf("{\n");
  probe<float>("sinf", [](float x) { return std::sin(x); }, -10.0, 10.0);
  probe<float>("cosf", [](float x) { return std::cos(x); }, -10.0, 10.0);
  probe<float>("tanf", [](float x) { return std::tan(x); }, -1.5, 1.5);
  probe<float>("expf", [](float x) { return std::exp(x); }, -20.0, 20.0);
  probe<float>("exp2f", [](float x) { return std::exp2(x); }, -20.0, 20.0);
  probe<float>("logf", [](float x) { return std::log(x); }, 1e-6, 1e6);
  probe<float>("log10f", [](float x) { return std::log10(x); }, 1e-6, 1e6);
  probe<float>("powf", [](float x) { return std::pow(x, 1.7f); }, 0.0, 20.0);
  probe<float>("tanhf", [](float x) { return std::tanh(x); }, -5.0, 5.0);
  probe<float>("acosf", [](float x) { return std::acos(x); }, -1.0, 1.0);
  probe<float>("atan2f", [](float x) { return std::atan2(x, 0.37f); }, -10.0, 10.0);
  probe<float>("cbrtf", [](float x) { return std::cbrt(x); }, -10.0, 10.0);
  probe<float>("sqrtf", [](float x) { return std::sqrt(x); }, 0.0, 100.0);
  probe<double>("sin", [](double x) { return std::sin(x); }, -10.0, 10.0);
  probe<double>("cos", [](double x) { return std::cos(x); }, -10.0, 10.0);
  probe<double>("tan", [](double x) { return std::tan(x); }, -1.5, 1.5);
  probe<double>("exp", [](double x) { return std::exp(x); }, -20.0, 20.0);
  probe<double>("exp2", [](double x) { return std::exp2(x); }, -20.0, 20.0);
  probe<double>("log", [](double x) { return std::log(x); }, 1e-6, 1e6);
  probe<double>("log10", [](double x) { return std::log10(x); }, 1e-6, 1e6);
  probe<double>("pow", [](double x) { return std::pow(x, 1.7); }, 0.0, 20.0);
  probe<double>("tanh", [](double x) { return std::tanh(x); }, -5.0, 5.0);
  probe<double>("atan", [](double x) { return std::atan(x); }, -10.0, 10.0);
  probe<double>("asin", [](double x) { return std::asin(x); }, -1.0, 1.0);
  probe<double>("acos", [](double x) { return std::acos(x); }, -1.0, 1.0);
  probe<double>("sinh", [](double x) { return std::sinh(x); }, -5.0, 5.0);
  probe<double>("cosh", [](double x) { return std::cosh(x); }, -5.0, 5.0);

  probe<float>("exact_div", [](float x) { return 1.2345f / x; }, 0.01, 100.0);
  probe<float>("fma", [](float x) { return std::fma(x, 1.0001f, 0.3333f); }, -10.0, 10.0);
  probe<float>("unfused_mul_add", [](float x) { return unfusedMulAdd(0.3333f, x, 1.0001f); }, -10.0, 10.0);
  probe<float>("nearbyint_to_int", [](float x) { return static_cast<int32_t>(std::nearbyint(x)); }, -1000.0, 1000.0);
  // Upstream's random phase (synth_oscillator.cpp:583) converts values in
  // [-INT_MAX, INT_MAX) to uint32_t. Negative float -> unsigned is undefined
  // behaviour: x86 GCC goes through a 64-bit cvttss2si and wraps modulo 2^32,
  // AArch64 FCVTZU saturates to 0. Volatile keeps it a run-time conversion.
  probe<float>("float_to_uint32_negative", [](float x) {
    volatile float v = x * static_cast<float>(INT32_MAX);
    return static_cast<uint32_t>(v);
  }, -1.0, 0.0);
  // Out-of-range float -> int32_t (also undefined): x86 returns 0x80000000,
  // AArch64 saturates.
  probe<float>("float_to_int32_overflow", [](float x) {
    volatile float v = x;
    return static_cast<int32_t>(v);
  }, 2147483648.0, 8589934592.0);

#if defined(__SSE2__)
  probe<float>("rcpps", [](float x) { return _mm_cvtss_f32(_mm_rcp_ps(_mm_set1_ps(x))); }, 0.01, 100.0);
  probe<float>("rsqrtps", [](float x) { return _mm_cvtss_f32(_mm_rsqrt_ps(_mm_set1_ps(x))); }, 0.01, 100.0);
  // The sequence GCC emits for vectorised a / b under -ffast-math:
  // r = rcpps(b); a * (r + r - b * r * r), i.e. one Newton-Raphson step.
  probe<float>("rcpps_newton_div", [](float x) {
    __m128 b = _mm_set1_ps(x);
    __m128 r = _mm_rcp_ps(b);
    __m128 refined = _mm_sub_ps(_mm_add_ps(r, r), _mm_mul_ps(b, _mm_mul_ps(r, r)));
    return _mm_cvtss_f32(_mm_mul_ps(_mm_set1_ps(1.2345f), refined));
  }, 0.01, 100.0);
  // upstream utils::toInt on SSE2 (round to nearest even under default MXCSR).
  probe<float>("poly_toInt", [](float x) { return _mm_cvtsi128_si32(_mm_cvtps_epi32(_mm_set1_ps(x))); }, -1000.0, 1000.0);
#endif

#if defined(__aarch64__)
  probe<float>("frecpe", [](float x) { return vgetq_lane_f32(vrecpeq_f32(vdupq_n_f32(x)), 0); }, 0.01, 100.0);
  probe<float>("frsqrte", [](float x) { return vgetq_lane_f32(vrsqrteq_f32(vdupq_n_f32(x)), 0); }, 0.01, 100.0);
  // upstream poly_float::mulAdd on NEON, compiled with default contraction.
  probe<float>("neon_vmlaq", [](float x) {
    return vgetq_lane_f32(vmlaq_f32(vdupq_n_f32(0.3333f), vdupq_n_f32(x), vdupq_n_f32(1.0001f)), 0);
  }, -10.0, 10.0);
  // upstream utils::toInt on NEON (truncation toward zero).
  probe<float>("poly_toInt", [](float x) { return vgetq_lane_s32(vcvtq_s32_f32(vdupq_n_f32(x)), 0); }, -1000.0, 1000.0);
#endif

#if REF_PROBE_LIBMVEC
  probe<float>("libmvec_sinf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_sinf(_mm_set1_ps(x))); }, -10.0, 10.0);
  probe<float>("libmvec_cosf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_cosf(_mm_set1_ps(x))); }, -10.0, 10.0);
  probe<float>("libmvec_tanf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_tanf(_mm_set1_ps(x))); }, -1.5, 1.5);
  probe<float>("libmvec_asinf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_asinf(_mm_set1_ps(x))); }, -1.0, 1.0);
  probe<float>("libmvec_acosf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_acosf(_mm_set1_ps(x))); }, -1.0, 1.0);
  probe<float>("libmvec_atan2f", [](float x) { return _mm_cvtss_f32(_ZGVbN4vv_atan2f(_mm_set1_ps(x), _mm_set1_ps(0.37f))); }, -10.0, 10.0);
  probe<double>("libmvec_sin", [](double x) { return _mm_cvtsd_f64(_ZGVbN2v_sin(_mm_set1_pd(x))); }, -10.0, 10.0);
  probe<double>("libmvec_cos", [](double x) { return _mm_cvtsd_f64(_ZGVbN2v_cos(_mm_set1_pd(x))); }, -10.0, 10.0);
  probe<double>("libmvec_exp", [](double x) { return _mm_cvtsd_f64(_ZGVbN2v_exp(_mm_set1_pd(x))); }, -20.0, 20.0);
#endif
  std::printf("\n}\n");
  return 0;
}
