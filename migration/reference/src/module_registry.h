// Registry of processors that ref-module can drive.
//
// Each module lives in its own file under src/modules/<area>/<module>.cpp and
// registers itself with REGISTER_REF_MODULE. CMake globs src/modules/**, so
// adding a module never touches a shared file. See
// migration/reference/README.md ("Adding a module").
#pragma once

#include "json/json.h"
#include "processor.h"

#include <functional>
#include <string>
#include <vector>

namespace refmod {

enum class Rate { kAudio, kControl, kTrigger };

struct InputDef {
  std::string name;
  int index;
  float default_value;
  Rate rate;
};

struct OutputDef {
  std::string name;
  int index;
};

// Context handed to a module's artifact hook.
struct ArtifactContext {
  // The resolved spec (module, params, ...).
  const nlohmann::json& spec;
  // Absolute path of migration/corpus (from --corpus-dir), for inputs such as
  // wavetable or preset JSON files referenced by params.
  std::string corpus_dir;

  // corpus_dir + "/" + relative.
  std::string corpusPath(const std::string& relative) const;
};

// Collects what an artifact hook produces.
class ArtifactSink {
  public:
    // An exact-match text artifact, written to artifact_<name> (name may carry
    // an extension, e.g. "saved.json"). Compared byte for byte by sas-parity.
    void text(const std::string& name, const std::string& content);

    // A float array written to out_<name>.npy with shape [frames, channels];
    // compared with the case tolerances like processor outputs.
    void array(const std::string& name, std::vector<float> data, size_t channels);

    struct Text { std::string name; std::string content; };
    struct Array { std::string name; std::vector<float> data; size_t channels; };
    const std::vector<Text>& texts() const { return texts_; }
    const std::vector<Array>& arrays() const { return arrays_; }

  private:
    std::vector<Text> texts_;
    std::vector<Array> arrays_;
};

// Return false and set error to fail the capture.
using ArtifactFn = std::function<bool(const ArtifactContext& context, ArtifactSink& sink, std::string& error)>;

struct ModuleDef {
  // Name used in specs ("module": "...") and case files. Must be unique.
  std::string name;
  std::string description;
  // Builds the processor; params is the spec's "params" object. Return
  // nullptr for invalid params. Leave empty for artifact-only modules.
  std::function<vital::Processor*(const nlohmann::json& params)> create;
  std::vector<InputDef> inputs;
  std::vector<OutputDef> outputs;
  // Optional: emits exact-match text artifacts (e.g. JSON from a load -> save
  // round trip) and extra arrays. Runs after the processor, if any.
  ArtifactFn artifacts;
  // Filled in by REGISTER_REF_MODULE.
  std::string source_file;
};

std::vector<ModuleDef>& registry();

struct Registrar {
  Registrar(ModuleDef def, const char* source_file);
};

} // namespace refmod

#define REF_MODULE_CONCAT_INNER(a, b) a##b
#define REF_MODULE_CONCAT(a, b) REF_MODULE_CONCAT_INNER(a, b)

// Usage (at namespace scope in a src/modules/<area>/<module>.cpp file):
//   REGISTER_REF_MODULE(refmod::ModuleDef{ "name", "description", factory, inputs, outputs });
#define REGISTER_REF_MODULE(...) \
  static ::refmod::Registrar REF_MODULE_CONCAT(ref_module_registrar_, __LINE__)(__VA_ARGS__, __FILE__)
