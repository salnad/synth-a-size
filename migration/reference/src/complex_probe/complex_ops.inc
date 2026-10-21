// Complex division as upstream code writes it, compiled once with the
// reference flags (complex_ops_fast.cpp) and once strict
// (complex_ops_strict.cpp). OPS_NS names the copy.

#include <complex>
#include <math.h>

namespace OPS_NS {

// 1.0f / std::complex<float> with the literal numerator, as in
// PhaseModifierKeyframe::render (fast-math may fold 1 * c and 0 * d).
void stdRealDivide(const float* c, const float* d, float* re, float* im, int n) {
  for (int i = 0; i < n; ++i) {
    std::complex<float> result = 1.0f / std::complex<float>(c[i], d[i]);
    re[i] = result.real();
    im[i] = result.imag();
  }
}

void stdDivide(const float* a, const float* b, const float* c, const float* d, float* re, float* im, int n) {
  for (int i = 0; i < n; ++i) {
    std::complex<float> result = std::complex<float>(a[i], b[i]) / std::complex<float>(c[i], d[i]);
    re[i] = result.real();
    im[i] = result.imag();
  }
}

// The compiler's own complex division (GCC/Clang `_Complex float`): inline
// under -fcx-limited-range / -ffast-math, otherwise the runtime's __divsc3
// (libgcc on Linux, compiler-rt on macOS).
void builtinRealDivide(const float* c, const float* d, float* re, float* im, int n) {
  for (int i = 0; i < n; ++i) {
    __complex__ float z;
    __real__ z = c[i];
    __imag__ z = d[i];
    __complex__ float result = 1.0f / z;
    re[i] = __real__ result;
    im[i] = __imag__ result;
  }
}

void builtinDivide(const float* a, const float* b, const float* c, const float* d, float* re, float* im, int n) {
  for (int i = 0; i < n; ++i) {
    __complex__ float x;
    __real__ x = a[i];
    __imag__ x = b[i];
    __complex__ float z;
    __real__ z = c[i];
    __imag__ z = d[i];
    __complex__ float result = x / z;
    re[i] = __real__ result;
    im[i] = __imag__ result;
  }
}

// cosf and sinf of the same argument, which Clang on Apple targets merges
// into __sincosf_stret.
void pairedSinCos(const float* x, float* c, float* s, int n) {
  for (int i = 0; i < n; ++i) {
    c[i] = cosf(x[i]);
    s[i] = sinf(x[i]);
  }
}

} // namespace OPS_NS
