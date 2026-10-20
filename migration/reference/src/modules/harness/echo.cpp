// Module "harness_echo": artifact-only self-test of the ref-module artifact
// path (no processor). Emits the params as JSON, a corpus file verbatim and a
// small array. Owned by the harness; used by sas-parity's self-tests.

#include "module_registry.h"

#include "harness_io.h"

REGISTER_REF_MODULE(refmod::ModuleDef{
  "harness_echo",
  "Harness self-test: artifact-only module. params.file: corpus file echoed verbatim; params.frames: ramp length",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    sink.text("params.json", params.dump(2) + "\n");

    if (params.count("file")) {
      std::string contents;
      std::string path = context.corpusPath(params["file"].get<std::string>());
      if (!harness::readTextFile(path, contents)) {
        error = "cannot read " + path;
        return false;
      }
      sink.text("corpus_file.txt", contents);
    }

    int frames = params.value("frames", 8);
    std::vector<float> ramp;
    for (int i = 0; i < frames; ++i) {
      ramp.push_back(static_cast<float>(i) / frames);
      ramp.push_back(-static_cast<float>(i) / frames);
    }
    sink.array("ramp", ramp, 2);
    return true;
  },
});
