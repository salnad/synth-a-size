// Shared helpers for the reference harness CLIs.
#pragma once

#include <cstdint>
#include <map>
#include <string>
#include <vector>

#include "json/json.h"

namespace harness {

using json = nlohmann::json;

// Points HOME (and XDG dirs) at a fresh empty temp directory so the engine
// never reads a developer's user config, user presets or MIDI-learn mappings.
// Must run before any JUCE or engine object is created.
void isolateUserEnvironment();

// Re-execs the process once with GLIBC_TUNABLES set so glibc picks the
// baseline (non-FMA/AVX) libm variants on every x86_64 CPU. Call first in
// main(). Set REF_NO_LIBM_PIN=1 to disable.
void pinLibmVariants(int argc, const char* argv[]);

// Information about how this reference binary was built and run.
json buildInfo();

enum class WavFormat { kFloat32, kPcm16, kPcm24 };
bool parseWavFormat(const std::string& name, WavFormat& format);
const char* wavFormatName(WavFormat format);

// Writes interleaved samples as a RIFF/WAVE file. Float32 output is written
// bit-exact (WAVE_FORMAT_IEEE_FLOAT); PCM output is clipped to [-1, 1] and
// rounded to nearest.
bool writeWav(const std::string& path, const std::vector<float>& interleaved, int channels,
              int sample_rate, WavFormat format, std::string& error);

// NumPy .npy v1.0, little-endian float32, C order.
bool writeNpyF32(const std::string& path, const float* data, const std::vector<size_t>& shape,
                 std::string& error);

// Reads a little-endian .npy file with dtype <f4, <f8, <i2 or <i4 (ints are
// scaled to [-1, 1)) into float32. Shape is returned as stored.
bool readNpy(const std::string& path, std::vector<float>& data, std::vector<size_t>& shape,
             std::string& error);

uint64_t fnv1a64(const void* data, size_t bytes, uint64_t seed = 1469598103934665603ULL);
std::string hex64(uint64_t value);

bool readTextFile(const std::string& path, std::string& contents);
bool writeTextFile(const std::string& path, const std::string& contents);
bool ensureDirectory(const std::string& path);

// Minimal "--key value" / "--flag" / positional argument parser.
class Args {
  public:
    Args(int argc, const char* argv[], const std::vector<std::string>& flag_names);

    bool has(const std::string& key) const { return values_.count(key) > 0; }
    std::string get(const std::string& key, const std::string& fallback = "") const;
    double getDouble(const std::string& key, double fallback) const;
    long long getInt(const std::string& key, long long fallback) const;
    const std::vector<std::string>& positional() const { return positional_; }
    const std::vector<std::string>& unknown() const { return unknown_; }

  private:
    std::map<std::string, std::string> values_;
    std::vector<std::string> positional_;
    std::vector<std::string> unknown_;
};

} // namespace harness
