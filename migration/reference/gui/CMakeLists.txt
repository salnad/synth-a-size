# ref-gui: upstream's interface, built from ../../../vital without modifying
# it, rendering one preset and tab to a PNG (README.md).
#
# A separate project from the audio reference (../CMakeLists.txt), so the
# parity build keeps its headless JUCE configuration and needs no GUI
# libraries. Linux only: X11, freetype and OpenGL (Mesa in CI).

cmake_minimum_required(VERSION 3.16)
project(synth_a_size_reference_gui CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

get_filename_component(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
set(VITAL_DIR "${REPO_ROOT}/vital")
set(JUCE_MODULES "${VITAL_DIR}/third_party/JUCE/modules")

find_package(Threads REQUIRED)
find_package(PkgConfig REQUIRED)
find_package(OpenGL REQUIRED)
pkg_check_modules(GUI_DEPS REQUIRED freetype2 x11)

set(JUCE_UNITS
  audio_basics audio_devices audio_formats audio_processors audio_utils core
  data_structures dsp events graphics gui_basics gui_extra opengl)
set(JUCE_SOURCES "")
foreach(unit ${JUCE_UNITS})
  list(APPEND JUCE_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/juce_config/include_juce_${unit}.cpp")
endforeach()

add_executable(ref-gui
  src/gui_main.cpp
  ${JUCE_SOURCES}
  "${VITAL_DIR}/standalone/JuceLibraryCode/BinaryData.cpp"
  "${VITAL_DIR}/src/unity_build/common.cpp"
  "${VITAL_DIR}/src/unity_build/synthesis.cpp"
  "${VITAL_DIR}/src/unity_build/interface_editor_components.cpp"
  "${VITAL_DIR}/src/unity_build/interface_editor_sections.cpp"
  "${VITAL_DIR}/src/unity_build/interface_editor_sections2.cpp"
  "${VITAL_DIR}/src/unity_build/interface_look_and_feel.cpp"
  "${VITAL_DIR}/src/unity_build/interface_wavetable.cpp"
)

target_include_directories(ref-gui PRIVATE
  # First: our AppConfig.h / JuceHeader.h; BinaryData.h comes from upstream.
  "${CMAKE_CURRENT_SOURCE_DIR}/juce_config"
  "${VITAL_DIR}/standalone/JuceLibraryCode"
  "${JUCE_MODULES}"
  "${VITAL_DIR}/src/common"
  "${VITAL_DIR}/src/common/wavetable"
  "${VITAL_DIR}/src/interface/editor_components"
  "${VITAL_DIR}/src/interface/editor_sections"
  "${VITAL_DIR}/src/interface/look_and_feel"
  "${VITAL_DIR}/src/interface/wavetable"
  "${VITAL_DIR}/src/interface/wavetable/editors"
  "${VITAL_DIR}/src/interface/wavetable/overlays"
  "${VITAL_DIR}/src/synthesis/synth_engine"
  "${VITAL_DIR}/src/synthesis/effects"
  "${VITAL_DIR}/src/synthesis/filters"
  "${VITAL_DIR}/src/synthesis/framework"
  "${VITAL_DIR}/src/synthesis/lookups"
  "${VITAL_DIR}/src/synthesis/modulators"
  "${VITAL_DIR}/src/synthesis/modules"
  "${VITAL_DIR}/src/synthesis/producers"
  "${VITAL_DIR}/src/synthesis/utilities"
  "${VITAL_DIR}/third_party"
  ${GUI_DEPS_INCLUDE_DIRS}
)

# Upstream's standalone defines, with NO_AUTH=1 (rule 4) and without
# REQUIRE_AUTH or Firebase.
target_compile_definitions(ref-gui PRIVATE
  LINUX=1
  NDEBUG=1
  NO_AUTH=1
  JUCE_USE_XRANDR=0
  JUCE_OPENGL3=1
  JUCE_DSP_USE_SHARED_FFTW=0
  "JucePlugin_Build_VST=0"
  "JucePlugin_Build_VST3=0"
  "JucePlugin_Build_AU=0"
  "JucePlugin_Build_AUv3=0"
  "JucePlugin_Build_RTAS=0"
  "JucePlugin_Build_AAX=0"
  "JucePlugin_Build_Standalone=0"
  "JucePlugin_Build_Unity=0"
)

# Pixels, not audio, are compared here, so plain -O2 is enough. Upstream and
# JUCE 6.0.5 warn a lot on modern compilers; nothing actionable in code we may
# not modify.
target_compile_options(ref-gui PRIVATE -O2 -w)

target_link_libraries(ref-gui PRIVATE
  Threads::Threads ${CMAKE_DL_LIBS} rt OpenGL::GL ${GUI_DEPS_LIBRARIES})
