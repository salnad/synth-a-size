// Module "line_generator": LineGenerator load -> render -> save. Artifact-only.
// Ports: port/crates/common (line_generator.rs).
//
// params.line: the line's JSON, as a string (so the port parses the same
// bytes); params.resolution; params.loop.

#include "module_registry.h"

#include "line_generator.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "line_generator",
  "LineGenerator: jsonToState(params.line) at params.resolution, optional setLoop(true); "
  "arrays buffer (cubic-interpolation buffer, resolution + 3) and value_at_phase (41 phases); "
  "text saved.json (stateToJson().dump())",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    int resolution = params.value("resolution", static_cast<int>(LineGenerator::kDefaultResolution));
    LineGenerator generator(resolution);
    try {
      generator.jsonToState(nlohmann::json::parse(params.at("line").get<std::string>()));
    }
    catch (const std::exception& e) {
      error = std::string("line JSON: ") + e.what();
      return false;
    }
    if (params.value("loop", false))
      generator.setLoop(true);

    const float* buffer = generator.getCubicInterpolationBuffer();
    sink.array("buffer", std::vector<float>(buffer, buffer + resolution + LineGenerator::kExtraValues), 1);

    std::vector<float> values;
    for (int i = 0; i <= 40; ++i)
      values.push_back(generator.valueAtPhase(i / 40.0f));
    sink.array("value_at_phase", values, 1);

    sink.text("saved.json", generator.stateToJson().dump());
    return true;
  },
});
