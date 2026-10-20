#include "module_registry.h"

namespace refmod {

std::vector<ModuleDef>& registry() {
  // Function-local static: safe to use from other translation units' static
  // initialisers regardless of initialisation order.
  static std::vector<ModuleDef> modules;
  return modules;
}

std::string ArtifactContext::corpusPath(const std::string& relative) const {
  return corpus_dir.empty() ? relative : corpus_dir + "/" + relative;
}

void ArtifactSink::text(const std::string& name, const std::string& content) {
  texts_.push_back({ name, content });
}

void ArtifactSink::array(const std::string& name, std::vector<float> data, size_t channels) {
  arrays_.push_back({ name, std::move(data), channels });
}

Registrar::Registrar(ModuleDef def, const char* source_file) {
  def.source_file = source_file;
  registry().push_back(std::move(def));
}

} // namespace refmod
