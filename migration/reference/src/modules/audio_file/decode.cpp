// Module "audio_file_decode": artifact-only. Reads a corpus audio file the way
// WavetableEditSection::loadAudioFile does (JUCE's AudioFormatManager with
// registerBasicFormats(), the whole file read as float) and emits the first
// channel, which loadAudioAsWavetable passes on, and the sample rate. The
// port's readers (port/crates/plugin/src/audio_file.rs) are compared with it
// sample for sample.

#include "module_registry.h"

#include "JuceHeader.h"

#include <memory>

REGISTER_REF_MODULE(refmod::ModuleDef{
  "audio_file_decode",
  "Audio file decode: params.file is a corpus audio file; emits its first channel and sample rate as JUCE reads them",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    if (!params.count("file")) {
      error = "params.file is required";
      return false;
    }
    std::string path = context.corpusPath(params["file"].get<std::string>());

    juce::AudioFormatManager format_manager;
    format_manager.registerBasicFormats();
    std::unique_ptr<juce::AudioFormatReader> reader(format_manager.createReaderFor(juce::File(path)));
    if (reader == nullptr) {
      error = "no JUCE reader for " + path;
      return false;
    }

    int num_samples = static_cast<int>(reader->lengthInSamples);
    juce::AudioSampleBuffer buffer(static_cast<int>(reader->numChannels), num_samples);
    reader->read(&buffer, 0, num_samples, 0, true, true);
    const float* first = buffer.getReadPointer(0);
    sink.array("first_channel", std::vector<float>(first, first + num_samples), 1);
    sink.text("sample_rate.txt", std::to_string(static_cast<int>(reader->sampleRate)) + "\n");
    return true;
  },
});
