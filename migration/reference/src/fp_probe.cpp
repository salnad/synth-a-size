// ref-fp-probe: hashes the results of (a) every libm function the reference
// binary imports, (b) the SSE approximate instructions rcpps / rsqrtps and
// (c) the glibc libmvec SSE kernels GCC can call from auto-vectorised loops,
// over fixed deterministic inputs. Running it on two machines shows whether
// either source returns different bits there. libm can differ through
// CPU-dispatched variants; rcpps / rsqrtps are only specified to ~12 bits, so
// their exact results are implementation-defined and differ between vendors.
//
// Built WITHOUT -ffast-math so the calls are not replaced or reassociated.

#include <emmintrin.h>
#include <xmmintrin.h>

#include <cinttypes>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <random>
#include <string>
#include <vector>

// glibc libmvec SSE entry points, as GCC calls them when it auto-vectorises
// scalar math under -ffast-math.
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

namespace {

uint64_t fnv(uint64_t hash, const void* data, size_t bytes) {
  const unsigned char* p = static_cast<const unsigned char*>(data);
  for (size_t i = 0; i < bytes; ++i) {
    hash ^= p[i];
    hash *= 1099511628211ULL;
  }
  return hash;
}

template<typename T, typename F>
void probe(const char* name, F function, T lo, T hi, bool& first) {
  std::mt19937_64 engine(12345);
  std::uniform_real_distribution<double> distribution(0.0, 1.0);
  uint64_t hash = 1469598103934665603ULL;
  const int kCount = 2000000;
  for (int i = 0; i < kCount; ++i) {
    T x = static_cast<T>(lo + (hi - lo) * distribution(engine));
    T y = function(x);
    hash = fnv(hash, &y, sizeof(y));
  }
  std::printf("%s  \"%s\": \"%016" PRIx64 "\"", first ? "" : ",\n", name, hash);
  first = false;
}

} // namespace

int main() {
  bool first = true;
  std::printf("{\n");
  probe<float>("sinf", [](float x) { return std::sin(x); }, -10.0f, 10.0f, first);
  probe<float>("cosf", [](float x) { return std::cos(x); }, -10.0f, 10.0f, first);
  probe<float>("tanf", [](float x) { return std::tan(x); }, -1.5f, 1.5f, first);
  probe<float>("expf", [](float x) { return std::exp(x); }, -20.0f, 20.0f, first);
  probe<float>("exp2f", [](float x) { return std::exp2(x); }, -20.0f, 20.0f, first);
  probe<float>("logf", [](float x) { return std::log(x); }, 1e-6f, 1e6f, first);
  probe<float>("log10f", [](float x) { return std::log10(x); }, 1e-6f, 1e6f, first);
  probe<float>("powf", [](float x) { return std::pow(x, 1.7f); }, 0.0f, 20.0f, first);
  probe<float>("tanhf", [](float x) { return std::tanh(x); }, -5.0f, 5.0f, first);
  probe<float>("acosf", [](float x) { return std::acos(x); }, -1.0f, 1.0f, first);
  probe<float>("atan2f", [](float x) { return std::atan2(x, 0.37f); }, -10.0f, 10.0f, first);
  probe<float>("cbrtf", [](float x) { return std::cbrt(x); }, -10.0f, 10.0f, first);
  probe<double>("sin", [](double x) { return std::sin(x); }, -10.0, 10.0, first);
  probe<double>("cos", [](double x) { return std::cos(x); }, -10.0, 10.0, first);
  probe<double>("tan", [](double x) { return std::tan(x); }, -1.5, 1.5, first);
  probe<double>("exp", [](double x) { return std::exp(x); }, -20.0, 20.0, first);
  probe<double>("exp2", [](double x) { return std::exp2(x); }, -20.0, 20.0, first);
  probe<double>("log", [](double x) { return std::log(x); }, 1e-6, 1e6, first);
  probe<double>("log10", [](double x) { return std::log10(x); }, 1e-6, 1e6, first);
  probe<double>("pow", [](double x) { return std::pow(x, 1.7); }, 0.0, 20.0, first);
  probe<double>("tanh", [](double x) { return std::tanh(x); }, -5.0, 5.0, first);
  probe<double>("atan", [](double x) { return std::atan(x); }, -10.0, 10.0, first);
  probe<double>("asin", [](double x) { return std::asin(x); }, -1.0, 1.0, first);
  probe<double>("acos", [](double x) { return std::acos(x); }, -1.0, 1.0, first);
  probe<double>("sinh", [](double x) { return std::sinh(x); }, -5.0, 5.0, first);
  probe<double>("cosh", [](double x) { return std::cosh(x); }, -5.0, 5.0, first);
  probe<float>("rcpps", [](float x) { return _mm_cvtss_f32(_mm_rcp_ps(_mm_set1_ps(x))); }, 0.01f, 100.0f, first);
  probe<float>("rsqrtps", [](float x) { return _mm_cvtss_f32(_mm_rsqrt_ps(_mm_set1_ps(x))); }, 0.01f, 100.0f, first);
  // The sequence GCC emits for vectorised a / b under -ffast-math:
  // r = rcpps(b); a * (r + r - b * r * r), i.e. one Newton-Raphson step.
  probe<float>("rcpps_newton_div", [](float x) {
    __m128 b = _mm_set1_ps(x);
    __m128 r = _mm_rcp_ps(b);
    __m128 refined = _mm_sub_ps(_mm_add_ps(r, r), _mm_mul_ps(b, _mm_mul_ps(r, r)));
    return _mm_cvtss_f32(_mm_mul_ps(_mm_set1_ps(1.2345f), refined));
  }, 0.01f, 100.0f, first);
  probe<float>("exact_div", [](float x) { return 1.2345f / x; }, 0.01f, 100.0f, first);
  probe<float>("libmvec_sinf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_sinf(_mm_set1_ps(x))); }, -10.0f, 10.0f, first);
  probe<float>("libmvec_cosf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_cosf(_mm_set1_ps(x))); }, -10.0f, 10.0f, first);
  probe<float>("libmvec_tanf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_tanf(_mm_set1_ps(x))); }, -1.5f, 1.5f, first);
  probe<float>("libmvec_asinf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_asinf(_mm_set1_ps(x))); }, -1.0f, 1.0f, first);
  probe<float>("libmvec_acosf", [](float x) { return _mm_cvtss_f32(_ZGVbN4v_acosf(_mm_set1_ps(x))); }, -1.0f, 1.0f, first);
  probe<float>("libmvec_atan2f", [](float x) { return _mm_cvtss_f32(_ZGVbN4vv_atan2f(_mm_set1_ps(x), _mm_set1_ps(0.37f))); }, -10.0f, 10.0f, first);
  probe<double>("libmvec_sin", [](double x) { return _mm_cvtsd_f64(_ZGVbN2v_sin(_mm_set1_pd(x))); }, -10.0, 10.0, first);
  probe<double>("libmvec_cos", [](double x) { return _mm_cvtsd_f64(_ZGVbN2v_cos(_mm_set1_pd(x))); }, -10.0, 10.0, first);
  probe<double>("libmvec_exp", [](double x) { return _mm_cvtsd_f64(_ZGVbN2v_exp(_mm_set1_pd(x))); }, -20.0, 20.0, first);
  std::printf("\n}\n");
  return 0;
}
