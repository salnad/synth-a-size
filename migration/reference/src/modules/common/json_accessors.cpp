// Module "json_accessors": nlohmann/json 3.1.2 typed conversions as upstream
// code reads settings (`float x = j`, `int x = j`, enums, ...). Artifact-only.
// Ports: port/crates/common (json.rs Value::to_*).
//
// params.values: a JSON array (as a string). Text conversions.json is the
// dump() of one object per value with the result of get<bool>, get<double>,
// get<float>, get<int>, get<int64_t> and get<std::string>, or null where the
// conversion throws. params.enum_values: likewise, text enums.json holds
// the int value of `CaptureEnum e = j` (an unscoped enum like upstream's)
// or null. Inputs avoid undefined conversions (floats outside int range,
// negative floats into the enum's unsigned underlying type).

#include "module_registry.h"

namespace {
  enum CaptureEnum { kCaptureA, kCaptureB, kCaptureC };

  template <class T>
  nlohmann::json convert(const nlohmann::json& value) {
    try {
      T result = value;
      return nlohmann::json(result);
    }
    catch (const std::exception&) {
      return nullptr;
    }
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "json_accessors",
  "nlohmann typed conversions (bool, double, float, int, int64_t, string, enum) of params.values / "
  "params.enum_values; texts conversions.json and enums.json",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    try {
      nlohmann::json conversions = nlohmann::json::array();
      for (const auto& value : nlohmann::json::parse(params.at("values").get<std::string>())) {
        nlohmann::json result;
        result["bool"] = convert<bool>(value);
        result["double"] = convert<double>(value);
        result["float"] = convert<float>(value);
        result["int"] = convert<int>(value);
        result["int64"] = convert<int64_t>(value);
        result["string"] = convert<std::string>(value);
        conversions.push_back(result);
      }
      sink.text("conversions.json", conversions.dump());

      nlohmann::json enums = nlohmann::json::array();
      for (const auto& value : nlohmann::json::parse(params.at("enum_values").get<std::string>())) {
        try {
          CaptureEnum result = value;
          enums.push_back(static_cast<int>(result));
        }
        catch (const std::exception&) {
          enums.push_back(nullptr);
        }
      }
      sink.text("enums.json", enums.dump());
    }
    catch (const std::exception& e) {
      error = std::string("params: ") + e.what();
      return false;
    }
    return true;
  },
});
