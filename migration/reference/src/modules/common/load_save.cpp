// Module "load_save_codecs": JUCE Base64, the LoadSave PCM re-encoders and
// LoadSave::compareVersionStrings. Artifact-only.
// Ports: port/crates/common (base64.rs, load_save.rs, scalar.rs PCM helpers).
//
// params.kind selects one of:
//   "base64_encode": params.bytes (0..255) -> text base64.txt
//                    (Base64::toBase64, upstream's own serialization)
//   "base64_decode": params.text -> array result = [returned ok flag,
//                    decoded size, decoded bytes...] (early stop keeps the
//                    bytes decoded so far)
//   "pcm":           params.values -> {"samples": base64 floats} through
//                    convertBufferToPcm (text pcm.json, array pcm =
//                    int16 / 32768, an exact scaling) then
//                    convertPcmToFloatBuffer (text float.json, array floats)
//   "versions":      params.pairs [[a, b], ...] -> array results (-1, 0, 1)

#include "module_registry.h"

#include "JuceHeader.h"
#include "load_save.h"

#include <cstring>

namespace {
  std::vector<uint8_t> decodeBase64(const std::string& text, bool& ok) {
    MemoryOutputStream decoded;
    ok = Base64::convertFromBase64(decoded, text);
    const uint8_t* data = static_cast<const uint8_t*>(decoded.getData());
    return std::vector<uint8_t>(data, data + decoded.getDataSize());
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "load_save_codecs",
  "JUCE Base64 encode/decode, LoadSave::convertBufferToPcm/convertPcmToFloatBuffer and "
  "LoadSave::compareVersionStrings, selected by params.kind",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    const std::string kind = params.value("kind", std::string());

    if (kind == "base64_encode") {
      std::vector<uint8_t> bytes;
      for (const auto& byte : params.at("bytes"))
        bytes.push_back(static_cast<uint8_t>(byte.get<int>()));
      sink.text("base64.txt", Base64::toBase64(bytes.data(), bytes.size()).toStdString());
      return true;
    }
    if (kind == "base64_decode") {
      bool ok = false;
      std::vector<uint8_t> bytes = decodeBase64(params.at("text").get<std::string>(), ok);
      std::vector<float> result = { ok ? 1.0f : 0.0f, static_cast<float>(bytes.size()) };
      for (uint8_t byte : bytes)
        result.push_back(byte);
      sink.array("result", result, 1);
      return true;
    }
    if (kind == "pcm") {
      std::vector<float> values;
      for (const auto& value : params.at("values"))
        values.push_back(value.get<float>());
      nlohmann::json data;
      data["samples"] = Base64::toBase64(values.data(), sizeof(float) * values.size()).toStdString();

      LoadSave::convertBufferToPcm(data, "samples");
      sink.text("pcm.json", data.dump());
      bool ok = false;
      std::vector<uint8_t> bytes = decodeBase64(data["samples"].get<std::string>(), ok);
      std::vector<float> pcm(bytes.size() / sizeof(int16_t));
      for (size_t i = 0; i < pcm.size(); ++i) {
        int16_t sample;
        memcpy(&sample, bytes.data() + i * sizeof(int16_t), sizeof(sample));
        pcm[i] = sample / 32768.0f;
      }
      sink.array("pcm", pcm, 1);

      LoadSave::convertPcmToFloatBuffer(data, "samples");
      sink.text("float.json", data.dump());
      bytes = decodeBase64(data["samples"].get<std::string>(), ok);
      std::vector<float> floats(bytes.size() / sizeof(float));
      memcpy(floats.data(), bytes.data(), floats.size() * sizeof(float));
      sink.array("floats", floats, 1);
      return true;
    }
    if (kind == "versions") {
      std::vector<float> results;
      for (const auto& pair : params.at("pairs")) {
        results.push_back(static_cast<float>(LoadSave::compareVersionStrings(
            String(pair.at(0).get<std::string>()), String(pair.at(1).get<std::string>()))));
      }
      sink.array("results", results, 1);
      return true;
    }
    error = "unknown params.kind " + kind;
    return false;
  },
});
