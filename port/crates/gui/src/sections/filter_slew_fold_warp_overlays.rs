// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The wavetable editor's Frequency Filter, Slew Limiter, Wave Folder and
//! Wave Warp overlays
//! (`vital/src/interface/wavetable/overlays/frequency_filter_overlay.cpp`,
//! `slew_limiter_overlay.cpp`, `wave_fold_overlay.cpp`,
//! `wave_warp_overlay.cpp`), shown while a keyframe of their modifier is
//! selected. Each is a controls background with text sliders, a style
//! selector and toggles along the edit strip; the Frequency Filter's covers
//! the amplitude display, the others cover nothing. Each draws twice around
//! the editor's displays, as the Wave Source overlay does (see
//! `wave_source_overlay`).
//!
//! The sliders aren't synth parameters: they are local controls set from
//! the keyframe when it is selected, named outside upstream's parameters,
//! so they show JUCE's own slider text (seven decimals). A slider change
//! writes the keyframe and auditions it, committing once no button is held
//! (`sliderDragEnded`); the filter style and the toggles commit at once.

use sas_engine::sas_oscillators::sas_wavetable::creator::component::WavetableComponent;

use super::wave_source_overlay::{edit_strip, paint_controls_background, title_bounds, FrameEdit};
use super::wavetable_edit::{KeyframeRef, WavetableEditLayout};
use super::Panel;
use crate::controls::button::just_text_toggle;
use crate::controls::slider::{Slider, SliderStyle};
use crate::host::Host;
use crate::layout::Rect;
use crate::local::{SliderModifierControls, FILTER_STYLES};
use crate::scope::Scope;

/// `WavetableComponentOverlay::kTitleHeightRatio`.
const TITLE_HEIGHT_RATIO: f32 = 0.4;
/// `style_->setTextHeightPercentage(0.4f)`.
const SELECTOR_TEXT_HEIGHT: f32 = 0.4;
/// JUCE's `numDecimalPlaces` for a slider range with no interval.
const UNASSIGNED_DECIMAL_PLACES: usize = 7;

/// The four overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliderModifier {
    /// `FrequencyFilterOverlay`.
    Filter,
    /// `SlewLimiterOverlay`.
    Slew,
    /// `WaveFoldOverlay`.
    Fold,
    /// `WaveWarpOverlay`.
    Warp,
}

impl SliderModifier {
    pub const ALL: [SliderModifier; 4] = [
        SliderModifier::Filter,
        SliderModifier::Slew,
        SliderModifier::Fold,
        SliderModifier::Warp,
    ];

    fn index(self) -> usize {
        self as usize
    }

    /// The controls background's titles (`addTitle`).
    fn titles(self) -> &'static [&'static str] {
        match self {
            SliderModifier::Filter => &["STYLE", "CUTOFF", "SHAPE"],
            // Upstream's order: the up limit sits under "DOWN LIMIT".
            SliderModifier::Slew => &["DOWN LIMIT", "UP LIMIT"],
            SliderModifier::Fold => &["MULTIPLY"],
            SliderModifier::Warp => &["", "X WARP", "", "Y WARP"],
        }
    }

    /// The toggles' texts (the buttons' names).
    fn toggle_texts(self) -> &'static [&'static str] {
        match self {
            SliderModifier::Filter => &["NORMALIZE"],
            SliderModifier::Warp => &["X Asymmetric", "Y Asymmetric"],
            _ => &[],
        }
    }

    /// The sliders' local controls, in layout order, with each one's
    /// double-click value (`setDoubleClickReturnValue`) and whether it's the
    /// style selector.
    fn sliders(self, controls: SliderModifierControls) -> Vec<(&'static str, f32, bool)> {
        match self {
            SliderModifier::Filter => vec![
                (controls.filter_style, 0.0, true),
                (controls.filter_cutoff, 4.0, false),
                (controls.filter_shape, 0.5, false),
            ],
            SliderModifier::Slew => vec![
                (controls.slew_up, 0.0, false),
                (controls.slew_down, 0.0, false),
            ],
            SliderModifier::Fold => vec![(controls.fold_amount, 1.0, false)],
            SliderModifier::Warp => vec![
                (controls.warp_horizontal, 0.0, false),
                (controls.warp_vertical, 0.0, false),
            ],
        }
    }
}

/// One overlay's `setEditBounds`, in interface coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct SliderModifierLayout {
    /// `controls_background_`.
    pub controls: Rect,
    /// The background's divider lines, as x in it (`addLine`).
    pub lines: Vec<i32>,
    /// The title texts' bounds, one per title.
    pub titles: Vec<Rect>,
    /// The sliders, in [`SliderModifier::sliders`] order.
    pub sliders: Vec<Rect>,
    /// The toggles, in [`SliderModifier::toggle_texts`] order.
    pub toggles: Vec<Rect>,
}

impl SliderModifierLayout {
    pub fn new(
        kind: SliderModifier,
        scope: &Scope,
        edit: &WavetableEditLayout,
    ) -> SliderModifierLayout {
        let (strip, padding) = edit_strip(scope, edit);
        let height = strip.height;
        let times = |ratio: f32| (height as f32 * ratio) as i32;
        let title_height = (TITLE_HEIGHT_RATIO * height as f32) as i32;
        let y = strip.y;
        let y_title = y + title_height;
        let height_title = height - title_height;
        // WavetableComponentOverlay::setEditBounds centres the background.
        let controls =
            |total: i32| Rect::new(strip.x + (strip.width - total) / 2, y, total, height);
        let (controls, lines, sliders, toggles) = match kind {
            SliderModifier::Filter => {
                let section = times(4.0);
                let total = 4 * section + 3 * padding;
                let background = controls(total);
                let x = background.x;
                let style = Rect::new(x, y_title, section, height_title);
                let cutoff = Rect::new(style.right() + padding, y_title, section, height_title);
                let shape = Rect::new(cutoff.right() + padding, y_title, section, height_title);
                let normalize_padding = height / 6;
                // The toggle starts at the shape's right, without padding.
                let normalize = Rect::new(
                    shape.right(),
                    y + normalize_padding,
                    section,
                    height - 2 * normalize_padding,
                );
                let lines = vec![section, 2 * section + padding, 3 * section + 2 * padding];
                (
                    background,
                    lines,
                    vec![style, cutoff, shape],
                    vec![normalize],
                )
            }
            SliderModifier::Slew => {
                let limit = times(4.0);
                let total = 2 * limit + padding;
                let background = controls(total);
                let up = Rect::new(background.x, y_title, limit, height_title);
                let down = Rect::new(up.right() + padding, y_title, limit, height_title);
                (background, vec![limit], vec![up, down], Vec::new())
            }
            SliderModifier::Fold => {
                // A float width: the background takes it truncated and
                // centres with integer division; the slider's x is centred
                // in floats, then truncated.
                let width = height as f32 * 4.0;
                let background = controls(width as i32);
                let x = (strip.x as f32 + (strip.width as f32 - width) / 2.0) as i32;
                let slider = Rect::new(x, y_title, width as i32, height_title);
                (background, Vec::new(), vec![slider], Vec::new())
            }
            SliderModifier::Warp => {
                let symmetry = times(3.5);
                let warp = times(5.0);
                let warp_padding = times(0.5);
                let total = 2 * symmetry + 2 * warp + 3 * padding;
                let background = controls(total);
                let symmetry_padding = (height as f32 / 6.0) as i32;
                let symmetry_height = height - 2 * symmetry_padding;
                let horizontal_asymmetric = Rect::new(
                    background.x,
                    y + symmetry_padding,
                    symmetry,
                    symmetry_height,
                );
                let horizontal = Rect::new(
                    horizontal_asymmetric.right() + padding + warp_padding,
                    y_title,
                    warp - 2 * warp_padding,
                    height_title,
                );
                let vertical_asymmetric = Rect::new(
                    horizontal.right() + padding + warp_padding,
                    y + symmetry_padding,
                    symmetry,
                    symmetry_height,
                );
                let vertical = Rect::new(
                    vertical_asymmetric.right() + padding + warp_padding,
                    y_title,
                    warp - 2 * warp_padding,
                    height_title,
                );
                let lines = vec![
                    symmetry,
                    symmetry + warp + padding,
                    2 * symmetry + warp + 2 * padding,
                    2 * symmetry + 2 * warp + 3 * padding,
                ];
                (
                    background,
                    lines,
                    vec![horizontal, vertical],
                    vec![horizontal_asymmetric, vertical_asymmetric],
                )
            }
        };
        let mut titles = title_bounds(controls, &lines, title_height);
        titles.truncate(kind.titles().len());
        SliderModifierLayout {
            controls,
            lines,
            titles,
            sliders,
            toggles,
        }
    }
}

/// The modifier's keyframe values for the sliders and its settings for the
/// toggles (`frameSelected`), or `None` when `component` isn't `kind`'s or
/// has no keyframe `index`.
fn values(
    kind: SliderModifier,
    component: &WavetableComponent,
    index: usize,
) -> Option<(Vec<f32>, Vec<bool>)> {
    match (kind, component) {
        (SliderModifier::Filter, WavetableComponent::FrequencyFilter(modifier)) => {
            let data = &modifier.keyframes.keyframes.get(index)?.data;
            let settings = &modifier.settings;
            Some((
                vec![settings.style as f32, data.cutoff, data.shape],
                vec![settings.normalize],
            ))
        }
        (SliderModifier::Slew, WavetableComponent::SlewLimiter(modifier)) => {
            let data = &modifier.keyframes.keyframes.get(index)?.data;
            Some((
                vec![data.slew_up_run_rise, data.slew_down_run_rise],
                Vec::new(),
            ))
        }
        (SliderModifier::Fold, WavetableComponent::WaveFolder(modifier)) => {
            let data = &modifier.keyframes.keyframes.get(index)?.data;
            Some((vec![data.wave_fold_boost], Vec::new()))
        }
        (SliderModifier::Warp, WavetableComponent::WaveWarp(modifier)) => {
            let data = &modifier.keyframes.keyframes.get(index)?.data;
            let settings = &modifier.settings;
            Some((
                vec![data.horizontal_power, data.vertical_power],
                vec![settings.horizontal_asymmetric, settings.vertical_asymmetric],
            ))
        }
        _ => None,
    }
}

/// `sliderValueChanged`: slider `slider` set to `value` on keyframe `index`
/// (the filter style on the modifier).
fn set_slider(component: &mut WavetableComponent, index: usize, slider: usize, value: f32) {
    match component {
        WavetableComponent::FrequencyFilter(modifier) => {
            if slider == 0 {
                modifier.settings.style = value as i32;
            } else if let Some(keyframe) = modifier.keyframes.keyframes.get_mut(index) {
                match slider {
                    1 => keyframe.data.cutoff = value,
                    _ => keyframe.data.shape = value,
                }
            }
        }
        WavetableComponent::SlewLimiter(modifier) => {
            if let Some(keyframe) = modifier.keyframes.keyframes.get_mut(index) {
                match slider {
                    0 => keyframe.data.slew_up_run_rise = value,
                    _ => keyframe.data.slew_down_run_rise = value,
                }
            }
        }
        WavetableComponent::WaveFolder(modifier) => {
            if let Some(keyframe) = modifier.keyframes.keyframes.get_mut(index) {
                keyframe.data.wave_fold_boost = value;
            }
        }
        WavetableComponent::WaveWarp(modifier) => {
            if let Some(keyframe) = modifier.keyframes.keyframes.get_mut(index) {
                match slider {
                    0 => keyframe.data.horizontal_power = value,
                    _ => keyframe.data.vertical_power = value,
                }
            }
        }
        _ => {}
    }
}

/// `buttonClicked`: toggle `toggle` set to `on` on the modifier.
fn set_toggle(component: &mut WavetableComponent, toggle: usize, on: bool) {
    match component {
        WavetableComponent::FrequencyFilter(modifier) => modifier.settings.normalize = on,
        WavetableComponent::WaveWarp(modifier) => match toggle {
            0 => modifier.settings.horizontal_asymmetric = on,
            _ => modifier.settings.vertical_asymmetric = on,
        },
        _ => {}
    }
}

/// One overlay's sliders and state between frames.
struct OverlayState {
    sliders: Vec<Slider>,
    /// Which slider is the style selector, whose change commits at once.
    selector: Option<usize>,
    /// The sliders' values as last applied.
    seen: Vec<f32>,
    /// The toggles' states (`getToggleState`).
    toggles: Vec<bool>,
    /// A slider changed since the last commit (`sliderDragEnded` to come).
    pending: bool,
    /// The selection and keyframe the controls were set from.
    loaded: Option<(u64, KeyframeRef)>,
}

impl OverlayState {
    fn new(kind: SliderModifier, controls: SliderModifierControls) -> OverlayState {
        let mut selector = None;
        let sliders: Vec<Slider> = kind
            .sliders(controls)
            .into_iter()
            .enumerate()
            .map(|(i, (name, double_click, is_selector))| {
                if is_selector {
                    selector = Some(i);
                    let mut slider = Slider::selector(name, Some(&FILTER_STYLES));
                    slider.format.string_lookup = Some(&FILTER_STYLES);
                    slider.text_height_percentage = SELECTOR_TEXT_HEIGHT;
                    slider
                } else {
                    let mut slider = Slider::new(name, SliderStyle::Text);
                    slider.format.unassigned_decimal_places = Some(UNASSIGNED_DECIMAL_PLACES);
                    slider.double_click_value = Some(double_click);
                    slider
                }
            })
            .collect();
        OverlayState {
            seen: vec![0.0; sliders.len()],
            sliders,
            selector,
            toggles: vec![false; kind.toggle_texts().len()],
            pending: false,
            loaded: None,
        }
    }
}

/// One oscillator's four overlays.
pub struct SliderModifierOverlays {
    states: Vec<OverlayState>,
}

impl SliderModifierOverlays {
    pub fn new(controls: SliderModifierControls) -> SliderModifierOverlays {
        SliderModifierOverlays {
            states: SliderModifier::ALL
                .iter()
                .map(|&kind| OverlayState::new(kind, controls))
                .collect(),
        }
    }

    /// `frameSelected`: the controls from keyframe `keyframe` of
    /// `component`, when the selection changed since they last were set
    /// (`dontSendNotification`: nothing is edited).
    fn sync(
        &mut self,
        kind: SliderModifier,
        host: &Host,
        selection: u64,
        keyframe: KeyframeRef,
        component: &WavetableComponent,
    ) {
        let state = &mut self.states[kind.index()];
        if state.loaded == Some((selection, keyframe)) {
            return;
        }
        state.loaded = Some((selection, keyframe));
        let Some((sliders, toggles)) = values(kind, component, keyframe.keyframe) else {
            return;
        };
        for (slider, value) in state.sliders.iter().zip(sliders) {
            host.set_once(slider.name, value);
        }
        state.seen = state
            .sliders
            .iter()
            .map(|slider| host.value(slider.name))
            .collect();
        state.toggles = toggles;
        state.pending = false;
    }

    /// Overlay `kind`'s input for keyframe `keyframe` of `component`,
    /// selected as selection `selection`: the controls load when the
    /// selection changed, then the sliders' changes and the toggles edit the
    /// modifier. `notifyChanged` becomes the returned edit.
    pub fn input(
        &mut self,
        kind: SliderModifier,
        panel: &mut Panel,
        layout: &SliderModifierLayout,
        selection: u64,
        keyframe: KeyframeRef,
        component: &mut WavetableComponent,
    ) -> Option<FrameEdit> {
        self.sync(kind, panel.host, selection, keyframe, component);
        let state = &mut self.states[kind.index()];
        let mut result = None;
        let mut note = |edit: FrameEdit| {
            result = Some(match result {
                Some(FrameEdit::Done) => FrameEdit::Done,
                _ => edit,
            });
        };
        for i in 0..state.sliders.len() {
            let value = panel.host.value(state.sliders[i].name);
            if value == state.seen[i] {
                continue;
            }
            state.seen[i] = value;
            set_slider(component, keyframe.keyframe, i, value);
            if state.selector == Some(i) {
                note(FrameEdit::Done);
            } else {
                state.pending = true;
                note(FrameEdit::Changed);
            }
        }
        if state.pending && !panel.ui.input(|input| input.pointer.any_down()) {
            // sliderDragEnded.
            state.pending = false;
            note(FrameEdit::Done);
        }
        for (i, (&text, &bounds)) in kind.toggle_texts().iter().zip(&layout.toggles).enumerate() {
            let id = panel
                .ui
                .scope_id()
                .with(("modifier toggle", kind.index(), i));
            if just_text_toggle(
                panel.ui,
                panel.pixels,
                &mut panel.layer,
                &panel.scope,
                id,
                text,
                state.toggles[i],
                bounds,
            ) {
                // The toggle flips (clickingTogglesState), and the modifier
                // takes it.
                state.toggles[i] = !state.toggles[i];
                set_toggle(component, i, state.toggles[i]);
                note(FrameEdit::Done);
            }
        }
        result
    }

    /// Overlay `kind`'s drawing: the background with its titles and the
    /// sliders, handled when `handle_sliders` (the second drawing).
    pub fn paint(
        &self,
        kind: SliderModifier,
        panel: &mut Panel,
        layout: &SliderModifierLayout,
        handle_sliders: bool,
    ) {
        let titles: Vec<(&str, Rect)> = kind
            .titles()
            .iter()
            .copied()
            .zip(layout.titles.iter().copied())
            .collect();
        paint_controls_background(panel, layout.controls, &layout.lines, &titles);
        let state = &self.states[kind.index()];
        for (slider, &bounds) in state.sliders.iter().zip(&layout.sliders) {
            if handle_sliders {
                panel.slider(slider, bounds);
            } else {
                let value = slider.host_value(panel.host);
                slider.paint(
                    panel.pixels,
                    &mut panel.layer,
                    &panel.scope,
                    bounds,
                    value,
                    false,
                );
            }
        }
    }
}
