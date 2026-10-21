// Module "line_generator_edit": LineGenerator init* shapes and editing
// methods. Artifact-only. Ports: port/crates/common (line_generator.rs).
//
// params.resolution; params.init: "linear", "triangle", "square", "sin",
// "saw_up" or "saw_down"; params.ops: edits applied in order, each
// {"op": name, ...}:
//   add_point {index, x, y}, add_middle_point {index}, remove_point {index},
//   flip_horizontal, flip_vertical, set_point {index, x, y},
//   set_power {index, power}, set_num_points {count}, set_smooth {value},
//   set_loop {value}
// Then render(). Arrays buffer (cubic-interpolation buffer), value_at_phase
// (41 phases) and state ([linear(), getNumPoints()]); text saved.json.

#include "module_registry.h"

#include "line_generator.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "line_generator_edit",
  "LineGenerator init<params.init>() then params.ops edits, render(); arrays buffer, "
  "value_at_phase, state; text saved.json",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    int resolution = params.value("resolution", static_cast<int>(LineGenerator::kDefaultResolution));
    LineGenerator generator(resolution);

    const std::string init = params.value("init", std::string("linear"));
    if (init == "linear")
      generator.initLinear();
    else if (init == "triangle")
      generator.initTriangle();
    else if (init == "square")
      generator.initSquare();
    else if (init == "sin")
      generator.initSin();
    else if (init == "saw_up")
      generator.initSawUp();
    else if (init == "saw_down")
      generator.initSawDown();
    else {
      error = "unknown params.init " + init;
      return false;
    }

    for (const auto& op : params.value("ops", nlohmann::json::array())) {
      const std::string name = op.at("op").get<std::string>();
      if (name == "add_point")
        generator.addPoint(op.at("index").get<int>(), { op.at("x").get<float>(), op.at("y").get<float>() });
      else if (name == "add_middle_point")
        generator.addMiddlePoint(op.at("index").get<int>());
      else if (name == "remove_point")
        generator.removePoint(op.at("index").get<int>());
      else if (name == "flip_horizontal")
        generator.flipHorizontal();
      else if (name == "flip_vertical")
        generator.flipVertical();
      else if (name == "set_point")
        generator.setPoint(op.at("index").get<int>(), { op.at("x").get<float>(), op.at("y").get<float>() });
      else if (name == "set_power")
        generator.setPower(op.at("index").get<int>(), op.at("power").get<float>());
      else if (name == "set_num_points")
        generator.setNumPoints(op.at("count").get<int>());
      else if (name == "set_smooth")
        generator.setSmooth(op.at("value").get<bool>());
      else if (name == "set_loop")
        generator.setLoop(op.at("value").get<bool>());
      else {
        error = "unknown op " + name;
        return false;
      }
    }
    generator.render();

    const float* buffer = generator.getCubicInterpolationBuffer();
    sink.array("buffer", std::vector<float>(buffer, buffer + resolution + LineGenerator::kExtraValues), 1);
    std::vector<float> values;
    for (int i = 0; i <= 40; ++i)
      values.push_back(generator.valueAtPhase(i / 40.0f));
    sink.array("value_at_phase", values, 1);
    sink.array("state", { generator.linear() ? 1.0f : 0.0f, static_cast<float>(generator.getNumPoints()) }, 1);
    sink.text("saved.json", generator.stateToJson().dump());
    return true;
  },
});
