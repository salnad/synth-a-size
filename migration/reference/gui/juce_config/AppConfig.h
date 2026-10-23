// JUCE configuration for ref-gui, the upstream interface screenshot tool.
//
// Reuses upstream's standalone AppConfig.h verbatim and only pre-defines the
// options it guards with #ifndef: no network, no web view, no audio devices
// (the tool never opens one), so the build needs no curl, webkit, ALSA or JACK.
#pragma once

#ifndef JUCE_USE_CURL
 #define JUCE_USE_CURL 0
#endif

#ifndef JUCE_WEB_BROWSER
 #define JUCE_WEB_BROWSER 0
#endif

#ifndef JUCE_ALSA
 #define JUCE_ALSA 0
#endif

#ifndef JUCE_JACK
 #define JUCE_JACK 0
#endif

#include "../../../../vital/standalone/JuceLibraryCode/AppConfig.h"
