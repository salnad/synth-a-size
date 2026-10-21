// Built without fast-math (CMakeLists.txt) so the ramp test input is the exact
// division on every compiler: Apple clang's -ffast-math turns it into a
// reciprocal multiply, 1 ulp off. Kept free of shared headers, because a
// per-file flag on a translation unit that instantiates inline code shared
// with the engine (nlohmann::json, std::complex, ...) lets the linker pick
// the strict copy for engine code too.

namespace refsig {
  float ramp(float from, float to, long long frame, long long total_frames) {
    return total_frames > 1 ? from + (to - from) * (frame / float(total_frames - 1)) : from;
  }
} // namespace refsig
