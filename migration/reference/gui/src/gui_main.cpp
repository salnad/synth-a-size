// ref-gui: renders upstream's interface (vital/src/interface, unmodified) for
// one preset and one tab, and writes the frame as a PNG.
//
// The port's editor is compared against these frames (migration/reference/gui/
// README.md). They show upstream's branding, so they are CI artifacts only and
// are never committed.
//
// Usage:
//   ref-gui [--out frame.png] [--layout layout.jsonl] [--preset file.vital]
//           [--tab voice|effects|matrix|advanced] [--width 1400] [--height 820] [--frames 30]
//           [--effects-scroll 0] [--animate 0]
//
// --effects-scroll scrolls the effects column down that many pixels (the
// viewport keeps it within the column).
//
// --animate 1 renders with animation on (FullInterface::animate), which
// the modulation overlay needs to draw at all (its amount knobs and
// meters); other animated views then show the idle engine's outputs.
//
// --layout writes the bounds of every named component, relative to the
// interface, so the port's layout can be checked exactly. Without --out
// the tool exits after writing it.
//
// Runs on any X display; CI uses Xvfb with Mesa's software OpenGL.

#include "JuceHeader.h"

#include "effects_interface.h"
#include "full_interface.h"
#include "sound_engine.h"
#include "synth_base.h"
#include "synth_gui_interface.h"
#include "tab_selector.h"

#include <cstdio>
#include <string>

namespace {

struct Options {
  std::string out;
  std::string preset;
  std::string tab = "voice";
  int width = vital::kDefaultWindowWidth;
  int height = vital::kDefaultWindowHeight;
  int frames = 30;
  int effects_scroll = 0;
  bool animate = false;
  std::string layout;
};

int tabIndex(const std::string& tab) {
  static const char* kTabs[] = { "voice", "effects", "matrix", "advanced" };
  for (int i = 0; i < 4; ++i) {
    if (tab == kTabs[i])
      return i;
  }
  return -1;
}

EffectsViewport* findEffectsViewport(Component* component) {
  if (auto* viewport = dynamic_cast<EffectsViewport*>(component))
    return viewport;
  for (Component* child : component->getChildren()) {
    if (EffectsViewport* found = findEffectsViewport(child))
      return found;
  }
  return nullptr;
}

TabSelector* findTabSelector(Component* component) {
  if (auto* selector = dynamic_cast<TabSelector*>(component)) {
    if (selector->getName() == "tab_selector")
      return selector;
  }
  for (Component* child : component->getChildren()) {
    if (TabSelector* found = findTabSelector(child))
      return found;
  }
  return nullptr;
}

// One line per named component: path from the interface, visibility and
// bounds relative to the interface, in tree order.
void dumpLayout(Component* root, Component* component, const std::string& path, std::string& out) {
  for (Component* child : component->getChildren()) {
    std::string name = child->getName().toStdString();
    std::string child_path = name.empty() ? path : (path.empty() ? name : path + "/" + name);
    if (!name.empty()) {
      Rectangle<int> bounds = root->getLocalArea(component, child->getBounds());
      json entry;
      entry["path"] = child_path;
      entry["visible"] = child->isVisible();
      entry["bounds"] = { bounds.getX(), bounds.getY(), bounds.getWidth(), bounds.getHeight() };
      out += entry.dump() + "\n";
    }
    dumpLayout(root, child, child_path, out);
  }
}

// Upstream's FullInterface, read back after its own render of the chosen frame.
class CaptureInterface : public FullInterface {
  public:
    CaptureInterface(SynthGuiData* data, const Options& options) : FullInterface(data), options_(options) { }

    void renderOpenGL() override {
      FullInterface::renderOpenGL();
      if (done_)
        return;
      if (++frame_ < options_.frames) {
        // Animation is off (see initialise), so nothing repaints by itself.
        if (OpenGLContext* context = OpenGLContext::getCurrentContext())
          context->triggerRepaint();
        return;
      }

      done_ = true;
      OpenGLContext* context = OpenGLContext::getCurrentContext();
      double scale = context ? context->getRenderingScale() : 1.0;
      int width = static_cast<int>(scale * getWidth());
      int height = static_cast<int>(scale * getHeight());
      std::vector<uint8_t> pixels(4 * width * height);
      glPixelStorei(GL_PACK_ALIGNMENT, 1);
      glReadPixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, pixels.data());

      Image image(Image::ARGB, width, height, false);
      Image::BitmapData bitmap(image, Image::BitmapData::writeOnly);
      for (int y = 0; y < height; ++y) {
        const uint8_t* row = pixels.data() + 4 * width * (height - 1 - y);
        for (int x = 0; x < width; ++x) {
          const uint8_t* p = row + 4 * x;
          bitmap.setPixelColour(x, y, Colour(p[0], p[1], p[2]));
        }
      }

      std::string out = options_.out;
      MessageManager::callAsync([image, out] {
        File file = File::getCurrentWorkingDirectory().getChildFile(out);
        file.deleteFile();
        FileOutputStream stream(file);
        PNGImageFormat png;
        bool ok = stream.openedOk() && png.writeImageToStream(image, stream);
        stream.flush();
        if (!ok)
          std::fprintf(stderr, "ref-gui: cannot write %s\n", out.c_str());
        JUCEApplicationBase::getInstance()->setApplicationReturnValue(ok ? 0 : 1);
        JUCEApplicationBase::quit();
      });
    }

  private:
    Options options_;
    int frame_ = 0;
    bool done_ = false;
};

// The window itself, as upstream's standalone SynthEditor: the interface finds
// its synth with findParentComponentOfClass<SynthGuiInterface>().
class RefGuiSynth : public Component, public SynthBase, public SynthGuiInterface {
  public:
    explicit RefGuiSynth(const Options& options) : Component("ref-gui"), SynthGuiInterface(this, false) {
      engine_->setSampleRate(vital::kDefaultSampleRate);
      SynthGuiData data(this);
      gui_ = std::make_unique<CaptureInterface>(&data, options);
    }

    const CriticalSection& getCriticalSection() override { return critical_section_; }
    void pauseProcessing(bool pause) override {
      if (pause)
        critical_section_.enter();
      else
        critical_section_.exit();
    }

  protected:
    SynthGuiInterface* getGuiInterface() override { return this; }

  private:
    CriticalSection critical_section_;
};

class RefGuiApplication : public JUCEApplication {
  public:
    const String getApplicationName() override { return "ref-gui"; }
    const String getApplicationVersion() override { return "1"; }
    bool moreThanOneInstanceAllowed() override { return true; }

    void initialise(const String& command_line) override {
      Options options;
      StringArray args = StringArray::fromTokens(command_line, true);
      for (int i = 0; i + 1 < args.size(); i += 2) {
        std::string key = args[i].unquoted().toStdString();
        std::string value = args[i + 1].unquoted().toStdString();
        if (key == "--out")
          options.out = value;
        else if (key == "--preset")
          options.preset = value;
        else if (key == "--tab")
          options.tab = value;
        else if (key == "--width")
          options.width = std::stoi(value);
        else if (key == "--height")
          options.height = std::stoi(value);
        else if (key == "--frames")
          options.frames = std::stoi(value);
        else if (key == "--layout")
          options.layout = value;
        else if (key == "--effects-scroll")
          options.effects_scroll = std::stoi(value);
        else if (key == "--animate")
          options.animate = std::stoi(value) != 0;
        else
          return fail("unknown option " + key);
      }
      int tab = tabIndex(options.tab);
      if ((options.out.empty() && options.layout.empty()) || tab < 0)
        return fail("usage: ref-gui (--out frame.png | --layout layout.jsonl) [--preset file.vital] "
                    "[--tab voice|effects|matrix|advanced] [--width W] [--height H] [--frames N] "
                    "[--effects-scroll Y] [--animate 0|1]");

      synth_ = std::make_unique<RefGuiSynth>(options);
      if (options.preset.empty()) {
        synth_->loadInitPreset();
      }
      else {
        std::string error;
        File preset = File::getCurrentWorkingDirectory().getChildFile(options.preset);
        if (!synth_->loadFromFile(preset, error))
          return fail("cannot load preset " + options.preset + ": " + error);
      }

      FullInterface* gui = synth_->getGui();
      // The reference engine processes no audio, so animated views would show
      // its idle outputs (a filter response at cutoff 0) instead of the
      // controls. Draw everything at rest, as with animation switched off,
      // unless asked for the modulation overlay.
      gui->animate(options.animate);
      synth_->setOpaque(true);
      synth_->addAndMakeVisible(gui);
      synth_->setSize(options.width, options.height);
      gui->setBounds(0, 0, options.width, options.height);
      synth_->addToDesktop(0);
      synth_->setVisible(true);
      synth_->updateFullGui();

      if (TabSelector* selector = findTabSelector(gui))
        selector->setValue(tab, sendNotificationSync);
      else if (tab != 0)
        return fail("tab selector not found");

      if (options.effects_scroll) {
        EffectsViewport* viewport = findEffectsViewport(gui);
        if (viewport == nullptr)
          return fail("effects viewport not found");
        viewport->setViewPosition(0, options.effects_scroll);
      }

      if (!options.layout.empty()) {
        std::string out;
        dumpLayout(gui, gui, "", out);
        File file = File::getCurrentWorkingDirectory().getChildFile(options.layout);
        if (!file.replaceWithText(out))
          return fail("cannot write " + options.layout);
        if (options.out.empty()) {
          quit();
          return;
        }
      }
    }

    void shutdown() override {
      if (synth_)
        synth_->removeAllChildren();
      synth_.reset();
    }

  private:
    void fail(const std::string& message) {
      std::fprintf(stderr, "ref-gui: %s\n", message.c_str());
      setApplicationReturnValue(2);
      quit();
    }

    std::unique_ptr<RefGuiSynth> synth_;
};

} // namespace

START_JUCE_APPLICATION(RefGuiApplication)
