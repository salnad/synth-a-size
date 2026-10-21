// Module "tuning": loads authored tuning files through upstream Tuning
// (common/tuning.cpp) and captures the note mapping.
// Ports: port/crates/engine (tests/tuning.rs).
//
// params.files: [{ "name": "file.scl" | ".kbm" | ".tun", "lines": [...] }],
// written to a temporary directory and loaded in order with
// Tuning::loadFile (each file name keeps its extension and stem, which become
// the tuning or mapping name). Optional params, applied after the files:
// reference_ratio (setReferenceRatio), reference_note_frequency
// [note, Hz] (setReferenceNoteFrequency), start_midi_note.
//
// Outputs:
//   converted   [128, 1]  convertMidiNote(0..127)
//   roundtrip   [128, 1]  the same after stateToJson -> jsonToState into a fresh Tuning
//   converted_note_percent, roundtrip_note_percent [128, 1]
//               the same in the form SynthVoiceHandler's "note" source
//               consumes them, note * (1.0f / (kMidiSize - 1))
//   state       [1, 1]    scale_start_midi_note
//   reference_midi_note [1, 1]
// params.skip_raw lists raw outputs (converted, roundtrip,
// reference_midi_note) a case does not capture, where fast-math rounding of
// a large or cancelling value exceeds the absolute gate; the normalized
// outputs and converted still carry them (see LEDGER).
//   scale       [n, 1]    the stored scale (stateToJson "scale")
//   artifact name.txt     getName()

#include "module_registry.h"

#include "tuning.h"

#include <set>

namespace {
  bool runTuning(const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());

    File directory = File::createTempFile("tuning");
    if (!directory.createDirectory()) {
      error = "cannot create a temporary directory";
      return false;
    }

    Tuning tuning;
    if (params.count("files")) {
      for (const auto& file_spec : params["files"]) {
        String text;
        for (const auto& line : file_spec["lines"])
          text += String(line.get<std::string>()) + "\n";
        File file = directory.getChildFile(String(file_spec["name"].get<std::string>()));
        if (!file.replaceWithText(text, false, false, "\n")) {
          error = "cannot write " + file.getFullPathName().toStdString();
          return false;
        }
        tuning.loadFile(file);
      }
    }
    directory.deleteRecursively();

    if (params.count("reference_ratio"))
      tuning.setReferenceRatio(params["reference_ratio"].get<float>());
    if (params.count("reference_note_frequency"))
      tuning.setReferenceNoteFrequency(params["reference_note_frequency"][0].get<int>(),
                                       params["reference_note_frequency"][1].get<float>());
    if (params.count("start_midi_note"))
      tuning.setStartMidiNote(params["start_midi_note"].get<int>());

    std::vector<float> converted, roundtrip, state, scale;
    for (int note = 0; note < vital::kMidiSize; ++note)
      converted.push_back(tuning.convertMidiNote(note));

    json data = tuning.stateToJson();
    Tuning loaded;
    loaded.jsonToState(data);
    for (int note = 0; note < vital::kMidiSize; ++note)
      roundtrip.push_back(loaded.convertMidiNote(note));

    state.push_back(data["scale_start_midi_note"].get<int>());
    for (const auto& value : data["scale"])
      scale.push_back(value.get<float>());

    std::set<std::string> skip_raw;
    if (params.count("skip_raw")) {
      for (const auto& name : params["skip_raw"])
        skip_raw.insert(name.get<std::string>());
    }
    const float note_scale = 1.0f / (vital::kMidiSize - 1);
    std::vector<float> converted_percent, roundtrip_percent;
    for (int note = 0; note < vital::kMidiSize; ++note) {
      converted_percent.push_back(converted[note] * note_scale);
      roundtrip_percent.push_back(roundtrip[note] * note_scale);
    }

    if (!skip_raw.count("converted"))
      sink.array("converted", converted, 1);
    if (!skip_raw.count("roundtrip"))
      sink.array("roundtrip", roundtrip, 1);
    if (!skip_raw.count("reference_midi_note"))
      sink.array("reference_midi_note", { data["reference_midi_note"].get<float>() }, 1);
    sink.array("converted_note_percent", converted_percent, 1);
    sink.array("roundtrip_note_percent", roundtrip_percent, 1);
    sink.array("state", state, 1);
    sink.array("scale", scale, 1);
    sink.text("name.txt", tuning.getName());
    return true;
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "tuning",
  "Tuning loaded from params.files (.scl/.kbm/.tun); convertMidiNote for every MIDI note",
  nullptr,
  {},
  {},
  runTuning,
});
