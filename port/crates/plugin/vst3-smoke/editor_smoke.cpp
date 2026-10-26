// SPDX-License-Identifier: GPL-3.0-or-later
//
// Editor-open check for the synth-a-size bundle: loads it through the VST3
// SDK's hosting helpers, as a host would, creates the editor view and
// attaches it to a real window (an NSWindow on macOS; an X11 window on
// Linux, e.g. under Xvfb), keeps it open for --seconds while it draws, then
// removes it. Exits non-zero if the view cannot be created or attached; a
// panic that reached the host would abort the process. scripts/editor_smoke.sh
// also checks the plugin's log for the first presented frame and for caught
// panics or wgpu errors (CI-only tool; never shipped).
//
//   vst3-editor-smoke <bundle.vst3> [--seconds 5]
//
// Compiled as Objective-C++ on macOS.

#include "pluginterfaces/gui/iplugview.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivsteditcontroller.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/plugprovider.h"

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <thread>

#ifdef __APPLE__
#import <Cocoa/Cocoa.h>
#else
#include <X11/Xlib.h>
#endif

using namespace Steinberg;
using namespace Steinberg::Vst;

namespace {

using Clock = std::chrono::steady_clock;

#ifdef __APPLE__
struct HostWindow {
  NSWindow* window = nil;

  bool open(int width, int height) {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    [NSApp finishLaunching];
    window = [[NSWindow alloc] initWithContentRect:NSMakeRect(100, 100, width, height)
                                         styleMask:NSWindowStyleMaskTitled
                                           backing:NSBackingStoreBuffered
                                             defer:NO];
    [window setTitle:@"vst3-editor-smoke"];
    [window makeKeyAndOrderFront:nil];
    [NSApp activateIgnoringOtherApps:YES];
    return window != nil;
  }

  void* parent() { return (__bridge void*)[window contentView]; }
  FIDString platform() { return kPlatformTypeNSView; }

  // Runs the main run loop, where the editor's display link and events fire.
  void pump(double seconds) {
    auto until = Clock::now() + std::chrono::duration<double>(seconds);
    while (Clock::now() < until) {
      @autoreleasepool {
        NSEvent* event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                            untilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]
                                               inMode:NSDefaultRunLoopMode
                                              dequeue:YES];
        if (event)
          [NSApp sendEvent:event];
      }
    }
  }

  void close() {
    [window close];
    window = nil;
  }
};
#else
struct HostWindow {
  Display* display = nullptr;
  Window window = 0;

  bool open(int width, int height) {
    display = XOpenDisplay(nullptr);
    if (!display) {
      std::fprintf(stderr, "no X display (run under xvfb-run)\n");
      return false;
    }
    window = XCreateSimpleWindow(display, DefaultRootWindow(display), 0, 0, width, height, 0, 0, 0);
    XStoreName(display, window, "vst3-editor-smoke");
    XMapWindow(display, window);
    XSync(display, False);
    return true;
  }

  void* parent() { return reinterpret_cast<void*>(static_cast<uintptr_t>(window)); }
  FIDString platform() { return kPlatformTypeX11EmbedWindowID; }

  // The editor runs its own X11 event thread; this only keeps ours drained.
  void pump(double seconds) {
    auto until = Clock::now() + std::chrono::duration<double>(seconds);
    while (Clock::now() < until) {
      while (XPending(display)) {
        XEvent event;
        XNextEvent(display, &event);
      }
      std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
  }

  void close() {
    XDestroyWindow(display, window);
    XCloseDisplay(display);
    display = nullptr;
  }
};
#endif

}  // namespace

int main(int argc, char* argv[]) {
  if (argc < 2) {
    std::fprintf(stderr, "usage: %s <bundle.vst3> [--seconds S]\n", argv[0]);
    return 2;
  }
  std::string path = argv[1];
  double seconds = 5.0;
  for (int i = 2; i + 1 < argc; i += 2) {
    if (!std::strcmp(argv[i], "--seconds"))
      seconds = std::atof(argv[i + 1]);
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
    std::printf("class: %s\n", info.name().c_str());

    auto provider = owned(new PlugProvider(factory, info, true));
    if (!provider->initialize()) {
      std::fprintf(stderr, "cannot initialize the plug-in\n");
      return 1;
    }
    IPtr<IEditController> controller = provider->getControllerPtr();
    if (!controller) {
      std::fprintf(stderr, "no IEditController\n");
      return 1;
    }
    IPtr<IPlugView> view = owned(controller->createView(ViewType::kEditor));
    if (!view) {
      std::fprintf(stderr, "createView returned no editor\n");
      return 1;
    }

    HostWindow window;
    ViewRect rect;
    if (view->getSize(&rect) != kResultOk || rect.getWidth() <= 0 || rect.getHeight() <= 0) {
      std::fprintf(stderr, "the editor has no size\n");
      return 1;
    }
    std::printf("editor size: %d x %d\n", rect.getWidth(), rect.getHeight());
    if (!window.open(rect.getWidth(), rect.getHeight()))
      return 1;
    if (view->isPlatformTypeSupported(window.platform()) != kResultTrue) {
      std::fprintf(stderr, "the editor does not support %s\n", window.platform());
      return 1;
    }
    if (view->attached(window.parent(), window.platform()) != kResultOk) {
      std::fprintf(stderr, "attaching the editor failed\n");
      return 1;
    }
    std::printf("editor attached; drawing for %g s\n", seconds);
    std::fflush(stdout);
    window.pump(seconds);

    if (view->removed() != kResultOk) {
      std::fprintf(stderr, "removing the editor failed\n");
      return 1;
    }
    view = nullptr;
    window.pump(0.5);
    window.close();
    std::printf("editor opened, drew and closed\n");
    return 0;
  }
  std::fprintf(stderr, "no audio processor class in %s\n", path.c_str());
  return 1;
}
