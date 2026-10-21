// The generated test inputs, built without fast-math (CMakeLists.txt) so they
// are identical on every compiler: Apple clang's -ffast-math rewrites the
// divisions below into reciprocal multiplies, 1 ulp off, while GCC keeps them.
// This file includes no headers on purpose: a per-file flag on a translation
// unit that instantiates inline code shared with the engine (nlohmann::json,
// std::complex, ...) lets the linker use the strict copy in engine code too.

namespace refsig {
  float ramp(float from, float to, long long frame, long long total_frames) {
    return total_frames > 1 ? from + (to - from) * (frame / float(total_frames - 1)) : from;
  }

  // kind: 0 sine, 1 saw, 2 square.
  float periodic(int kind, long long frame, int sample_rate, double freq, double phase) {
    double t = static_cast<double>(frame) / sample_rate;
    double cycle = freq * t + phase;
    double fractional = cycle - __builtin_floor(cycle);
    if (kind == 0)
      return static_cast<float>(__builtin_sin(2.0 * 3.14159265358979323846 * cycle));
    if (kind == 1)
      return static_cast<float>(2.0 * fractional - 1.0);
    return fractional < 0.5 ? 1.0f : -1.0f;
  }
} // namespace refsig
