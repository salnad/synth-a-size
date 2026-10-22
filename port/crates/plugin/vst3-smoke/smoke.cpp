// SPDX-License-Identifier: GPL-3.0-or-later
//
// VST3 sound check for the synth-a-size bundle: loads it through the VST3
// SDK's hosting helpers, as a host would, plays one note and reports the
// output peak while the note is held and after the release. Exits non-zero
// if the held peak is below --min-peak (CI-only tool; never shipped).
//
//   vst3-smoke <bundle.vst3> [--min-peak 0.01] [--sample-rate 48000] [--block 512]

#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstcomponent.h"
#include "pluginterfaces/vst/ivstevents.h"
#include "public.sdk/source/vst/hosting/eventlist.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/plugprovider.h"
#include "public.sdk/source/vst/hosting/processdata.h"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>

using namespace Steinberg;
using namespace Steinberg::Vst;

namespace {

Event note_event(bool on, int16 pitch, float velocity) {
  Event event {};
  event.busIndex = 0;
  event.sampleOffset = 0;
  if (on) {
    event.type = Event::kNoteOnEvent;
    event.noteOn.channel = 0;
    event.noteOn.pitch = pitch;
    event.noteOn.velocity = velocity;
    event.noteOn.noteId = -1;
  }
  else {
    event.type = Event::kNoteOffEvent;
    event.noteOff.channel = 0;
    event.noteOff.pitch = pitch;
    event.noteOff.velocity = 0.0f;
    event.noteOff.noteId = -1;
  }
  return event;
}

float block_peak(const HostProcessData& data) {
  float peak = 0.0f;
  for (int32 bus = 0; bus < data.numOutputs; ++bus) {
    const AudioBusBuffers& buffers = data.outputs[bus];
    for (int32 channel = 0; channel < buffers.numChannels; ++channel) {
      for (int32 i = 0; i < data.numSamples; ++i) {
        float sample = buffers.channelBuffers32[channel][i];
        if (!std::isfinite(sample))
          return INFINITY;
        peak = std::max(peak, std::fabs(sample));
      }
    }
  }
  return peak;
}

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    std::fprintf(stderr, "usage: %s <bundle.vst3> [--min-peak X] [--sample-rate R] [--block N]\n", argv[0]);
    return 2;
  }
  std::string path = argv[1];
  double min_peak = 0.01;
  double sample_rate = 48000.0;
  int32 block = 512;
  for (int i = 2; i + 1 < argc; i += 2) {
    if (!std::strcmp(argv[i], "--min-peak"))
      min_peak = std::atof(argv[i + 1]);
    else if (!std::strcmp(argv[i], "--sample-rate"))
      sample_rate = std::atof(argv[i + 1]);
    else if (!std::strcmp(argv[i], "--block"))
      block = std::atoi(argv[i + 1]);
  }

  std::string error;
  auto module = VST3::Hosting::Module::create(path, error);
  if (!module) {
    std::fprintf(stderr, "cannot load %s: %s\n", path.c_str(), error.c_str());
    return 1;
  }
  HostApplication host;
  PluginContextFactory::instance().setPluginContext(&host);

  const auto& factory = module->getFactory();
  for (const auto& info : factory.classInfos()) {
    if (info.category() != kVstAudioEffectClass)
      continue;
    std::printf("class: %s (%s, %s)\n", info.name().c_str(), info.vendor().c_str(),
                info.subCategoriesString().c_str());

    auto provider = owned(new PlugProvider(factory, info, true));
    if (!provider->initialize()) {
      std::fprintf(stderr, "cannot initialize the plug-in\n");
      return 1;
    }
    IPtr<IComponent> component = provider->getComponentPtr();
    FUnknownPtr<IAudioProcessor> processor(component);
    if (!processor) {
      std::fprintf(stderr, "no IAudioProcessor\n");
      return 1;
    }

    ProcessSetup setup {kRealtime, kSample32, block, sample_rate};
    if (processor->setupProcessing(setup) != kResultOk) {
      std::fprintf(stderr, "setupProcessing failed\n");
      return 1;
    }
    for (int32 i = 0; i < component->getBusCount(kEvent, kInput); ++i)
      component->activateBus(kEvent, kInput, i, true);
    for (int32 i = 0; i < component->getBusCount(kAudio, kOutput); ++i)
      component->activateBus(kAudio, kOutput, i, true);
    component->setActive(true);
    processor->setProcessing(true);

    HostProcessData data;
    data.prepare(*component, block, kSample32);
    EventList events;
    ProcessContext context {};
    context.sampleRate = sample_rate;
    context.tempo = 120.0;
    context.state = ProcessContext::kTempoValid;
    data.inputEvents = &events;
    data.processContext = &context;
    data.numSamples = block;

    const int held_blocks = static_cast<int>(sample_rate / block);  // ~1 s
    const int release_blocks = static_cast<int>(2 * sample_rate / block);
    float silent_peak = 0.0f, held_peak = 0.0f, release_peak = 0.0f;
    for (int i = 0; i < 4 + held_blocks + release_blocks; ++i) {
      events.clear();
      if (i == 4) {
        Event on = note_event(true, 60, 0.8f);
        events.addEvent(on);
      }
      if (i == 4 + held_blocks) {
        Event off = note_event(false, 60, 0.0f);
        events.addEvent(off);
      }
      if (processor->process(data) != kResultOk) {
        std::fprintf(stderr, "process failed at block %d\n", i);
        return 1;
      }
      float peak = block_peak(data);
      if (i < 4)
        silent_peak = std::max(silent_peak, peak);
      else if (i < 4 + held_blocks)
        held_peak = std::max(held_peak, peak);
      else
        release_peak = std::max(release_peak, peak);
      context.continousTimeSamples += block;
      context.projectTimeSamples += block;
    }

    processor->setProcessing(false);
    component->setActive(false);
    data.unprepare();

    std::printf("peak before the note: %g\n", silent_peak);
    std::printf("peak while held:      %g\n", held_peak);
    std::printf("peak after release:   %g\n", release_peak);
    if (!std::isfinite(held_peak) || !std::isfinite(release_peak)) {
      std::fprintf(stderr, "non-finite output\n");
      return 1;
    }
    if (held_peak < min_peak) {
      std::fprintf(stderr, "held peak %g is below %g\n", held_peak, min_peak);
      return 1;
    }
    std::printf("sound check passed (min peak %g)\n", min_peak);
    return 0;
  }
  std::fprintf(stderr, "no audio processor class in %s\n", path.c_str());
  return 1;
}
