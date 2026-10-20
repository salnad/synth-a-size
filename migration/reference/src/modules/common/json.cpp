// Module "json_serializer": nlohmann/json 3.1.2 (vital/third_party/json) parse -> dump,
// the serializer behind every saved preset and wavetable. Artifact-only.
// Ports: port/crates/common (json.rs).
//
// params.text: JSON text to parse; texts dump.json (dump()) and pretty.json
// (dump(2)). params.float_count: also emit floats.json, the dump() of an
// array of that many doubles (and doubles of floats) from a fixed 64-bit LCG,
// keeping only normal finite values (integer exponent test, so fast-math
// cannot fold it). The generator is repeated in port/crates/common/tests.

#include "module_registry.h"

#include <cstring>

REGISTER_REF_MODULE(refmod::ModuleDef{
  "json_serializer",
  "nlohmann parse(params.text) -> dump() / dump(2); params.float_count: dump() of generated doubles",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    if (params.count("text")) {
      try {
        nlohmann::json data = nlohmann::json::parse(params["text"].get<std::string>());
        sink.text("dump.json", data.dump());
        sink.text("pretty.json", data.dump(2));
      }
      catch (const std::exception& e) {
        error = std::string("parse: ") + e.what();
        return false;
      }
    }

    if (params.count("float_count")) {
      int count = params["float_count"];
      unsigned long long state = 0x2545F4914F6CDD1DULL;
      nlohmann::json values = nlohmann::json::array();
      for (int i = 0; i < count; ++i) {
        state = state * 6364136223846793005ULL + 1442695040888963407ULL;
        if (i % 2 == 0) {
          unsigned long long exponent = (state >> 52) & 0x7ffULL;
          if (exponent == 0 || exponent == 0x7ffULL)
            continue;
          double value;
          memcpy(&value, &state, sizeof(value));
          values.push_back(value);
        }
        else {
          unsigned bits = static_cast<unsigned>(state >> 32);
          unsigned exponent = (bits >> 23) & 0xffu;
          if (exponent == 0 || exponent == 0xffu)
            continue;
          float value;
          memcpy(&value, &bits, sizeof(value));
          values.push_back(value);
        }
      }
      sink.text("floats.json", values.dump());
    }
    return true;
  },
});
