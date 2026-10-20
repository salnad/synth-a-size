// JuceHeader.h for the reference harness.
//
// Same module set as upstream's headless target. ProjectInfo matches the
// upstream *plugin* build (version 1.0.6) rather than the headless target's
// placeholder 99999.9.9, so preset version handling (LoadSave::jsonToState,
// updateFromOldVersion) and saved "synth_version" fields behave like the
// shipped plugin. The strings are upstream's own identifiers; they never
// appear in synth-a-size product code.
#pragma once

#include "AppConfig.h"

#include <juce_audio_basics/juce_audio_basics.h>
#include <juce_audio_formats/juce_audio_formats.h>
#include <juce_core/juce_core.h>
#include <juce_data_structures/juce_data_structures.h>
#include <juce_dsp/juce_dsp.h>
#include <juce_events/juce_events.h>

#if ! DONT_SET_USING_JUCE_NAMESPACE
 using namespace juce;
#endif

namespace ProjectInfo
{
    const char* const  projectName    = "Vial";
    const char* const  companyName    = "Vial Audio";
    const char* const  versionString  = "1.0.6";
    const int          versionNumber  = 0x10006;
}
