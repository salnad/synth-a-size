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
//           [--effects-scroll 0] [--overlay preset_browser] [--data-dir folder]
//
// --effects-scroll scrolls the effects column down that many pixels (the
// viewport keeps it within the column).
//
// --overlay preset_browser opens the preset browser over the tab, as the
// header's preset selector does.
//
// --data-dir sets upstream's data folder (LoadSave::saveDataDirectory, in
// the config under HOME) before the interface is built, so its browsers
// read the presets there.
//
// ref-gui always runs with HOME in a fresh temporary folder and the XDG
// folders unset (as the other reference tools' isolateUserEnvironment), so
// upstream reads no config, favourites or presets from the machine, and
// --data-dir never writes the user's own config.
//
// --layout writes the bounds of every named component, relative to the
// interface, so the port's layout can be checked exactly. Without --out
// the tool exits after writing it.
//
// [--midi file.mid] [--seconds S] [--block-size 64] run the engine before
// the interface is shown, as ref-render does (render_synth.h): S seconds of
// audio (default: up to the last MIDI event) in blocks of --block-size
// samples, with the MIDI file's events. The interface then gets the engine's
// oscilloscope and audio memory, as the plugin's editor does, so displays
// that read engine outputs show that live state.
//
// [--animate on|off] (default off) draws the animated views (filter
// responses, envelope and LFO positions, meters, the oscilloscope) from the
// engine's outputs, as the plugin's editor does. It needs a render that
// isn't empty (--midi, or --seconds above 0).
//
// Runs on any X display; CI uses Xvfb with Mesa's software OpenGL.

#include "JuceHeader.h"

#include "default_look_and_feel.h"
#include "effects_interface.h"
#include "full_interface.h"
#include "load_save.h"
#include "sound_engine.h"
#include "synth_base.h"
#include "synth_gui_interface.h"
#include "tab_selector.h"

#include "render_synth.h"

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>

namespace {

// A whole-string decimal number; false on anything else.
bool parseNumber(const std::string& text, double& value) {
  char* end = nullptr;
  value = std::strtod(text.c_str(), &end);
  return !text.empty() && end == text.c_str() + text.size() && std::isfinite(value);
}

// harness::isolateUserEnvironment for ref-gui, keeping X's authority file,
// which X finds through HOME when XAUTHORITY isn't set. Returns the new
// HOME, removed at shutdown.
std::string isolateUserEnvironment() {
  const char* home = getenv("HOME");
  if (getenv("XAUTHORITY") == nullptr && home != nullptr) {
    std::string authority = std::string(home) + "/.Xauthority";
    if (File(authority).existsAsFile())
      setenv("XAUTHORITY", authority.c_str(), 1);
  }
  char templ[] = "/tmp/ref-gui-home-XXXXXX";
  const char* dir = mkdtemp(templ);
  if (dir == nullptr)
    return "";
  setenv("HOME", dir, 1);
  unsetenv("XDG_CONFIG_HOME");
  unsetenv("XDG_DATA_HOME");
  unsetenv("XDG_CACHE_HOME");
  return dir;
}

struct Options {
  std::string midi;
  double seconds = -1.0;
  int block_size = 64;
  bool animate = false;
  std::string out;
  std::string preset;
  std::string tab = "voice";
  int width = vital::kDefaultWindowWidth;
  int height = vital::kDefaultWindowHeight;
  int frames = 30;
  int effects_scroll = 0;
  std::string layout;
  std::string overlay;
  std::string data_dir;
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
        // With animation off (see initialise), nothing repaints by itself.
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
class RefGuiSynth : public Component, public harness::RenderLoop<SynthBase>, public SynthGuiInterface {
  public:
    explicit RefGuiSynth(const Options& options) : Component("ref-gui"), SynthGuiInterface(this, false) {
      // As upstream's SynthEditor: components that don't set their own (text
      // editors, scroll bars) draw with this, not JUCE's default.
      setLookAndFeel(DefaultLookAndFeel::instance());
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
      home_ = isolateUserEnvironment();
      Options options;
      StringArray args = StringArray::fromTokens(command_line, true);
      for (int i = 0; i + 1 < args.size(); i += 2) {
        std::string key = args[i].unquoted().toStdString();
        std::string value = args[i + 1].unquoted().toStdString();
        if (key == "--midi")
          options.midi = value;
        else if (key == "--seconds") {
          if (!parseNumber(value, options.seconds) || !(options.seconds >= 0.0))
            return fail("--seconds takes a number >= 0");
        }
        else if (key == "--block-size") {
          double block_size = 0.0;
          if (!parseNumber(value, block_size) || block_size != std::floor(block_size) ||
              block_size < 1.0 || block_size > 65536.0)
            return fail("--block-size takes a whole number from 1 to 65536");
          options.block_size = static_cast<int>(block_size);
        }
        else if (key == "--animate") {
          if (value != "on" && value != "off")
            return fail("--animate takes on or off");
          options.animate = value == "on";
        }
        else if (key == "--out")
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
        else if (key == "--overlay")
          options.overlay = value;
        else if (key == "--data-dir")
          options.data_dir = value;
        else
          return fail("unknown option " + key);
      }
      int tab = tabIndex(options.tab);
      if ((options.out.empty() && options.layout.empty()) || tab < 0)
        return fail("usage: ref-gui (--out frame.png | --layout layout.jsonl) [--preset file.vital] "
                    "[--tab voice|effects|matrix|advanced] [--width W] [--height H] [--frames N] "
                    "[--effects-scroll Y] [--overlay preset_browser] [--data-dir folder]");
      if (!options.overlay.empty() && options.overlay != "preset_browser")
        return fail("unknown overlay " + options.overlay);

      if (!options.data_dir.empty()) {
        File folder = File::getCurrentWorkingDirectory().getChildFile(options.data_dir);
        if (!folder.isDirectory())
          return fail("no data folder " + options.data_dir);
        LoadSave::saveDataDirectory(folder);
      }

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

      bool live = !options.midi.empty() || options.seconds >= 0.0;
      if (options.animate && !live)
        return fail("--animate on needs a running engine (--midi or --seconds)");
      if (live) {
        long long rendered = 0;
        std::string error = renderLive(options, rendered);
        if (!error.empty())
          return fail(error);
        if (options.animate && rendered == 0)
          return fail("--animate on needs a running engine: the render is empty");
      }

      FullInterface* gui = synth_->getGui();
      if (live) {
        gui->setOscilloscopeMemory(synth_->getOscilloscopeMemory());
        gui->setAudioMemory(synth_->getAudioMemory());
      }
      // Animated views read the engine's outputs. Without --midi or --seconds
      // the engine processes no audio, so they would show its idle outputs (a
      // filter response at cutoff 0) instead of the controls. By default
      // everything is drawn at rest, as with animation switched off; --animate
      // on draws the views as the plugin's editor does by default.
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

      if (options.overlay == "preset_browser")
        gui->setPresetBrowserVisibility(true, tab);

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
      if (!home_.empty())
        File(home_).deleteRecursively();
    }

  private:
    // ref-render's block loop (render_main.cpp) at its default sample rate
    // and tempo, without writing the audio. Runs before the interface is on
    // screen, so no frame is drawn while the engine is processing.
    std::string renderLive(const Options& options, long long& rendered) {
      const int sample_rate = vital::kDefaultSampleRate;

      std::vector<harness::TimedMessage> events;
      double last_event_seconds = 0.0;
      if (!options.midi.empty()) {
        std::string error;
        if (!harness::loadMidi(options.midi, sample_rate, events, last_event_seconds, error))
          return error;
      }
      double seconds = options.seconds >= 0.0 ? options.seconds : last_event_seconds;
      const long long total_samples = static_cast<long long>(std::ceil(seconds * sample_rate));

      synth_->prepare(sample_rate, 120.0);
      double seconds_time = 0.0;
      AudioSampleBuffer buffer(vital::kNumChannels, options.block_size);
      size_t event_index = 0;
      for (long long block_start = 0; block_start < total_samples; block_start += options.block_size) {
        int n = static_cast<int>(std::min<long long>(options.block_size, total_samples - block_start));
        buffer.setSize(vital::kNumChannels, n, false, false, true);
        buffer.clear();

        MidiBuffer midi;
        while (event_index < events.size() && events[event_index].sample < block_start + n) {
          long long offset = events[event_index].sample - block_start;
          midi.addEvent(events[event_index].message, static_cast<int>(std::max(0LL, offset)));
          ++event_index;
        }
        synth_->renderBlock(buffer, midi, seconds_time, sample_rate);
      }
      rendered = total_samples;
      return "";
    }

    void fail(const std::string& message) {
      std::fprintf(stderr, "ref-gui: %s\n", message.c_str());
      setApplicationReturnValue(2);
      quit();
    }

    std::unique_ptr<RefGuiSynth> synth_;
    std::string home_;
};

} // namespace

START_JUCE_APPLICATION(RefGuiApplication)
