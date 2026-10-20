// ref-libm-probe: hashes the results of every libm function the reference
// binary imports over a fixed, deterministic set of inputs. Running it on two
// machines shows whether the C math library itself returns different bits
// there (e.g. different CPU-dispatched variants of sin/exp/pow).
//
// Built WITHOUT -ffast-math so the calls are not replaced or reassociated.

#include <cinttypes>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <random>
#include <string>
#include <vector>

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
  std::printf("\n}\n");
  return 0;
}
