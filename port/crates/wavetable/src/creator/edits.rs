// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The edits the wavetable editor's timeline makes to a creator: the
//! component list's (`WavetableComponentList::addSource`, `addComponent`,
//! `removeGroup`, `removeComponent`, `resetComponent`, `moveGroupUp`,
//! `moveGroupDown`, `moveModifierUp`, `moveModifierDown`) and the
//! organizer's (`createKeyframeAtPosition`, `deleteKeyframe` and a keyframe
//! drag's `mouseDrag` / `mouseUp`), from
//! `vital/src/interface/wavetable/wavetable_component_list.cpp` and
//! `wavetable_organizer.cpp`. Indices out of range change nothing, as the
//! menus never offer them.

use super::component::{ComponentType, WavetableComponent};
use super::group::WavetableGroup;
use super::WavetableCreator;
use crate::NUM_OSCILLATOR_WAVE_FRAMES;

/// The "Add Source" menu's types (`WavetableComponentFactory::getSourceType`).
pub const SOURCE_TYPES: [ComponentType; 3] = [
    ComponentType::WaveSource,
    ComponentType::LineSource,
    ComponentType::FileSource,
];

/// The "Add Modifier" menu's types (`getModifierType`).
pub const MODIFIER_TYPES: [ComponentType; 6] = [
    ComponentType::PhaseModifier,
    ComponentType::WaveWindow,
    ComponentType::FrequencyFilter,
    ComponentType::SlewLimiter,
    ComponentType::WaveFolder,
    ComponentType::WaveWarp,
];

/// Where a keyframe is: group, component and keyframe index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyframeIndex {
    pub group: usize,
    pub component: usize,
    pub keyframe: usize,
}

/// `addSource`: a new group holding a new source of `source_type` with a
/// keyframe at position 0.
pub fn add_source(creator: &mut WavetableCreator, source_type: ComponentType) {
    let mut component = WavetableComponent::new(source_type);
    component.insert_new_keyframe(0);
    let mut group = WavetableGroup::new();
    group.add_component(component);
    creator.add_group(group);
}

/// `addComponent`: a new modifier of `modifier_type`, with a keyframe at
/// position 0, at the end of group `group`.
pub fn add_modifier(creator: &mut WavetableCreator, group: usize, modifier_type: ComponentType) {
    if let Some(group) = creator.groups.get_mut(group) {
        let mut component = WavetableComponent::new(modifier_type);
        component.insert_new_keyframe(0);
        group.add_component(component);
    }
}

/// `removeGroup`.
pub fn remove_group(creator: &mut WavetableCreator, group: usize) {
    creator.remove_group(group);
}

/// `removeComponent`.
pub fn remove_component(creator: &mut WavetableCreator, group: usize, component: usize) {
    if let Some(group) = creator.groups.get_mut(group) {
        group.remove_component(component);
    }
}

/// `resetComponent`: the group menu's "Reset Source" resets component 0.
pub fn reset_component(creator: &mut WavetableCreator, group: usize, component: usize) {
    if let Some(component) = component_mut(creator, group, component) {
        component.reset();
    }
}

/// `moveGroupUp`.
pub fn move_group_up(creator: &mut WavetableCreator, group: usize) {
    if group > 0 {
        creator.move_up(group);
    }
}

/// `moveGroupDown`.
pub fn move_group_down(creator: &mut WavetableCreator, group: usize) {
    if group + 1 < creator.groups.len() {
        creator.move_down(group);
    }
}

/// `moveModifierUp`.
pub fn move_modifier_up(creator: &mut WavetableCreator, group: usize, component: usize) {
    if component == 0 {
        return;
    }
    if let Some(group) = creator.groups.get_mut(group) {
        group.move_up(component);
    }
}

/// `moveModifierDown`.
pub fn move_modifier_down(creator: &mut WavetableCreator, group: usize, component: usize) {
    if let Some(group) = creator.groups.get_mut(group) {
        if component + 1 < group.components.len() {
            group.move_down(component);
        }
    }
}

/// `createKeyframeAtPosition`: a keyframe at `position` in component
/// `component` of group `group`, interpolated from its neighbours. Returns
/// its index.
pub fn create_keyframe(
    creator: &mut WavetableCreator,
    group: usize,
    component: usize,
    position: i32,
) -> Option<usize> {
    let component = component_mut(creator, group, component)?;
    Some(component.insert_new_keyframe(position))
}

/// `deleteKeyframe`.
pub fn delete_keyframe(creator: &mut WavetableCreator, keyframe: KeyframeIndex) {
    if let Some(component) = component_mut(creator, keyframe.group, keyframe.component) {
        component.remove_keyframe(keyframe.keyframe);
    }
}

/// A keyframe drag's `mouseDrag`: moves every keyframe in `selected` by
/// the dragged keyframe's step to `target`, each clamped to the table,
/// without reordering them (`setPosition`). Components without keyframes
/// stay. `dragged` is the keyframe under the pointer.
pub fn drag_keyframes(
    creator: &mut WavetableCreator,
    selected: &[KeyframeIndex],
    dragged: KeyframeIndex,
    target: i32,
) {
    let Some(dragged_position) = component(creator, dragged.group, dragged.component)
        .and_then(|component| component.keyframe_position(dragged.keyframe))
    else {
        return;
    };
    let delta = target - dragged_position;
    for keyframe in selected {
        let Some(component) = component_mut(creator, keyframe.group, keyframe.component) else {
            continue;
        };
        if !component.has_keyframes() {
            continue;
        }
        let Some(position) = component.keyframe_position(keyframe.keyframe) else {
            continue;
        };
        let moved = (position + delta).clamp(0, NUM_OSCILLATOR_WAVE_FRAMES - 1);
        component.set_keyframe_position(keyframe.keyframe, moved);
    }
}

/// A keyframe drag's `mouseUp`: each keyframe in `selected`, in order,
/// clamped to the table and moved to its place for its position
/// (`reposition`). Returns the selection's new indices.
pub fn finish_keyframe_drag(
    creator: &mut WavetableCreator,
    selected: &[KeyframeIndex],
) -> Vec<KeyframeIndex> {
    let mut selected = selected.to_vec();
    for i in 0..selected.len() {
        let keyframe = selected[i];
        let Some(component) = component_mut(creator, keyframe.group, keyframe.component) else {
            continue;
        };
        if !component.has_keyframes() {
            continue;
        }
        let Some(position) = component.keyframe_position(keyframe.keyframe) else {
            continue;
        };
        let clamped = position.clamp(0, NUM_OSCILLATOR_WAVE_FRAMES - 1);
        component.set_keyframe_position(keyframe.keyframe, clamped);
        let from = keyframe.keyframe;
        let to = component.reposition_keyframe(from);
        for other in selected.iter_mut() {
            if other.group != keyframe.group || other.component != keyframe.component {
                continue;
            }
            if other.keyframe == from {
                other.keyframe = to;
            } else if from < other.keyframe && other.keyframe <= to {
                other.keyframe -= 1;
            } else if to <= other.keyframe && other.keyframe < from {
                other.keyframe += 1;
            }
        }
    }
    selected
}

fn component(
    creator: &WavetableCreator,
    group: usize,
    component: usize,
) -> Option<&WavetableComponent> {
    creator.groups.get(group)?.components.get(component)
}

fn component_mut(
    creator: &mut WavetableCreator,
    group: usize,
    component: usize,
) -> Option<&mut WavetableComponent> {
    creator.groups.get_mut(group)?.components.get_mut(component)
}
