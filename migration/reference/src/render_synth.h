// The offline render loop shared by ref-render and ref-bench: a HeadlessSynth
// driven like SynthPlugin::processBlock, and the Standard MIDI File reader.
// See migration/reference/README.md for the render contract.
#pragma once

#include "JuceHeader.h"
#include "line_generator.h"
#include "modulation_connection_processor.h"
#include "sample_source.h"
#include "sound_engine.h"
#include "synth_base.h"
#include "synth_parameters.h"
#include "wavetable_creator.h"

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

namespace harness {

// SynthPlugin::prepareToPlay and processBlock on any SynthBase, so ref-render,
// ref-bench and ref-gui drive the engine the same way.
template <class Base>
class RenderLoop : public Base {
  public:
    void prepare(int sample_rate, double bpm) {
      this->engine_->setSampleRate(sample_rate);
      this->engine_->setBpm(static_cast<float>(bpm));
      this->engine_->updateAllModulationSwitches();
      this->midi_manager_->setSampleRate(sample_rate);
    }

    // Mirrors SynthPlugin::processBlock with a transport that is always
    // playing from t=0 and no bypass.
    void renderBlock(AudioSampleBuffer& buffer, MidiBuffer& midi, double& seconds_time, int sample_rate) {
      int total_samples = buffer.getNumSamples();
      this->processModulationChanges();
      if (total_samples)
        this->processKeyboardEvents(midi, total_samples);

      double sample_time = 1.0 / sample_rate;
      for (int sample_offset = 0; sample_offset < total_samples;) {
        int num_samples = std::min<int>(total_samples - sample_offset, vital::kMaxBufferSize);

        this->engine_->correctToTime(seconds_time);
        this->processMidi(midi, sample_offset, sample_offset + num_samples);
        this->processAudio(&buffer, buffer.getNumChannels(), num_samples, sample_offset);

        seconds_time += num_samples * sample_time;
        sample_offset += num_samples;
      }
    }
};

class RenderSynth : public RenderLoop<HeadlessSynth> {
  public:

    json state() { return saveToJson(); }

    // SynthGuiInterface::connectModulation (common/synth_gui_interface.cpp):
    // the connection, then initModulationValues if it took a new slot.
    void guiConnectModulation(const std::string& source, const std::string& destination) {
      bool created = connectModulation(source, destination);
      if (!created)
        return;

      int connection_index = getConnectionIndex(source, destination);
      if (connection_index < 0)
        return;

      vital::ModulationConnection* connection = getModulationBank().atIndex(connection_index);
      connection->modulation_processor->lineMapGenerator()->initLinear();
      valueChanged("modulation_" + std::to_string(connection_index + 1) + "_power", 0.0f);
    }

    // ModulationManager::modulationDraggedToComponent
    // (interface/editor_sections/modulation_manager.cpp): a source dropped on a
    // destination's control, without the GUI. If the two aren't connected yet:
    // the amount from the control's position (its slider shows the control's
    // value), SynthGuiInterface::connectModulation, then
    // SynthGuiInterface::setModulationValues(amount, bipolar, false, false).
    // `bipolar` is whether a modifier key is held.
    void dragModulation(const std::string& source, const std::string& destination, bool bipolar) {
      static constexpr float kDefaultModulationRatio = 0.25f;
      if (getConnection(source, destination))
        return;

      vital::ValueDetails details = vital::Parameters::getDetails(destination);
      // juce::Slider::valueToProportionOfLength on the control's linear range.
      double value = controls_[destination]->value();
      float percent = std::min(1.0, std::max(0.0, (value - details.min) / (details.max - details.min)));
      float modulation_amount = 1.0f - percent;
      if (bipolar)
        modulation_amount = std::min(modulation_amount, percent) * 2.0f;
      modulation_amount = std::max(modulation_amount, kDefaultModulationRatio);

      guiConnectModulation(source, destination);
      int connection_index = getConnectionIndex(source, destination);
      if (connection_index < 0)
        return;
      std::string number = std::to_string(connection_index + 1);
      valueChanged("modulation_" + number + "_amount", modulation_amount);
      valueChanged("modulation_" + number + "_bipolar", bipolar ? 1.0f : 0.0f);
      valueChanged("modulation_" + number + "_stereo", 0.0f);
      valueChanged("modulation_" + number + "_bypass", 0.0f);
    }

    // SynthPlugin::parameterChanged -> SynthBase::valueChangedExternal, without
    // the GUI callback it posts.
    bool setControl(const std::string& name, vital::mono_float value) {
      if (controls_.count(name) == 0)
        return false;
      valueChanged(name, value);
      if (name == "mod_wheel")
        engine_->setModWheelAllChannels(value);
      else if (name == "pitch_wheel")
        engine_->setZonedPitchWheel(value, 0, vital::kNumMidiChannels - 1);
      return true;
    }

    // One live edit of an edit script (migration/reference/README.md), made
    // as upstream's editor makes it. Returns an error, or "" on success.
    std::string applyEdit(const json& edit) {
      const std::string type = edit.at("type").get<std::string>();
      if (type == "set_control") {
        if (!setControl(edit.at("name").get<std::string>(), edit.at("value").get<float>()))
          return "unknown control " + edit.at("name").get<std::string>();
      }
      else if (type == "connect_modulation") {
        std::string destination = edit.at("destination").get<std::string>();
        if (controls_.count(destination) == 0)
          return "connect_modulation: no control " + destination;
        dragModulation(edit.at("source").get<std::string>(), destination, edit.value("bipolar", false));
      }
      else if (type == "disconnect_modulation")
        disconnectModulation(edit.at("source").get<std::string>(), edit.at("destination").get<std::string>());
      else if (type == "line_mapping") {
        // The remap curve editor edits the slot's line map and renders it.
        int slot = edit.at("slot").get<int>();
        if (slot < 0 || slot >= vital::kMaxModulationConnections)
          return "line_mapping slot out of range";
        LineGenerator* line = getModulationBank().atIndex(slot)->modulation_processor->lineMapGenerator();
        line->jsonToState(edit.at("line"));
        line->render();
      }
      else if (type == "lfo_shape") {
        // LoadSave::loadLfos for one LFO, as the LFO editor loads a shape.
        int lfo = edit.at("lfo").get<int>();
        if (lfo < 0 || lfo >= vital::kNumLfos)
          return "lfo_shape lfo out of range";
        getLfoSource(lfo)->jsonToState(edit.at("line"));
        getLfoSource(lfo)->render();
      }
      else if (type == "wavetable") {
        // LoadSave::loadWavetables for one oscillator.
        int oscillator = edit.at("oscillator").get<int>();
        if (oscillator < 0 || oscillator >= vital::kNumOscillators)
          return "wavetable oscillator out of range";
        WavetableCreator* creator = getWavetableCreator(oscillator);
        creator->jsonToState(edit.at("wavetable"));
        creator->render();
      }
      else if (type == "sample")
        getSample()->jsonToState(edit.at("data")); // LoadSave::loadSample
      else
        return "unknown edit type " + type;
      return "";
    }
};

struct TimedMessage {
  long long sample;
  MidiMessage message;
};

inline bool loadMidi(const std::string& path, int sample_rate, std::vector<TimedMessage>& events,
                     double& last_event_seconds, std::string& error) {
  File file = File::getCurrentWorkingDirectory().getChildFile(path);
  FileInputStream stream(file);
  if (!stream.openedOk()) {
    error = "cannot open MIDI file " + path;
    return false;
  }

  MidiFile midi_file;
  if (!midi_file.readFrom(stream)) {
    error = "cannot parse MIDI file " + path;
    return false;
  }

  midi_file.convertTimestampTicksToSeconds();
  MidiMessageSequence merged;
  for (int track = 0; track < midi_file.getNumTracks(); ++track)
    merged.addSequence(*midi_file.getTrack(track), 0.0);
  merged.sort();

  last_event_seconds = 0.0;
  for (int i = 0; i < merged.getNumEvents(); ++i) {
    const MidiMessage& message = merged.getEventPointer(i)->message;
    if (message.isMetaEvent() || message.isSysEx())
      continue;

    double seconds = message.getTimeStamp();
    long long sample = static_cast<long long>(std::floor(seconds * sample_rate + 0.5));
    events.push_back({ sample, message });
    last_event_seconds = std::max(last_event_seconds, seconds);
  }
  return true;
}

} // namespace harness
