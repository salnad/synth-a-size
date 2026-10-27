// Module "wavetable_edit": the edits the wavetable editor's timeline makes
// to a creator, then render(). Artifact-only. Ports: port/crates/wavetable
// (creator::edits), which port/crates/gui's wavetable editor calls.
//
// params.creator (creator JSON as a string, {"$wave": ...} recipes expanded,
// see wave_recipes.h) or params.preset "init" is the starting creator.
// params.edits is a list of edits, each as the editor makes it:
//   {"op": "add_source", "type": 0..2}         WavetableComponentList::addSource
//   {"op": "add_modifier", "group", "type": 0..5}         addComponent
//   {"op": "remove_group", "group"}                       removeGroup
//   {"op": "remove_component", "group", "component"}      removeComponent
//   {"op": "reset_component", "group", "component"}       resetComponent
//   {"op": "move_group_up" | "move_group_down", "group"}  moveGroupUp/Down
//   {"op": "move_modifier_up" | "move_modifier_down", "group", "component"}
//   {"op": "create_keyframe", "group", "component", "position"}
//                                     WavetableOrganizer::createKeyframeAtPosition
//   {"op": "delete_keyframe", "group", "component", "keyframe"}  deleteKeyframe
//   {"op": "drag_keyframes", "selected": [[group, component, keyframe], ...],
//    "dragged": [group, component, keyframe], "targets": [position, ...]}
//                                     mouseDrag to each target, then mouseUp
// with the guards the editor's menus and handlers apply. Keyframes are
// named by their indices when the edit starts.
// params.seed (default 0) sets the generator counter before the edits: a
// new Audio File Source draws its random seed from it.
// Texts saved.json (stateToJson().dump()); arrays as wavetable_render
// (render_arrays.h).

#include "module_registry.h"

#include "render_arrays.h"
#include "wave_recipes.h"
#include "wavetable.h"
#include "wavetable_component.h"
#include "wavetable_component_factory.h"
#include "wavetable_creator.h"
#include "wavetable_group.h"
#include "wavetable_keyframe.h"
#include "synth_constants.h"
#include "utils.h"

#include <vector>

namespace {
  WavetableComponent* componentAt(WavetableCreator& creator, int group, int component) {
    if (group < 0 || group >= creator.numGroups())
      return nullptr;
    WavetableGroup* wavetable_group = creator.getGroup(group);
    if (component < 0 || component >= wavetable_group->numComponents())
      return nullptr;
    return wavetable_group->getComponent(component);
  }

  WavetableKeyframe* keyframeAt(WavetableCreator& creator, const nlohmann::json& index) {
    WavetableComponent* component = componentAt(creator, index[0], index[1]);
    int keyframe = index[2];
    if (component == nullptr || keyframe < 0 || keyframe >= component->numFrames())
      return nullptr;
    return component->getFrameAt(keyframe);
  }

  void applyEdit(WavetableCreator& creator, const nlohmann::json& edit) {
    const std::string op = edit["op"];
    int group = edit.value("group", -1);
    int index = edit.value("component", -1);
    if (op == "add_source") {
      WavetableComponentFactory::ComponentType type = WavetableComponentFactory::getSourceType(edit["type"]);
      WavetableComponent* component = WavetableComponentFactory::createComponent(type);
      component->insertNewKeyframe(0);
      WavetableGroup* new_group = new WavetableGroup();
      new_group->addComponent(component);
      creator.addGroup(new_group);
    }
    else if (op == "add_modifier") {
      if (group >= 0 && group < creator.numGroups()) {
        WavetableComponentFactory::ComponentType type = WavetableComponentFactory::getModifierType(edit["type"]);
        WavetableComponent* component = WavetableComponentFactory::createComponent(type);
        component->insertNewKeyframe(0);
        creator.getGroup(group)->addComponent(component);
      }
    }
    else if (op == "remove_group") {
      if (group >= 0 && group < creator.numGroups())
        creator.removeGroup(group);
    }
    else if (op == "remove_component") {
      if (componentAt(creator, group, index))
        creator.getGroup(group)->removeComponent(index);
    }
    else if (op == "reset_component") {
      if (WavetableComponent* component = componentAt(creator, group, index))
        component->reset();
    }
    else if (op == "move_group_up") {
      if (group > 0 && group < creator.numGroups())
        creator.moveUp(group);
    }
    else if (op == "move_group_down") {
      if (group >= 0 && group < creator.numGroups() - 1)
        creator.moveDown(group);
    }
    else if (op == "move_modifier_up") {
      if (index > 0 && componentAt(creator, group, index))
        creator.getGroup(group)->moveUp(index);
    }
    else if (op == "move_modifier_down") {
      if (componentAt(creator, group, index) && index < creator.getGroup(group)->numComponents() - 1)
        creator.getGroup(group)->moveDown(index);
    }
    else if (op == "create_keyframe") {
      if (WavetableComponent* component = componentAt(creator, group, index))
        component->insertNewKeyframe(edit["position"]);
    }
    else if (op == "delete_keyframe") {
      nlohmann::json at = { group, index, edit["keyframe"] };
      if (WavetableKeyframe* keyframe = keyframeAt(creator, at))
        keyframe->owner()->remove(keyframe);
    }
    else if (op == "drag_keyframes") {
      std::vector<WavetableKeyframe*> selected;
      for (const nlohmann::json& at : edit["selected"]) {
        if (WavetableKeyframe* keyframe = keyframeAt(creator, at))
          selected.push_back(keyframe);
      }
      WavetableKeyframe* dragged = keyframeAt(creator, edit["dragged"]);
      if (dragged == nullptr)
        return;
      for (int target : edit["targets"]) {
        int delta = target - dragged->position();
        for (WavetableKeyframe* keyframe : selected) {
          if (!keyframe->owner()->hasKeyframes())
            continue;
          int position = keyframe->position() + delta;
          keyframe->setPosition(vital::utils::iclamp(position, 0, vital::kNumOscillatorWaveFrames - 1));
        }
      }
      for (WavetableKeyframe* keyframe : selected) {
        if (!keyframe->owner()->hasKeyframes())
          continue;
        keyframe->setPosition(vital::utils::iclamp(keyframe->position(), 0, vital::kNumOscillatorWaveFrames - 1));
        keyframe->owner()->reposition(keyframe);
      }
    }
    else
      throw std::runtime_error("unknown edit " + op);
  }
} // namespace

REGISTER_REF_MODULE(refmod::ModuleDef{
  "wavetable_edit",
  "WavetableCreator (params.creator or preset init) with params.edits as the wavetable editor makes them, "
  "then render(); text saved.json, arrays as wavetable_render",
  nullptr,
  {},
  {},
  [](const refmod::ArtifactContext& context, refmod::ArtifactSink& sink, std::string& error) {
    const nlohmann::json params = context.spec.value("params", nlohmann::json::object());
    vital::Wavetable wavetable(vital::kNumOscillatorWaveFrames);
    WavetableCreator creator(&wavetable);
    try {
      if (params.count("creator"))
        creator.jsonToState(wave_recipes::expand(nlohmann::json::parse(params["creator"].get<std::string>())));
      else if (params.value("preset", std::string()) == "init")
        creator.init();
      else {
        error = "params needs creator or preset";
        return false;
      }
      // A new Audio File Source draws its random seed from the generator
      // counter, so the edits start from a known count.
      vital::utils::RandomGenerator::next_seed_ = params.value("seed", 0);
      for (const nlohmann::json& edit : params.value("edits", nlohmann::json::array()))
        applyEdit(creator, edit);
    }
    catch (const std::exception& e) {
      error = std::string("wavetable edit: ") + e.what();
      return false;
    }

    creator.render();
    sink.text("saved.json", creator.stateToJson().dump());
    render_arrays::emit(wavetable, sink);
    return true;
  },
});
