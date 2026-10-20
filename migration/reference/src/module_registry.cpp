#include "module_registry.h"

namespace refmod {

std::vector<ModuleDef>& registry() {
  // Function-local static: safe to use from other translation units' static
  // initialisers regardless of initialisation order.
  static std::vector<ModuleDef> modules;
  return modules;
}

Registrar::Registrar(ModuleDef def, const char* source_file) {
  def.source_file = source_file;
  registry().push_back(std::move(def));
}

} // namespace refmod
