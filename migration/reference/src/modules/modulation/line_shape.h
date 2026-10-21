// Builds LineGenerator shapes for modulation ref-modules from params[key]:
//   "linear" | "triangle" | "square" | "sin" | "saw_up" | "saw_down", or
//   { "points": [[x, y], ...], "powers": [p, ...], "smooth": bool }.
// Mirrored in port/crates/modulation/tests/common/mod.rs.
#pragma once

#include "json/json.h"
#include "line_generator.h"

#include <string>

namespace refmod_modulation {

// Applies params[key] to line. Absent key: line is left as it is. Returns
// false for an unknown or invalid shape.
inline bool applyLineShape(LineGenerator* line, const nlohmann::json& params, const char* key) {
  if (!params.count(key))
    return true;

  const nlohmann::json& shape = params[key];
  if (shape.is_string()) {
    std::string name = shape.get<std::string>();
    if (name == "linear") line->initLinear();
    else if (name == "triangle") line->initTriangle();
    else if (name == "square") line->initSquare();
    else if (name == "sin") line->initSin();
    else if (name == "saw_up") line->initSawUp();
    else if (name == "saw_down") line->initSawDown();
    else
      return false;
    return true;
  }

  const nlohmann::json& points = shape["points"];
  int num_points = static_cast<int>(points.size());
  if (num_points < 1 || num_points > LineGenerator::kMaxPoints)
    return false;
  line->setNumPoints(num_points);
  for (int i = 0; i < num_points; ++i) {
    line->setPoint(i, { points[i][0].get<float>(), points[i][1].get<float>() });
    float power = shape.count("powers") ? shape["powers"][i].get<float>() : 0.0f;
    line->setPower(i, power);
  }
  line->setSmooth(shape.value("smooth", false));
  line->render();
  return true;
}

// A new generator with params[key] applied, "triangle" when absent. Returns
// nullptr for an unknown shape. The generator is never freed: it must outlive
// the processor, and ref-module exits after one capture.
inline LineGenerator* createLineShape(const nlohmann::json& params, const char* key = "shape") {
  LineGenerator* line = new LineGenerator();
  line->initTriangle();
  if (!applyLineShape(line, params, key)) {
    delete line;
    return nullptr;
  }
  return line;
}

} // namespace refmod_modulation
