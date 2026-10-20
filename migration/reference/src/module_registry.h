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

struct ModuleDef {
  // Name used in specs ("module": "...") and case files. Must be unique.
  std::string name;
  std::string description;
  // Builds the processor; params is the spec's "params" object. Return
  // nullptr for invalid params.
  std::function<vital::Processor*(const nlohmann::json& params)> create;
  std::vector<InputDef> inputs;
  std::vector<OutputDef> outputs;
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
