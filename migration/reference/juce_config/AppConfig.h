// JUCE configuration for the reference harness.
//
// This reuses upstream's headless AppConfig.h verbatim and only pre-defines the
// options it guards with #ifndef, so the engine is configured exactly like the
// upstream headless target apart from the overrides below.
#pragma once

// No network stack: avoids a libcurl build dependency. The engine never uses
// it in the headless + NO_AUTH configuration.
#ifndef JUCE_USE_CURL
 #define JUCE_USE_CURL 0
#endif

#ifndef JUCE_WEB_BROWSER
 #define JUCE_WEB_BROWSER 0
#endif

#include "../../../vital/headless/JuceLibraryCode/AppConfig.h"
