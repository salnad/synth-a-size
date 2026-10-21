// Module "synth_voice_handler": the full upstream vital::SynthVoiceHandler,
// built and initialised as SoundEngine does, driven by a script of note and
// expression events. Captures the voice handler's note articulation and the
// state it publishes.
// Ports: port/crates/engine (tests/synth_voice_handler.rs).
//
// Per block:
//   block_state  [blocks, 18]  active voices, pressed notes,
//                midi_offset_output (4 lanes), and note_retrigger's trigger
//                mask (1 per set lane), value and offset (4 lanes each),
//                read after the block's events and before processing
//   status       [blocks, 56]  status outputs note, note_in_octave,
//                aftertouch, velocity, slide, lift, mod_wheel, pitch_wheel,
//                stereo, num_voices, macro_control_1..4 (4 lanes each)
//
// params:
//   polyphony, voice_priority, voice_override: input values
//   legato: bool, set before every block (SoundEngine::process)
//   controls: { name: value } set on getControls() before
//     updateAllModulationSwitches()
//   hold_released_voices: sets env_1_release to its maximum (~32 s), so no
//     voice is freed within the case. The port builds the voice killer (the
//     amplitude envelope) with the modulation crate; until then this keeps
//     voice allocation comparable.
//   num_samples: internal-rate frames; events as in the voice_handler module
//     (note_on, note_off, pitch_wheel, mod_wheel, channel_aftertouch, ...)
// Spec block_size is internal-rate and a multiple of oversample.

#include "module_registry.h"

#include "synth_constants.h"
#include "synth_parameters.h"
#include "synth_voice_handler.h"
#include "value.h"

#include <algorithm>
#include <memory>

namespace {
  const char* kStatusNames[] = {
    "note", "note_in_octave", "aftertouch", "velocity", "slide", "lift", "mod_wheel", "pitch_wheel",
    "stereo", "num_voices", "macro_control_1", "macro_control_2", "macro_control_3", "macro_control_4",
  };

  bool applyEvent(vital::SynthVoiceHandler* handler, const nlohmann::json& event, int offset, std::string& error) {
    std::string type = event.value("type", std::string());
    int channel = event.value("channel", 0);
    int from = event.value("from", 0);
    int to = event.value("to", 0);
    float value = event.value("value", 0.0f);

    if (type == "note_on")
      handler->noteOn(event.value("note", 0), event.value("velocity", 1.0f), offset, channel);
    else if (type == "note_off")
      handler->noteOff(event.value("note", 0), event.value("lift", 0.5f), offset, channel);
    else if (type == "sustain_on")
      handler->sustainOn(channel);
    else if (type == "sustain_off")
      handler->sustainOff(offset, channel);
    else if (type == "aftertouch")
      handler->setAftertouch(event.value("note", 0), value, offset, channel);
    else if (type == "channel_aftertouch")
      handler->setChannelAftertouch(channel, value, offset);
    else if (type == "channel_slide")
      handler->setChannelSlide(channel, value, offset);
    else if (type == "mod_wheel")
      handler->setModWheel(value, channel);
    else if (type == "pitch_wheel")
      handler->setPitchWheel(value, channel);
    else if (type == "zoned_pitch_wheel")
      handler->setZonedPitchWheel(value, from, to);
    else {
      error = "unknown event type '" + type + "'";
      return false;
    }
    return true;
  }

  bool runSynthVoiceHandler(const refmod::ArtifactContext& context, refmod::ArtifactSink& sink,
                            std::string& error) {
    const nlohmann::json& spec = context.spec;
    const nlohmann::json params = spec.value("params", nlohmann::json::object());
    const int sample_rate = spec.value("sample_rate", 44100);
    const int oversample = spec.value("oversample", 1);
    const int block_size = spec.value("block_size", 64);
    const long long num_samples = params.value("num_samples", 0LL);
    if (num_samples <= 0 || block_size % oversample) {
      error = "num_samples must be positive and block_size a multiple of oversample";
      return false;
    }

    vital::cr::Value beats_per_second(params.value("beats_per_second", 2.0f));
    // SoundEngine's destructor calls prepareDestroy() before deleting the
    // voice handler; the deleter does the same.
    auto destroy = [](vital::SynthVoiceHandler* handler) {
      handler->prepareDestroy();
      delete handler;
    };
    std::unique_ptr<vital::SynthVoiceHandler, decltype(destroy)> handler(
        new vital::SynthVoiceHandler(beats_per_second.output()), destroy);
    handler->setPolyphony(vital::kMaxPolyphony);

    vital::cr::Value polyphony(params.value("polyphony", 8.0f));
    vital::cr::Value voice_priority(params.value("voice_priority", 4.0f));
    vital::cr::Value voice_override(params.value("voice_override", 0.0f));
    handler->plug(&polyphony, vital::VoiceHandler::kPolyphony);
    handler->plug(&voice_priority, vital::VoiceHandler::kVoicePriority);
    handler->plug(&voice_override, vital::VoiceHandler::kVoiceOverride);

    handler->init();
    handler->disableUnnecessaryModSources();
    handler->setOversampleAmount(oversample);
    handler->setSampleRate(sample_rate);

    vital::control_map controls = handler->getControls();
    if (params.value("hold_released_voices", false))
      controls["env_1_release"]->set(vital::Parameters::getDetails("env_1_release").max);
    if (params.count("controls")) {
      for (auto it = params["controls"].begin(); it != params["controls"].end(); ++it) {
        if (controls.count(it.key()) == 0) {
          error = "no control '" + it.key() + "'";
          return false;
        }
        controls[it.key()]->set(it.value().get<float>());
      }
    }
    handler->updateAllModulationSwitches();
    const bool legato = params.value("legato", false);

    std::vector<nlohmann::json> events;
    if (params.count("events")) {
      for (const auto& event : params["events"])
        events.push_back(event);
    }
    std::stable_sort(events.begin(), events.end(), [](const nlohmann::json& a, const nlohmann::json& b) {
      return a.value("sample", 0LL) < b.value("sample", 0LL);
    });

    std::vector<float> block_state, status;
    size_t next_event = 0;
    for (long long block_start = 0; block_start < num_samples; block_start += block_size) {
      int n = static_cast<int>(std::min<long long>(block_size, num_samples - block_start));
      long long base_start = block_start / oversample;
      long long base_end = (block_start + n + oversample - 1) / oversample;
      handler->setLegato(legato);
      while (next_event < events.size() && events[next_event].value("sample", 0LL) < base_end) {
        long long sample = events[next_event].value("sample", 0LL);
        int offset = static_cast<int>(std::max(0LL, sample - base_start));
        if (!applyEvent(handler.get(), events[next_event], offset, error))
          return false;
        ++next_event;
      }

      const vital::Output* retrigger = handler->note_retrigger();
      float retrigger_state[12];
      for (int lane = 0; lane < vital::poly_float::kSize; ++lane) {
        retrigger_state[lane] = retrigger->trigger_mask[lane] ? 1.0f : 0.0f;
        retrigger_state[4 + lane] = retrigger->trigger_value[lane];
        retrigger_state[8 + lane] = static_cast<float>(retrigger->trigger_offset[lane]);
      }

      handler->process(n);

      block_state.push_back(static_cast<float>(handler->getNumActiveVoices()));
      block_state.push_back(static_cast<float>(handler->getNumPressedNotes()));
      for (int lane = 0; lane < vital::poly_float::kSize; ++lane)
        block_state.push_back(handler->midi_offset_output()->buffer[0][lane]);
      block_state.insert(block_state.end(), retrigger_state, retrigger_state + 12);

      for (const char* name : kStatusNames) {
        vital::poly_float value = handler->getStatusOutput(name)->value();
        for (int lane = 0; lane < vital::poly_float::kSize; ++lane)
          status.push_back(value[lane]);
      }
    }

    sink.array("block_state", block_state, 18);
    sink.array("status", status, 4 * (sizeof(kStatusNames) / sizeof(kStatusNames[0])));
    return true;
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "synth_voice_handler",
  "SynthVoiceHandler (init as SoundEngine does) driven by params.events; articulation and status capture",
  nullptr,
  {},
  {},
  runSynthVoiceHandler,
});
