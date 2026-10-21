// Module "voice_handler": drives an upstream vital::VoiceHandler from a
// script of note, pedal and expression events and records what every voice
// copy sees, block by block.
// Ports: port/crates/engine (tests/voice_handler.rs builds the same graph).
//
// The voice router holds two processors, as a SynthVoiceHandler voice would:
//
// - VoiceProbe (stateful, one clone per voice pair) reads the 18 per-voice
//   control-rate outputs of the handler. Each time a clone processes, it
//   appends every output's trigger value, trigger mask (1 per set lane) and
//   trigger offset, 4 lanes each, to the voice_values / voice_masks /
//   voice_offsets arrays (one row per aggregate-voice process call). Its
//   audio output is a per-lane envelope: 1 from a voice_on trigger, falling
//   by 1/release_samples per sample after voice_off and 1/kill_samples after
//   voice_kill. It is the voice killer and an accumulated output ("audio").
// - cr::Multiply(note, 1/127) is a non-accumulated control-rate output
//   ("note_percent"), like SynthVoiceHandler's note_percentage_.
//
// block_state has one row per block: active voices, pressed notes,
// getLastActiveNote(), getCurrentVoiceMask() (4 lanes, 1 per set lane).
//
// params:
//   polyphony, voice_priority, voice_override: initial input values
//   legato: bool
//   release_samples, kill_samples: probe envelope lengths
//   tuning_scala: optional Scala file lines (Tuning::loadScalaFile)
//   events: [{ "sample": n, "type": ..., ... }], sample in base-rate frames
//     note_on {note, velocity, channel}, note_off {note, lift, channel},
//     sustain_on/off {channel}, sostenuto_on/off {channel},
//     sustain_on_range/off_range, sostenuto_on_range/off_range {from, to},
//     all_notes_off {channel?}, all_notes_off_range {from, to},
//     all_sounds_off, aftertouch {note, value, channel},
//     channel_aftertouch {channel, value}, channel_range_aftertouch {from, to, value},
//     channel_slide {channel, value}, channel_range_slide {from, to, value},
//     mod_wheel {channel, value}, mod_wheel_all {value},
//     pitch_wheel {channel, value}, zoned_pitch_wheel {from, to, value},
//     polyphony / voice_priority / voice_override {value}, legato {value}
//   num_samples: length in internal-rate frames (sample_rate * oversample).
//     It is a param, not the spec key, because the captured arrays are not
//     one row per frame.
// Spec block_size is in internal-rate frames and must be a multiple of
// oversample.

#include "module_registry.h"

#include "operators.h"
#include "synth_constants.h"
#include "tuning.h"
#include "value.h"
#include "voice_handler.h"

#include <algorithm>
#include <memory>

namespace {
  constexpr int kNumProbed = 18;

  struct ProbeLog {
    std::vector<float> values;
    std::vector<float> masks;
    std::vector<float> offsets;
  };

  ProbeLog& probeLog() {
    static ProbeLog log;
    return log;
  }

  class VoiceProbe : public vital::Processor {
    public:
      VoiceProbe(float release_samples, float kill_samples) :
          vital::Processor(kNumProbed, 1), amp_(), decay_(),
          release_step_(1.0f / release_samples), kill_step_(1.0f / kill_samples) { }

      vital::Processor* clone() const override { return new VoiceProbe(*this); }

      void process(int num_samples) override {
        ProbeLog& log = probeLog();
        for (int i = 0; i < kNumProbed; ++i) {
          const vital::Output* source = input(i)->source;
          for (int lane = 0; lane < vital::poly_float::kSize; ++lane) {
            log.values.push_back(source->trigger_value[lane]);
            log.masks.push_back(source->trigger_mask[lane] ? 1.0f : 0.0f);
            log.offsets.push_back(static_cast<float>(source->trigger_offset[lane]));
          }
        }

        const vital::Output* event = input(0)->source;
        vital::poly_float* dest = output()->buffer;
        for (int s = 0; s < num_samples; ++s) {
          vital::poly_float value = 0.0f;
          for (int lane = 0; lane < vital::poly_float::kSize; ++lane) {
            if (event->trigger_mask[lane] && static_cast<int>(event->trigger_offset[lane]) == s) {
              float trigger = event->trigger_value[lane];
              if (trigger == vital::kVoiceOn) {
                amp_[lane] = 1.0f;
                decay_[lane] = 0.0f;
              }
              else if (trigger == vital::kVoiceOff)
                decay_[lane] = release_step_;
              else if (trigger == vital::kVoiceKill)
                decay_[lane] = kill_step_;
            }
            amp_[lane] = std::max(0.0f, amp_[lane] - decay_[lane]);
            value.set(lane, amp_[lane]);
          }
          dest[s] = value;
        }
      }

    private:
      float amp_[vital::poly_float::kSize];
      float decay_[vital::poly_float::kSize];
      float release_step_;
      float kill_step_;
  };

  float num(const nlohmann::json& event, const char* key, float fallback = 0.0f) {
    return event.value(key, fallback);
  }

  int inum(const nlohmann::json& event, const char* key, int fallback = 0) {
    return event.value(key, fallback);
  }

  bool applyEvent(vital::VoiceHandler* handler, vital::Value* inputs[3], const nlohmann::json& event,
                  int offset, std::string& error) {
    std::string type = event.value("type", std::string());
    int channel = inum(event, "channel");
    int from = inum(event, "from");
    int to = inum(event, "to");
    float value = num(event, "value");

    if (type == "note_on")
      handler->noteOn(inum(event, "note"), num(event, "velocity", 1.0f), offset, channel);
    else if (type == "note_off")
      handler->noteOff(inum(event, "note"), num(event, "lift", 0.5f), offset, channel);
    else if (type == "sustain_on")
      handler->sustainOn(channel);
    else if (type == "sustain_off")
      handler->sustainOff(offset, channel);
    else if (type == "sostenuto_on")
      handler->sostenutoOn(channel);
    else if (type == "sostenuto_off")
      handler->sostenutoOff(offset, channel);
    else if (type == "sustain_on_range")
      handler->sustainOnRange(from, to);
    else if (type == "sustain_off_range")
      handler->sustainOffRange(offset, from, to);
    else if (type == "sostenuto_on_range")
      handler->sostenutoOnRange(from, to);
    else if (type == "sostenuto_off_range")
      handler->sostenutoOffRange(offset, from, to);
    else if (type == "all_notes_off") {
      if (event.count("channel"))
        handler->allNotesOff(offset, channel);
      else
        handler->allNotesOff(offset);
    }
    else if (type == "all_notes_off_range")
      handler->allNotesOffRange(offset, from, to);
    else if (type == "all_sounds_off")
      handler->allSoundsOff();
    else if (type == "aftertouch")
      handler->setAftertouch(inum(event, "note"), value, offset, channel);
    else if (type == "channel_aftertouch")
      handler->setChannelAftertouch(channel, value, offset);
    else if (type == "channel_range_aftertouch")
      handler->setChannelRangeAftertouch(from, to, value, offset);
    else if (type == "channel_slide")
      handler->setChannelSlide(channel, value, offset);
    else if (type == "channel_range_slide")
      handler->setChannelRangeSlide(from, to, value, offset);
    else if (type == "mod_wheel")
      handler->setModWheel(value, channel);
    else if (type == "mod_wheel_all")
      handler->setModWheelAllChannels(value);
    else if (type == "pitch_wheel")
      handler->setPitchWheel(value, channel);
    else if (type == "zoned_pitch_wheel")
      handler->setZonedPitchWheel(value, from, to);
    else if (type == "polyphony")
      inputs[vital::VoiceHandler::kPolyphony]->set(value);
    else if (type == "voice_priority")
      inputs[vital::VoiceHandler::kVoicePriority]->set(value);
    else if (type == "voice_override")
      inputs[vital::VoiceHandler::kVoiceOverride]->set(value);
    else if (type == "legato")
      handler->setLegato(value != 0.0f);
    else {
      error = "unknown event type '" + type + "'";
      return false;
    }
    return true;
  }

  float maskLane(vital::poly_mask mask, int lane) {
    return mask[lane] ? 1.0f : 0.0f;
  }

  bool runVoiceHandler(const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
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

    std::unique_ptr<vital::VoiceHandler> handler =
        std::make_unique<vital::VoiceHandler>(0, vital::kMaxPolyphony);

    std::unique_ptr<vital::Value> inputs[3] = {
      std::make_unique<vital::cr::Value>(params.value("polyphony", 8.0f)),
      std::make_unique<vital::cr::Value>(params.value("voice_priority", 4.0f)),
      std::make_unique<vital::cr::Value>(params.value("voice_override", 0.0f)),
    };
    vital::Value* input_values[3] = { inputs[0].get(), inputs[1].get(), inputs[2].get() };
    for (int i = 0; i < 3; ++i)
      handler->plug(inputs[i].get(), i);

    VoiceProbe* probe = new VoiceProbe(params.value("release_samples", 300.0f),
                                       params.value("kill_samples", 40.0f));
    vital::Output* probed[kNumProbed] = {
      handler->voice_event(), handler->retrigger(), handler->reset(), handler->note(),
      handler->last_note(), handler->note_pressed(), handler->note_count(), handler->note_in_octave(),
      handler->channel(), handler->velocity(), handler->lift(), handler->aftertouch(),
      handler->slide(), handler->active_mask(), handler->mod_wheel(), handler->pitch_wheel(),
      handler->pitch_wheel_percent(), handler->local_pitch_bend(),
    };
    for (int i = 0; i < kNumProbed; ++i)
      probe->plug(probed[i], i);
    handler->addProcessor(probe);

    static const vital::cr::Value max_midi_invert(1.0f / (vital::kMidiSize - 1));
    vital::cr::Multiply* note_percent = new vital::cr::Multiply();
    note_percent->plug(&max_midi_invert, 0);
    note_percent->plug(handler->note(), 1);
    handler->addProcessor(note_percent);

    vital::Output* audio = handler->registerOutput(probe->output());
    vital::Output* note_out = handler->registerControlRateOutput(note_percent->output(), true);
    handler->setVoiceKiller(probe->output());
    handler->setLegato(params.value("legato", false));

    Tuning tuning;
    if (params.count("tuning_scala")) {
      StringArray lines;
      for (const auto& line : params["tuning_scala"])
        lines.add(line.get<std::string>());
      tuning.loadScalaFile(lines);
      handler->setTuning(&tuning);
    }

    handler->init();
    handler->setSampleRate(sample_rate);
    handler->setOversampleAmount(oversample);

    ProbeLog& log = probeLog();
    log = ProbeLog();

    std::vector<nlohmann::json> events;
    if (params.count("events")) {
      for (const auto& event : params["events"])
        events.push_back(event);
    }
    std::stable_sort(events.begin(), events.end(), [](const nlohmann::json& a, const nlohmann::json& b) {
      return a.value("sample", 0LL) < b.value("sample", 0LL);
    });

    std::vector<float> block_state, audio_capture, note_capture;
    size_t next_event = 0;
    for (long long block_start = 0; block_start < num_samples; block_start += block_size) {
      int n = static_cast<int>(std::min<long long>(block_size, num_samples - block_start));
      long long base_start = block_start / oversample;
      long long base_end = (block_start + n + oversample - 1) / oversample;
      while (next_event < events.size() && events[next_event].value("sample", 0LL) < base_end) {
        long long sample = events[next_event].value("sample", 0LL);
        int offset = static_cast<int>(std::max(0LL, sample - base_start));
        if (!applyEvent(handler.get(), input_values, events[next_event], offset, error))
          return false;
        ++next_event;
      }

      handler->process(n);

      vital::poly_mask voice_mask = handler->getCurrentVoiceMask();
      block_state.push_back(static_cast<float>(handler->getNumActiveVoices()));
      block_state.push_back(static_cast<float>(handler->getNumPressedNotes()));
      block_state.push_back(handler->getLastActiveNote());
      for (int lane = 0; lane < vital::poly_float::kSize; ++lane)
        block_state.push_back(maskLane(voice_mask, lane));

      for (int s = 0; s < n; ++s) {
        for (int lane = 0; lane < vital::poly_float::kSize; ++lane) {
          audio_capture.push_back(audio->buffer[s][lane]);
          note_capture.push_back(note_out->buffer[0][lane]);
        }
      }
    }

    const size_t columns = kNumProbed * vital::poly_float::kSize;
    sink.array("voice_values", log.values, columns);
    sink.array("voice_masks", log.masks, columns);
    sink.array("voice_offsets", log.offsets, columns);
    sink.array("block_state", block_state, 7);
    sink.array("audio", audio_capture, 4);
    sink.array("note_percent", note_capture, 4);
    return true;
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "voice_handler",
  "VoiceHandler driven by a params.events script (note, pedal, MPE expression); per-voice trigger capture",
  nullptr,
  {},
  {},
  runVoiceHandler,
});
