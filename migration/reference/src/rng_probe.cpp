// ref-rng-probe: hashes the output of upstream's vital::utils::RandomGenerator
// (std::mt19937 + std::uniform_real_distribution<float>) for every range the
// engine constructs it with and several seeds, 2e8 distinct draws in total. Linked
// against the engine and compiled with the same flags, so the distribution
// code is exactly what the reference runs. uniform_real_distribution /
// generate_canonical are implementation-defined, so libstdc++ (Linux) and
// libc++ (macOS) could differ; comparing this probe between machines settles
// it (migration/decisions/reference-arm.md, A2). It also counts draws that hit
// the upper bound, which generate_canonical's float rounding can produce.

#include "JuceHeader.h"
#include "harness_io.h"
#include "utils.h"

#include <cinttypes>
#include <cstdio>

namespace {

struct ProbeRange {
  const char* name;
  float min;
  float max;
};

// Every RandomGenerator range in vital/src (synth_oscillator, random_lfo,
// trigger_random, sample_source, file_source).
const ProbeRange kRanges[] = {
  { "m1_1", -1.0f, 1.0f },
  { "0_1", 0.0f, 1.0f },
  { "m0.9_0.9", -0.9f, 0.9f },
  { "mpi_pi", -vital::kPi, vital::kPi },
};

const int kSeeds[] = { 0, 1, 7, 12345, 2147483646 };
const int kDrawsPerStream = 10000000;

} // namespace

int main() {
  harness::json result;
  long long total = 0;
  int range_index = 0;
  for (const ProbeRange& range : kRanges) {
    ++range_index;
    for (int base_seed : kSeeds) {
      // Distinct streams per range, so all 2e8 draws are different.
      int seed = base_seed - (range_index - 1) * 1000003;
      vital::utils::RandomGenerator::next_seed_ = seed;
      vital::utils::RandomGenerator generator(range.min, range.max);
      // Same seed, raw engine output: both libstdc++ and libc++ use one 32-bit
      // mt19937 output per float draw, so this replays the stream feeding
      // generate_canonical and counts outputs whose x / 2^32 rounds to 1.0f.
      std::mt19937 raw(seed);
      uint64_t hash = 1469598103934665603ULL;
      long long at_max = 0;
      long long at_min = 0;
      long long round_to_one = 0;
      for (int i = 0; i < kDrawsPerStream; ++i) {
        round_to_one += raw() >= 0xFFFFFF80u;
        float value = generator.next();
        hash = harness::fnv1a64(&value, sizeof(value), hash);
        at_max += value >= range.max;
        at_min += value == range.min;
      }
      total += kDrawsPerStream;
      std::string key = std::string("rng_") + range.name + "_seed" + std::to_string(seed);
      result["hashes"][key] = harness::hex64(hash);
      result["upper_bound_hits"][key] = at_max;
      result["lower_bound_hits"][key] = at_min;
      result["canonical_round_to_one_inputs"][key] = round_to_one;
    }
  }

  // polyVoiceNext / polyNext as used by the oscillator and random LFO.
  vital::utils::RandomGenerator::next_seed_ = 3;
  vital::utils::RandomGenerator poly_generator(-1.0f, 1.0f);
  uint64_t poly_hash = 1469598103934665603ULL;
  for (int i = 0; i < kDrawsPerStream / 4; ++i) {
    vital::poly_float value = poly_generator.polyVoiceNext();
    for (int lane = 0; lane < vital::poly_float::kSize; ++lane) {
      float lane_value = value[lane];
      poly_hash = harness::fnv1a64(&lane_value, sizeof(lane_value), poly_hash);
    }
  }
  total += kDrawsPerStream / 4 * 2;
  result["hashes"]["poly_voice_next_m1_1_seed3"] = harness::hex64(poly_hash);
  result["total_draws"] = total;
  result["build"] = harness::buildInfo();

  std::printf("%s\n", result.dump(2).c_str());
  return 0;
}
