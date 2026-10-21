// Modules for the Value family and utilities (value.h, smooth_value.h,
// portamento_slope.h, peak_meter.h). Ports: port/crates/dsp-core
// (tests/parity.rs mirrors these definitions by name).

#include "module_registry.h"

#include "peak_meter.h"
#include "portamento_slope.h"
#include "smooth_value.h"
#include "value.h"

using refmod::Rate;

REGISTER_REF_MODULE(refmod::ModuleDef{
  "value", "Value(value); param value; the set input's triggers change lanes",
  [](const nlohmann::json& p) -> vital::Processor* { return new vital::Value(p.value("value", 0.0f)); },
  { { "set", vital::Value::kSet, 0.0f, Rate::kTrigger } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "cr_value", "cr::Value(value); param value; the set input's triggers change lanes",
  [](const nlohmann::json& p) -> vital::Processor* { return new vital::cr::Value(p.value("value", 0.0f)); },
  { { "set", vital::Value::kSet, 0.0f, Rate::kTrigger } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "smooth_value", "SmoothValue(initial), then set(target); params initial, target",
  [](const nlohmann::json& p) -> vital::Processor* {
    vital::SmoothValue* value = new vital::SmoothValue(p.value("initial", 0.0f));
    value->set(p.value("target", 1.0f));
    return value;
  },
  { { "set", vital::Value::kSet, 0.0f, Rate::kTrigger } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "cr_smooth_value", "cr::SmoothValue(initial), then set(target); params initial, target",
  [](const nlohmann::json& p) -> vital::Processor* {
    vital::cr::SmoothValue* value = new vital::cr::SmoothValue(p.value("initial", 0.0f));
    value->set(p.value("target", 1.0f));
    return value;
  },
  { { "set", vital::Value::kSet, 0.0f, Rate::kTrigger } },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "portamento_slope", "PortamentoSlope",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::PortamentoSlope(); },
  {
    { "target", vital::PortamentoSlope::kTarget, 0.0f, Rate::kControl },
    { "source", vital::PortamentoSlope::kSource, 0.0f, Rate::kControl },
    { "force", vital::PortamentoSlope::kPortamentoForce, 0.0f, Rate::kControl },
    { "scale", vital::PortamentoSlope::kPortamentoScale, 0.0f, Rate::kControl },
    { "run_seconds", vital::PortamentoSlope::kRunSeconds, 0.0f, Rate::kControl },
    { "slope_power", vital::PortamentoSlope::kSlopePower, 0.0f, Rate::kControl },
    { "reset", vital::PortamentoSlope::kReset, 0.0f, Rate::kTrigger },
    { "num_notes_pressed", vital::PortamentoSlope::kNumNotesPressed, 1.0f, Rate::kControl },
  },
  { { "out", 0 } },
});

REGISTER_REF_MODULE(refmod::ModuleDef{
  "peak_meter", "PeakMeter (outputs hold sample 0 of each block)",
  [](const nlohmann::json&) -> vital::Processor* { return new vital::PeakMeter(); },
  { { "audio", 0, 0.0f, Rate::kAudio } },
  { { "level", vital::PeakMeter::kLevel }, { "memory_peak", vital::PeakMeter::kMemoryPeak } },
});
