# Reference harness for the synth-a-size port.
#
# Builds the upstream engine (headless configuration: HEADLESS=1, NO_AUTH=1,
# no GUI, no Firebase) straight out of ../../vital without modifying it, and
# links two small CLIs written for differential testing:
#
#   ref-render  - render a .vital preset + .mid file to a WAV
#   ref-module  - drive a single engine Processor and dump its buffers (.npy)
#
# Linux x86_64 is the supported reference platform (that is what CI uses).

cmake_minimum_required(VERSION 3.16)
# macOS: one deployment target for every reference build, so a binary built on
# a newer macOS runner also runs on macOS 14 (used to separate Apple libm /
# OS drift from compiler drift; migration/decisions/reference-arm.md).
set(CMAKE_OSX_DEPLOYMENT_TARGET "14.0" CACHE STRING "Minimum macOS version")
project(synth_a_size_reference CXX)

set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release CACHE STRING "Build type" FORCE)
endif()

# Upstream's release builds use -Ofast (= -O3 -ffast-math). Keep that as the
# default so the reference behaves like the shipped plugin; the flag is
# recorded in every render's metadata.
option(REF_FAST_MATH "Compile the reference with -ffast-math like upstream release builds" ON)

# Experiment switches, OFF for the reference (migration/decisions/reference-arm.md):
# REF_FP_CONTRACT lets the compiler fuse a*b+c into FMA (GCC on aarch64 and
# Apple clang do so by default); REF_USE_VDSP lets JUCE use Accelerate/vDSP
# for the FFT on Apple, as upstream's Mac builds do.
option(REF_FP_CONTRACT "Allow FMA contraction (experiment only)" OFF)
option(REF_USE_VDSP "Apple only: JUCE FFT via Accelerate/vDSP (experiment only)" OFF)

get_filename_component(REPO_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../.." ABSOLUTE)
set(VITAL_DIR "${REPO_ROOT}/vital")
set(JUCE_MODULES "${VITAL_DIR}/third_party/JUCE/modules")

if(NOT EXISTS "${VITAL_DIR}/src/unity_build/synthesis.cpp")
  message(FATAL_ERROR "Upstream sources not found at ${VITAL_DIR}")
endif()

file(READ "${REPO_ROOT}/migration/UPSTREAM_SHA" UPSTREAM_SHA)
string(STRIP "${UPSTREAM_SHA}" UPSTREAM_SHA)

find_package(Threads REQUIRED)

set(JUCE_UNITS
  juce_audio_basics
  juce_audio_formats
  juce_core
  juce_data_structures
  juce_dsp
  juce_events
)
# JUCE needs Objective-C++ on Apple platforms (same split as upstream's
# headless JuceLibraryCode: include_*.cpp vs include_*.mm).
if(APPLE)
  enable_language(OBJCXX)
  set(CMAKE_OBJCXX_STANDARD 14)
  set(CMAKE_OBJCXX_STANDARD_REQUIRED ON)
  set(JUCE_UNIT_EXT mm)
else()
  set(JUCE_UNIT_EXT cpp)
endif()

set(JUCE_SOURCES "")
foreach(unit ${JUCE_UNITS})
  list(APPEND JUCE_SOURCES "${CMAKE_CURRENT_SOURCE_DIR}/juce_config/include_${unit}.${JUCE_UNIT_EXT}")
endforeach()

# Engine + JUCE in one static library shared by both CLIs.
add_library(upstream_engine STATIC
  ${JUCE_SOURCES}
  "${VITAL_DIR}/src/unity_build/common.cpp"
  "${VITAL_DIR}/src/unity_build/synthesis.cpp"
)

target_include_directories(upstream_engine PUBLIC
  # Must come first: provides JuceHeader.h / AppConfig.h for the engine.
  "${CMAKE_CURRENT_SOURCE_DIR}/juce_config"
  "${JUCE_MODULES}"
  "${VITAL_DIR}/src/common"
  "${VITAL_DIR}/src/common/wavetable"
  # Header-only use from the engine (e.g. synth_strings.h); same include list
  # as upstream's headless Makefile. No GUI code is compiled.
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
)

if(NOT APPLE)
  target_compile_definitions(upstream_engine PUBLIC LINUX=1)
endif()

target_compile_definitions(upstream_engine PUBLIC
  NDEBUG=1
  HEADLESS=1
  NO_AUTH=1
  JUCE_USE_XRANDR=0
  # Upstream sets JUCE_DSP_USE_SHARED_FFTW=1, which makes JUCE dlopen
  # libfftw3f at runtime if present. That would make renders depend on what
  # is installed on the machine, so the reference always uses JUCE's
  # built-in FFT instead.
  JUCE_DSP_USE_SHARED_FFTW=0
  "JucePlugin_Build_VST=0"
  "JucePlugin_Build_VST3=0"
  "JucePlugin_Build_AU=0"
  "JucePlugin_Build_AUv3=0"
  "JucePlugin_Build_RTAS=0"
  "JucePlugin_Build_AAX=0"
  "JucePlugin_Build_Standalone=0"
  "JucePlugin_Build_Unity=0"
  "REF_UPSTREAM_SHA=\"${UPSTREAM_SHA}\""
  "REF_FAST_MATH=$<BOOL:${REF_FAST_MATH}>"
)

set(REF_OPT_FLAGS -O3 -ftree-vectorize -ftree-slp-vectorize -funroll-loops)
if(REF_FAST_MATH)
  # -U__FAST_MATH__: glibc's <bits/math-vector.h> only declares the libmvec
  # SIMD variants of sinf/asinf/... when __FAST_MATH__ is defined. Without
  # it GCC keeps scalar libm calls instead of auto-vectorising them into
  # libmvec's SSE kernels, which use RCPPS/RSQRTPS internally and so return
  # CPU-dependent bits. Nothing else in the build reads the macro.
  list(APPEND REF_OPT_FLAGS -ffast-math -U__FAST_MATH__)
endif()
if(CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|AMD64|amd64")
  # Cross-machine determinism (migration/decisions/reference-determinism.md):
  # - Pin the target ISA to baseline x86-64 (SSE2) so no build host
  #   features leak in.
  # - -mrecip=none: under -ffast-math GCC otherwise turns vectorised float
  #   division and sqrtf into RCPPS/RSQRTPS plus a Newton step. Those
  #   instructions return implementation-defined bits (Intel and AMD differ),
  #   which made the same binary render differently on different CPUs.
  # - -ffp-contract=off keeps FMA contraction out even if someone overrides
  #   -march with an FMA-capable target.
  list(APPEND REF_OPT_FLAGS -march=x86-64 -mtune=generic -msse2 -mfpmath=sse -mno-recip -ffp-contract=off)
elseif(CMAKE_SYSTEM_PROCESSOR MATCHES "aarch64|arm64|ARM64")
  # ARM determinism (migration/decisions/reference-arm.md):
  # - GCC (GNU mode) defaults to -ffp-contract=fast and Apple clang to
  #   -ffp-contract=on; both fuse a*b+c into FMADD/FMLA, including upstream's
  #   NEON mulAdd/mulSub (vmlaq_f32/vmlsq_f32 are written as a + b * c in
  #   arm_neon.h). Turning contraction off gives the same unfused
  #   multiply-then-add as the x86 SSE2 path.
  # - GCC: pin the baseline ISA and generic tuning (some -mtune targets
  #   enable FRECPE/FRSQRTE-based division and sqrt approximations).
  # - Apple: pin JUCE's FFT to its portable fallback instead of vDSP, the
  #   same FFT code the x86 reference runs.
  if(CMAKE_CXX_COMPILER_ID STREQUAL "GNU")
    list(APPEND REF_OPT_FLAGS -march=armv8-a -mtune=generic)
  endif()
  if(NOT REF_FP_CONTRACT)
    list(APPEND REF_OPT_FLAGS -ffp-contract=off)
  endif()
  if(CMAKE_CXX_COMPILER_ID STREQUAL "AppleClang" AND CMAKE_CXX_COMPILER_VERSION VERSION_LESS 17)
    # Apple clang 15 passes -menable-unsafe-fp-math for -ffast-math, and its
    # backend then fuses FMAs even with -ffp-contract=off (measured by
    # scripts/fp_contract_flags_probe.sh). check_fp_determinism.sh fails such
    # a build; the macOS reference identity uses Apple clang 17 (macOS 15).
    message(WARNING "Apple clang < 17 ignores -ffp-contract=off under -ffast-math; this build will not match the macOS reference")
  endif()
endif()
if(APPLE)
  if(REF_USE_VDSP)
    list(APPEND REF_OPT_FLAGS -DJUCE_USE_VDSP_FRAMEWORK=1)
  else()
    list(APPEND REF_OPT_FLAGS -DJUCE_USE_VDSP_FRAMEWORK=0)
  endif()
endif()

list(JOIN REF_OPT_FLAGS " " REF_OPT_FLAGS_STRING)
message(STATUS "Reference: ${CMAKE_CXX_COMPILER_ID} ${CMAKE_CXX_COMPILER_VERSION} on ${CMAKE_SYSTEM_NAME}/${CMAKE_SYSTEM_PROCESSOR}")
message(STATUS "Reference optimisation flags: ${REF_OPT_FLAGS_STRING}")
target_compile_definitions(upstream_engine PUBLIC "REF_OPT_FLAGS=\"${REF_OPT_FLAGS_STRING}\"")

target_compile_options(upstream_engine PUBLIC
  ${REF_OPT_FLAGS}
  # Upstream code + JUCE 6.0.5 are noisy on modern compilers; warnings in
  # code we may not modify are not actionable here.
  -w
)

target_link_libraries(upstream_engine PUBLIC Threads::Threads ${CMAKE_DL_LIBS})
if(APPLE)
  # OSXFrameworks declared by the six JUCE modules.
  target_link_libraries(upstream_engine PUBLIC
    "-framework Cocoa" "-framework Foundation" "-framework IOKit"
    "-framework CoreAudio" "-framework CoreMIDI" "-framework QuartzCore"
    "-framework AudioToolbox" "-framework Accelerate")
else()
  target_link_libraries(upstream_engine PUBLIC rt)
endif()

add_library(harness_common STATIC src/harness_io.cpp)
target_link_libraries(harness_common PUBLIC upstream_engine)

add_executable(ref-render src/render_main.cpp)
target_link_libraries(ref-render PRIVATE harness_common)

# One file per module under src/modules/<area>/<module>.cpp; each registers
# itself (REGISTER_REF_MODULE), so new modules never edit a shared list.
# Compiled straight into the executable (not a static library) so the linker
# keeps every registration.
file(GLOB_RECURSE REF_MODULE_SOURCES CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/src/modules/*.cpp")
add_executable(ref-module src/module_main.cpp src/module_registry.cpp ${REF_MODULE_SOURCES})
target_include_directories(ref-module PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/src")
target_link_libraries(ref-module PRIVATE harness_common)

# Diagnostic: hashes upstream's RandomGenerator streams. Linked against the
# engine (same flags) on purpose, unlike ref-fp-probe.
add_executable(ref-rng-probe src/rng_probe.cpp)
target_link_libraries(ref-rng-probe PRIVATE harness_common)

# Diagnostic: hashes libm and rcpps/rsqrtps results so two machines can be
# compared directly (see migration/decisions/reference-determinism.md).
add_executable(ref-fp-probe src/fp_probe.cpp)
target_compile_options(ref-fp-probe PRIVATE -O2 -fno-fast-math)
if(CMAKE_SYSTEM_NAME STREQUAL "Linux" AND CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|AMD64|amd64")
  target_compile_definitions(ref-fp-probe PRIVATE REF_PROBE_LIBMVEC=1)
  target_link_libraries(ref-fp-probe PRIVATE mvec m)
endif()
