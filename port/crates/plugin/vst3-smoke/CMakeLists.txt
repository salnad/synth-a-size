# SPDX-License-Identifier: GPL-3.0-or-later
#
# CI-only tools against a VST3 SDK checkout (MIT since 3.8; never shipped or
# linked into the plugin): `vst3-smoke`, which loads the bundle like a host,
# plays a note and checks the output is not silent; `vst3-editor-smoke`, which
# opens the editor in a real window (X11 on Linux); and the SDK's own
# `validator`.
#
#   cmake -S port/crates/plugin/vst3-smoke -B build/vst3-tools \
#     -DVST3_SDK_ROOT=<vst3sdk> -DCMAKE_BUILD_TYPE=Release
#   cmake --build build/vst3-tools --target vst3-smoke vst3-editor-smoke validator
cmake_minimum_required(VERSION 3.19)
project(sas_vst3_smoke CXX)
if(APPLE)
  enable_language(OBJCXX)
endif()

set(VST3_SDK_ROOT "" CACHE PATH "VST3 SDK checkout")
if(NOT VST3_SDK_ROOT)
  message(FATAL_ERROR "set VST3_SDK_ROOT")
endif()

set(SMTG_ENABLE_VSTGUI_SUPPORT OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_VST3_PLUGIN_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SMTG_ENABLE_VST3_HOSTING_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SMTG_RUN_VST_VALIDATOR OFF CACHE BOOL "" FORCE)
add_subdirectory(${VST3_SDK_ROOT} vst3sdk EXCLUDE_FROM_ALL)

set(hosting ${VST3_SDK_ROOT}/public.sdk/source/vst/hosting/plugprovider.cpp)
if(APPLE)
  list(APPEND hosting ${VST3_SDK_ROOT}/public.sdk/source/vst/hosting/module_mac.mm)
  set_source_files_properties(${VST3_SDK_ROOT}/public.sdk/source/vst/hosting/module_mac.mm
    PROPERTIES COMPILE_FLAGS "-fobjc-arc")
  set_source_files_properties(editor_smoke.cpp
    PROPERTIES LANGUAGE OBJCXX COMPILE_FLAGS "-fobjc-arc")
elseif(UNIX)
  list(APPEND hosting ${VST3_SDK_ROOT}/public.sdk/source/vst/hosting/module_linux.cpp)
endif()

add_executable(vst3-smoke smoke.cpp ${hosting})
add_executable(vst3-editor-smoke editor_smoke.cpp ${hosting})
foreach(tool vst3-smoke vst3-editor-smoke)
  target_compile_features(${tool} PRIVATE cxx_std_17)
  target_link_libraries(${tool} PRIVATE sdk_hosting)
  if(APPLE)
    find_library(COCOA_FRAMEWORK Cocoa)
    target_link_libraries(${tool} PRIVATE ${COCOA_FRAMEWORK})
  endif()
endforeach()
if(UNIX AND NOT APPLE)
  find_package(X11 REQUIRED)
  target_link_libraries(vst3-editor-smoke PRIVATE X11::X11)
endif()
