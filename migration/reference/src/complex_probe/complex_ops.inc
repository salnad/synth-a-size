// Complex division as upstream code writes it, compiled once with the
// reference flags (complex_ops_fast.cpp) and once strict
// (complex_ops_strict.cpp). OPS_NS names the copy.

#include <complex>

namespace OPS_NS {

// value / std::complex<float>, as in PhaseModifierKeyframe::render.
void stdRealDivide(float value, const float* c, const float* d, float* re, float* im, int n) {
  for (int i = 0; i < n; ++i) {
    std::complex<float> result = value / std::complex<float>(c[i], d[i]);
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
void builtinRealDivide(float value, const float* c, const float* d, float* re, float* im, int n) {
  for (int i = 0; i < n; ++i) {
    __complex__ float z;
    __real__ z = c[i];
    __imag__ z = d[i];
    __complex__ float result = value / z;
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

} // namespace OPS_NS
