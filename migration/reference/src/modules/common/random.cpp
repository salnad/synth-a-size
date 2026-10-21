// Module "random_generator": vital::utils::RandomGenerator (std::mt19937 through
// the C++ library's uniform_real_distribution<float>). Artifact-only.
// Ports: port/crates/common (random.rs).
//
// params.seed (int), params.min/params.max (floats, or params.range = "pi"
// for [-kPi, kPi]), params.count. Array at_max counts values equal to max:
// libc++ returns max for draws u >= 2^32 - 128 and libstdc++ never does (A9).
// That differs by (max - min) * 2^-24 in values, inside any tolerance, so
// at_max is what tells the two columns apart.

#include "module_registry.h"

#include "common.h"
#include "utils.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "random_generator",
  "RandomGenerator(min, max) reseeded with params.seed; arrays values (params.count draws of next()) "
  "and at_max (how many equal max)",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    float min = params.value("min", -1.0f);
    float max = params.value("max", 1.0f);
    if (params.value("range", std::string()) == "pi") {
      min = -vital::kPi;
      max = vital::kPi;
    }
    vital::utils::RandomGenerator generator(min, max);
    generator.seed(params.at("seed").get<int>());
    std::vector<float> values;
    int count = params.value("count", 4096);
    for (int i = 0; i < count; ++i)
      values.push_back(generator.next());
    sink.array("values", values, 1);
    float at_max = 0.0f;
    for (float value : values)
      at_max += value == max ? 1.0f : 0.0f;
    sink.array("at_max", { at_max }, 1);
    return true;
  },
});
