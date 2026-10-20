#include "harness_io.h"

#include <cerrno>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <sstream>
#include <sys/stat.h>
#include <unistd.h>

#if defined(__GLIBC__)
  #include <gnu/libc-version.h>
#endif

namespace harness {

namespace {
  void writeU16(std::ostream& out, uint16_t value) {
    char bytes[2] = { char(value & 0xff), char((value >> 8) & 0xff) };
    out.write(bytes, 2);
  }

  void writeU32(std::ostream& out, uint32_t value) {
    char bytes[4] = { char(value & 0xff), char((value >> 8) & 0xff),
                      char((value >> 16) & 0xff), char((value >> 24) & 0xff) };
    out.write(bytes, 4);
  }

  float clampUnit(float value) {
    if (!(value > -1.0f))
      return -1.0f;
    if (value > 1.0f)
      return 1.0f;
    return value;
  }
} // namespace

void isolateUserEnvironment() {
  char templ[] = "/tmp/ref-harness-home-XXXXXX";
  const char* dir = mkdtemp(templ);
  if (dir == nullptr)
    return;

  setenv("HOME", dir, 1);
  unsetenv("XDG_CONFIG_HOME");
  unsetenv("XDG_DATA_HOME");
  unsetenv("XDG_CACHE_HOME");
}

namespace {
  // Masks the CPU features glibc uses to pick libm variants (FMA/AVX/FMA4
  // versions of sin, exp, pow, ...), so every x86_64 CPU runs the baseline
  // SSE2 variants. Measured: the double-precision variants return different
  // bits with and without FMA (migration/decisions/reference-determinism.md).
  const char* kLibmTunables = "glibc.cpu.hwcaps=-AVX,-AVX2,-FMA,-FMA4";
  const char* kPinnedMarker = "REF_HARNESS_LIBM_PINNED";
  const char* kNoPinVariable = "REF_NO_LIBM_PIN";
}

void pinLibmVariants(int argc, const char* argv[]) {
#if defined(__GLIBC__) && defined(__x86_64__)
  (void)argc;
  if (getenv(kPinnedMarker) || getenv(kNoPinVariable))
    return;

  // ifunc variants are chosen when libm is loaded, so the tunable only
  // takes effect in a fresh process image: set it and re-exec ourselves.
  std::string tunables = kLibmTunables;
  const char* existing = getenv("GLIBC_TUNABLES");
  if (existing && *existing)
    tunables = std::string(existing) + ":" + tunables;
  setenv("GLIBC_TUNABLES", tunables.c_str(), 1);
  setenv(kPinnedMarker, "1", 1);
  execv("/proc/self/exe", const_cast<char* const*>(argv));
  std::fprintf(stderr, "warning: could not re-exec to pin libm variants: %s\n", std::strerror(errno));
  unsetenv(kPinnedMarker);
#else
  (void)argc;
  (void)argv;
#endif
}

json buildInfo() {
  json info;
  info["upstream_sha"] = REF_UPSTREAM_SHA;
  info["fast_math"] = REF_FAST_MATH ? true : false;
  info["opt_flags"] = REF_OPT_FLAGS;
#if defined(__GLIBC__)
  info["glibc"] = gnu_get_libc_version();
#endif
  const char* pinned = getenv(kPinnedMarker);
  info["libm_variants"] = pinned ? std::string("baseline (GLIBC_TUNABLES ") + kLibmTunables + ")"
                                 : std::string("cpu-dispatched (not pinned)");
#if defined(__VERSION__)
  info["compiler"] = __VERSION__;
#endif
#if defined(__x86_64__)
  info["arch"] = "x86_64";
#elif defined(__aarch64__)
  info["arch"] = "aarch64";
#else
  info["arch"] = "unknown";
#endif
  return info;
}

bool parseWavFormat(const std::string& name, WavFormat& format) {
  if (name == "f32" || name == "float32" || name == "32f") {
    format = WavFormat::kFloat32;
    return true;
  }
  if (name == "s16" || name == "16") {
    format = WavFormat::kPcm16;
    return true;
  }
  if (name == "s24" || name == "24") {
    format = WavFormat::kPcm24;
    return true;
  }
  return false;
}

const char* wavFormatName(WavFormat format) {
  switch (format) {
    case WavFormat::kFloat32: return "f32";
    case WavFormat::kPcm16: return "s16";
    case WavFormat::kPcm24: return "s24";
  }
  return "unknown";
}

bool writeWav(const std::string& path, const std::vector<float>& interleaved, int channels,
              int sample_rate, WavFormat format, std::string& error) {
  std::ofstream out(path, std::ios::binary | std::ios::trunc);
  if (!out) {
    error = "cannot open " + path + " for writing";
    return false;
  }

  const bool is_float = format == WavFormat::kFloat32;
  const int bytes_per_sample = format == WavFormat::kPcm16 ? 2 : (format == WavFormat::kPcm24 ? 3 : 4);
  const uint32_t data_bytes = static_cast<uint32_t>(interleaved.size() * bytes_per_sample);
  const uint32_t fmt_chunk_size = is_float ? 18 : 16;
  const uint32_t fact_chunk_bytes = is_float ? 12 : 0;

  out.write("RIFF", 4);
  writeU32(out, 4 + (8 + fmt_chunk_size) + fact_chunk_bytes + (8 + data_bytes));
  out.write("WAVE", 4);

  out.write("fmt ", 4);
  writeU32(out, fmt_chunk_size);
  writeU16(out, is_float ? 3 : 1);
  writeU16(out, static_cast<uint16_t>(channels));
  writeU32(out, static_cast<uint32_t>(sample_rate));
  writeU32(out, static_cast<uint32_t>(sample_rate * channels * bytes_per_sample));
  writeU16(out, static_cast<uint16_t>(channels * bytes_per_sample));
  writeU16(out, static_cast<uint16_t>(bytes_per_sample * 8));
  if (is_float) {
    writeU16(out, 0);
    out.write("fact", 4);
    writeU32(out, 4);
    writeU32(out, static_cast<uint32_t>(interleaved.size() / channels));
  }

  out.write("data", 4);
  writeU32(out, data_bytes);

  for (float sample : interleaved) {
    if (format == WavFormat::kFloat32) {
      uint32_t bits;
      std::memcpy(&bits, &sample, sizeof(bits));
      writeU32(out, bits);
    }
    else if (format == WavFormat::kPcm16) {
      long value = std::lround(clampUnit(sample) * 32767.0f);
      writeU16(out, static_cast<uint16_t>(static_cast<int16_t>(value)));
    }
    else {
      long value = std::lround(clampUnit(sample) * 8388607.0f);
      uint32_t bits = static_cast<uint32_t>(value);
      char bytes[3] = { char(bits & 0xff), char((bits >> 8) & 0xff), char((bits >> 16) & 0xff) };
      out.write(bytes, 3);
    }
  }

  if (!out) {
    error = "write failed for " + path;
    return false;
  }
  return true;
}

bool writeNpyF32(const std::string& path, const float* data, const std::vector<size_t>& shape,
                 std::string& error) {
  std::ofstream out(path, std::ios::binary | std::ios::trunc);
  if (!out) {
    error = "cannot open " + path + " for writing";
    return false;
  }

  std::ostringstream header;
  header << "{'descr': '<f4', 'fortran_order': False, 'shape': (";
  size_t count = 1;
  for (size_t i = 0; i < shape.size(); ++i) {
    header << shape[i];
    if (shape.size() == 1 || i + 1 < shape.size())
      header << ",";
    if (i + 1 < shape.size())
      header << " ";
    count *= shape[i];
  }
  header << "), }";

  std::string header_string = header.str();
  // magic(6) + version(2) + header_len(2) + header, padded with spaces to a
  // multiple of 64 and terminated by '\n'.
  size_t total = 10 + header_string.size() + 1;
  size_t padding = (64 - total % 64) % 64;
  header_string.append(padding, ' ');
  header_string.push_back('\n');

  out.write("\x93NUMPY", 6);
  out.put(1);
  out.put(0);
  writeU16(out, static_cast<uint16_t>(header_string.size()));
  out.write(header_string.data(), header_string.size());
  out.write(reinterpret_cast<const char*>(data), count * sizeof(float));

  if (!out) {
    error = "write failed for " + path;
    return false;
  }
  return true;
}

bool readNpy(const std::string& path, std::vector<float>& data, std::vector<size_t>& shape,
             std::string& error) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    error = "cannot open " + path;
    return false;
  }

  char magic[6];
  in.read(magic, 6);
  if (!in || std::memcmp(magic, "\x93NUMPY", 6) != 0) {
    error = path + " is not a .npy file";
    return false;
  }

  unsigned char version[2];
  in.read(reinterpret_cast<char*>(version), 2);
  uint32_t header_len = 0;
  if (version[0] == 1) {
    unsigned char len[2];
    in.read(reinterpret_cast<char*>(len), 2);
    header_len = len[0] | (len[1] << 8);
  }
  else {
    unsigned char len[4];
    in.read(reinterpret_cast<char*>(len), 4);
    header_len = len[0] | (len[1] << 8) | (len[2] << 16) | (uint32_t(len[3]) << 24);
  }

  std::string header(header_len, '\0');
  in.read(&header[0], header_len);
  if (!in) {
    error = "truncated header in " + path;
    return false;
  }

  if (header.find("'fortran_order': False") == std::string::npos) {
    error = path + ": only C-order arrays are supported";
    return false;
  }

  std::string descr;
  size_t descr_pos = header.find("'descr':");
  if (descr_pos != std::string::npos) {
    size_t start = header.find('\'', descr_pos + 8);
    size_t end = header.find('\'', start + 1);
    descr = header.substr(start + 1, end - start - 1);
  }

  shape.clear();
  size_t shape_pos = header.find("'shape':");
  size_t open = header.find('(', shape_pos);
  size_t close = header.find(')', open);
  std::string dims = header.substr(open + 1, close - open - 1);
  std::stringstream dim_stream(dims);
  std::string token;
  size_t count = 1;
  while (std::getline(dim_stream, token, ',')) {
    if (token.find_first_not_of(" ") == std::string::npos)
      continue;
    size_t dim = std::stoull(token);
    shape.push_back(dim);
    count *= dim;
  }

  data.resize(count);
  if (descr == "<f4") {
    in.read(reinterpret_cast<char*>(data.data()), count * sizeof(float));
  }
  else if (descr == "<f8") {
    std::vector<double> raw(count);
    in.read(reinterpret_cast<char*>(raw.data()), count * sizeof(double));
    for (size_t i = 0; i < count; ++i)
      data[i] = static_cast<float>(raw[i]);
  }
  else if (descr == "<i2") {
    std::vector<int16_t> raw(count);
    in.read(reinterpret_cast<char*>(raw.data()), count * sizeof(int16_t));
    for (size_t i = 0; i < count; ++i)
      data[i] = raw[i] / 32768.0f;
  }
  else if (descr == "<i4") {
    std::vector<int32_t> raw(count);
    in.read(reinterpret_cast<char*>(raw.data()), count * sizeof(int32_t));
    for (size_t i = 0; i < count; ++i)
      data[i] = static_cast<float>(raw[i] / 2147483648.0);
  }
  else {
    error = path + ": unsupported dtype " + descr;
    return false;
  }

  if (!in) {
    error = "truncated data in " + path;
    return false;
  }
  return true;
}

uint64_t fnv1a64(const void* data, size_t bytes, uint64_t seed) {
  const unsigned char* p = static_cast<const unsigned char*>(data);
  uint64_t hash = seed;
  for (size_t i = 0; i < bytes; ++i) {
    hash ^= p[i];
    hash *= 1099511628211ULL;
  }
  return hash;
}

std::string hex64(uint64_t value) {
  char buffer[17];
  std::snprintf(buffer, sizeof(buffer), "%016llx", static_cast<unsigned long long>(value));
  return buffer;
}

bool readTextFile(const std::string& path, std::string& contents) {
  std::ifstream in(path, std::ios::binary);
  if (!in)
    return false;
  std::ostringstream buffer;
  buffer << in.rdbuf();
  contents = buffer.str();
  return true;
}

bool writeTextFile(const std::string& path, const std::string& contents) {
  std::ofstream out(path, std::ios::binary | std::ios::trunc);
  if (!out)
    return false;
  out << contents;
  return static_cast<bool>(out);
}

bool ensureDirectory(const std::string& path) {
  if (path.empty())
    return true;
  std::string partial;
  std::stringstream stream(path);
  std::string part;
  if (path[0] == '/')
    partial = "/";
  while (std::getline(stream, part, '/')) {
    if (part.empty())
      continue;
    partial += part + "/";
    mkdir(partial.c_str(), 0755);
  }
  struct stat info;
  return stat(path.c_str(), &info) == 0 && S_ISDIR(info.st_mode);
}

Args::Args(int argc, const char* argv[], const std::vector<std::string>& flag_names) {
  for (int i = 1; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg.size() > 2 && arg.compare(0, 2, "--") == 0) {
      std::string key = arg.substr(2);
      size_t eq = key.find('=');
      if (eq != std::string::npos) {
        values_[key.substr(0, eq)] = key.substr(eq + 1);
        continue;
      }

      bool is_flag = false;
      for (const std::string& flag : flag_names)
        is_flag = is_flag || flag == key;

      if (is_flag)
        values_[key] = "1";
      else if (i + 1 < argc)
        values_[key] = argv[++i];
      else
        unknown_.push_back(arg);
    }
    else if (arg == "-o" && i + 1 < argc) {
      values_["out"] = argv[++i];
    }
    else {
      positional_.push_back(arg);
    }
  }
}

std::string Args::get(const std::string& key, const std::string& fallback) const {
  auto found = values_.find(key);
  return found == values_.end() ? fallback : found->second;
}

double Args::getDouble(const std::string& key, double fallback) const {
  auto found = values_.find(key);
  return found == values_.end() ? fallback : std::stod(found->second);
}

long long Args::getInt(const std::string& key, long long fallback) const {
  auto found = values_.find(key);
  return found == values_.end() ? fallback : std::stoll(found->second);
}

} // namespace harness
