// The offline render loop shared by ref-render and ref-bench: a HeadlessSynth
// driven like SynthPlugin::processBlock, and the Standard MIDI File reader.
// See migration/reference/README.md for the render contract.
#pragma once

#include "JuceHeader.h"
#include "sound_engine.h"
#include "synth_base.h"

#include <algorithm>
#include <cmath>
#include <string>
#include <vector>

namespace harness {

class RenderSynth : public HeadlessSynth {
  public:
    void prepare(int sample_rate, double bpm) {
      engine_->setSampleRate(sample_rate);
      engine_->setBpm(static_cast<float>(bpm));
      engine_->updateAllModulationSwitches();
      midi_manager_->setSampleRate(sample_rate);
    }

    // Mirrors SynthPlugin::processBlock with a transport that is always
    // playing from t=0 and no bypass.
    void renderBlock(AudioSampleBuffer& buffer, MidiBuffer& midi, double& seconds_time, int sample_rate) {
      int total_samples = buffer.getNumSamples();
      processModulationChanges();
      if (total_samples)
        processKeyboardEvents(midi, total_samples);

      double sample_time = 1.0 / sample_rate;
      for (int sample_offset = 0; sample_offset < total_samples;) {
        int num_samples = std::min<int>(total_samples - sample_offset, vital::kMaxBufferSize);

        engine_->correctToTime(seconds_time);
        processMidi(midi, sample_offset, sample_offset + num_samples);
        processAudio(&buffer, buffer.getNumChannels(), num_samples, sample_offset);

        seconds_time += num_samples * sample_time;
        sample_offset += num_samples;
      }
    }

    json state() { return saveToJson(); }
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
