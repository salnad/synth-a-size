// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The modulation column: envelopes, LFOs and random LFOs, each a tab
//! selector of sources beside the selected source's section, and the
//! keyboard sources. Ports `ModulationInterface`
//! (`vital/src/interface/editor_sections/modulation_interface.cpp`).
//!
//! A tab selector shows a minimum number of tabs plus one past the last
//! connected source; connections come with drag-to-modulate, so for now
//! only the minimum shows.

use egui::Ui;

use super::envelope::{self, EnvelopeLayout, EnvelopeSection, NUM_ENVELOPES};
use super::lfo::{self, LfoLayout, LfoSection, NUM_LFOS};
use super::random::{self, RandomLayout, RandomSection, NUM_RANDOM_LFOS};
use super::{MenuOwner, Overlay, Panel};
use crate::controls::modulation_button::ModulationTabSelector;
use crate::host::Host;
use crate::layout::Rect;
use crate::modulation::{disconnect_menu, ModulationManager};
use crate::paint::PixelPainter;
use crate::popup::Menu;
use crate::scope::Scope;
use crate::section::{paint_body, paint_border, paint_tab_shadow};
use crate::skin::{ColorId, ValueId};

/// `ModulationInterface::kMin...ModulationsToShow`.
const MIN_ENVELOPES_SHOWN: usize = 3;
const MIN_LFOS_SHOWN: usize = 4;
const MIN_RANDOMS_SHOWN: usize = 2;
const MIN_TOTAL_SHOWN: usize = MIN_ENVELOPES_SHOWN + MIN_LFOS_SHOWN + MIN_RANDOMS_SHOWN;

/// `ModulationInterface::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModulationLayout {
    pub bounds: Rect,
    pub envelope_tabs: Rect,
    pub envelope: Rect,
    pub lfo_tabs: Rect,
    pub lfo: Rect,
    pub random_tabs: Rect,
    pub random: Rect,
    pub keyboard_top: Rect,
    pub keyboard_bottom: Rect,
}

impl ModulationLayout {
    /// The column at `bounds` (`FullLayout::modulation`); `top` is the
    /// interface's scope.
    pub fn new(top: &Scope, bounds: Rect) -> ModulationLayout {
        let padding = top.padding() as i32;
        let (width, height) = (bounds.width, bounds.height);
        let active_height = height - 2 * padding;
        let envelope_height =
            (active_height as f32 * MIN_ENVELOPES_SHOWN as f32 / MIN_TOTAL_SHOWN as f32) as i32;
        let lfo_height =
            (active_height as f32 * MIN_LFOS_SHOWN as f32 / MIN_TOTAL_SHOWN as f32) as i32;
        let mod_width = top.value(ValueId::ModulationButtonWidth) as i32;

        let envelope_tabs = Rect::new(0, 0, mod_width, envelope_height);
        let envelope = Rect::new(mod_width, 0, width - mod_width, envelope_height);
        let lfo_y = envelope.bottom() + padding;
        let lfo_tabs = Rect::new(0, lfo_y, mod_width, lfo_height);
        let lfo = Rect::new(mod_width, lfo_y, width - mod_width, lfo_height);

        let keyboard_width = mod_width * 4;
        let keyboard_x = width - keyboard_width;
        let random_y = lfo.bottom() + padding;
        let random_height = height - random_y;
        let random_tabs = Rect::new(0, random_y, mod_width, random_height);
        let random = Rect::new(
            mod_width,
            random_y,
            keyboard_x - padding - mod_width,
            random_height,
        );
        let keyboard_top_height = random_height / 2;
        let keyboard_top = Rect::new(keyboard_x, random_y, keyboard_width, keyboard_top_height);
        let keyboard_bottom_y = random_y + keyboard_top_height + 1;
        let keyboard_bottom = Rect::new(
            keyboard_x,
            keyboard_bottom_y,
            keyboard_width,
            height - keyboard_bottom_y,
        );

        let at = |r: Rect| r.translated(bounds.x, bounds.y);
        ModulationLayout {
            bounds,
            envelope_tabs: at(envelope_tabs),
            envelope: at(envelope),
            lfo_tabs: at(lfo_tabs),
            lfo: at(lfo),
            random_tabs: at(random_tabs),
            random: at(random),
            keyboard_top: at(keyboard_top),
            keyboard_bottom: at(keyboard_bottom),
        }
    }
}

/// The modulation column and which source of each kind is shown.
pub struct ModulationInterface {
    envelopes: Vec<EnvelopeSection>,
    lfos: Vec<LfoSection>,
    randoms: Vec<RandomSection>,
    envelope_tabs: ModulationTabSelector,
    lfo_tabs: ModulationTabSelector,
    random_tabs: ModulationTabSelector,
    keyboard_top: ModulationTabSelector,
    keyboard_bottom: ModulationTabSelector,
    /// The selected envelope, LFO and random LFO.
    pub selected: [usize; 3],
}

/// A selectable tab column of sources, joined to the section on its
/// right.
fn tabs(names: &[&'static str], min_shown: usize) -> ModulationTabSelector {
    let mut selector = ModulationTabSelector::numbered(names);
    selector.selections = true;
    selector.min_shown = min_shown;
    selector.connect_right = true;
    selector.draw_borders = true;
    selector
}

/// A row of keyboard sources.
fn keyboard_row(
    names: &[&'static str],
    overrides: &[(usize, &'static str)],
) -> ModulationTabSelector {
    let mut selector = ModulationTabSelector::numbered(names);
    selector.vertical = false;
    selector.draw_borders = true;
    for &(i, text) in overrides {
        selector.buttons[i].text_override = Some(text);
    }
    selector
}

impl Default for ModulationInterface {
    fn default() -> ModulationInterface {
        ModulationInterface {
            envelopes: (0..NUM_ENVELOPES).map(EnvelopeSection::new).collect(),
            lfos: (0..NUM_LFOS).map(LfoSection::new).collect(),
            randoms: (0..NUM_RANDOM_LFOS).map(RandomSection::new).collect(),
            envelope_tabs: tabs(
                &["env_1", "env_2", "env_3", "env_4", "env_5", "env_6"],
                MIN_ENVELOPES_SHOWN,
            ),
            lfo_tabs: tabs(
                &[
                    "lfo_1", "lfo_2", "lfo_3", "lfo_4", "lfo_5", "lfo_6", "lfo_7", "lfo_8",
                ],
                MIN_LFOS_SHOWN,
            ),
            random_tabs: tabs(
                &["random_1", "random_2", "random_3", "random_4"],
                MIN_RANDOMS_SHOWN,
            ),
            keyboard_top: keyboard_row(
                &["note", "velocity", "lift", "note_in_octave"],
                &[(3, "OCT NOTE")],
            ),
            keyboard_bottom: keyboard_row(
                &["aftertouch", "slide", "stereo", "random"],
                &[(0, "PRESSURE")],
            ),
            selected: [0; 3],
        }
    }
}

impl ModulationInterface {
    /// The LFO that asked for the popup browser since the last call, with
    /// the browser's bounds and its section's.
    pub fn take_lfo_browse_request(&self) -> Option<(usize, Rect, Rect)> {
        self.lfos.iter().enumerate().find_map(|(index, lfo)| {
            lfo.take_browse_request()
                .map(|(browser, section)| (index, browser, section))
        })
    }

    /// LFO `index`'s current file (`getCurrentFile`).
    pub fn lfo_file(&self, index: usize) -> Option<std::path::PathBuf> {
        self.lfos.get(index)?.current_file()
    }

    /// Loads `file` into LFO `index` (`LfoSection::loadFile`).
    pub fn load_lfo_file(
        &self,
        model: &dyn crate::interface::EditorModel,
        index: usize,
        file: &std::path::Path,
    ) {
        if let Some(lfo) = self.lfos.get(index) {
            lfo.load_file(model, file);
        }
    }

    /// Each selector with the number of its tabs shown, which depends on the
    /// sources with a modulation.
    fn selectors(&self, modulation: &ModulationManager) -> [(&ModulationTabSelector, usize); 5] {
        let shown = |s: &ModulationTabSelector| s.num_shown(|name| modulation.is_connected(name));
        [
            (&self.envelope_tabs, shown(&self.envelope_tabs)),
            (&self.lfo_tabs, shown(&self.lfo_tabs)),
            (&self.random_tabs, shown(&self.random_tabs)),
            (&self.keyboard_top, shown(&self.keyboard_top)),
            (&self.keyboard_bottom, shown(&self.keyboard_bottom)),
        ]
    }

    /// `ModulationInterface::paintBackground` and its children's.
    fn paint_background(
        &self,
        pixels: &PixelPainter,
        top: &Scope,
        layout: &ModulationLayout,
        modulation: &ModulationManager,
    ) {
        pixels.fill_rect(layout.bounds, top.color(ColorId::Background));
        for bounds in [
            layout.envelope_tabs,
            layout.envelope,
            layout.lfo_tabs,
            layout.lfo,
            layout.random_tabs,
            layout.random,
            layout.keyboard_top,
            layout.keyboard_bottom,
        ] {
            paint_tab_shadow(pixels, top, bounds);
        }

        let mod_x = layout.lfo_tabs.x;
        let span = |tabs: Rect, section: Rect| {
            Rect::new(mod_x, tabs.y, section.right() - mod_x, tabs.height)
        };
        let lfo_bounds = span(layout.lfo_tabs, layout.lfo);
        let envelope_bounds = span(layout.envelope_tabs, layout.envelope);
        let random_bounds = span(layout.random_tabs, layout.random);
        for bounds in [lfo_bounds, envelope_bounds, random_bounds] {
            paint_body(pixels, top, bounds);
        }

        // paintChildrenBackgrounds, each clipped to its child.
        let [env_tabs, lfo_tabs, random_tabs, top_row, bottom_row] = self.selectors(modulation);
        let [envelope, lfo, random] = self.selected;
        let tab_bounds = [layout.envelope_tabs, layout.lfo_tabs, layout.random_tabs];
        let paint_tabs =
            |(selector, shown): (&ModulationTabSelector, usize), bounds: Rect, selected| {
                pixels.with_clip(bounds, |pixels| {
                    selector.paint_background(pixels, top, bounds, shown, selected)
                });
            };
        let scope = top.within(envelope::SKIN);
        pixels.with_clip(layout.envelope, |pixels| {
            let section_layout = EnvelopeLayout::new(&scope, layout.envelope);
            self.envelopes[envelope].paint_background(pixels, &scope, &section_layout);
        });
        paint_tabs(env_tabs, tab_bounds[0], Some(envelope));
        let scope = top.within(lfo::SKIN);
        pixels.with_clip(layout.lfo, |pixels| {
            let section_layout = LfoLayout::new(&scope, layout.lfo);
            self.lfos[lfo].paint_background(pixels, &scope, &section_layout);
        });
        paint_tabs(lfo_tabs, tab_bounds[1], Some(lfo));
        let scope = top.within(random::SKIN);
        pixels.with_clip(layout.random, |pixels| {
            let section_layout = RandomLayout::new(&scope, layout.random);
            self.randoms[random].paint_background(pixels, &scope, &section_layout);
        });
        paint_tabs(random_tabs, tab_bounds[2], Some(random));
        paint_tabs(top_row, layout.keyboard_top, None);
        paint_tabs(bottom_row, layout.keyboard_bottom, None);

        let tabs_right = layout.lfo_tabs.right();
        let clip = Rect::new(
            tabs_right,
            layout.bounds.y,
            layout.bounds.right() - tabs_right,
            layout.bounds.height,
        );
        pixels.with_clip(clip, |pixels| {
            for bounds in [lfo_bounds, envelope_bounds, random_bounds] {
                paint_border(pixels, top, bounds);
            }
        });
    }

    /// Paints the column and shows the selected sources' controls. A click
    /// on a tab shows that source (`modulationSelected`).
    pub fn show(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
    ) {
        let layout = ModulationLayout::new(top, bounds);
        let selectors = self.selectors(&overlay.modulation);
        let all_bounds = [
            layout.envelope_tabs,
            layout.lfo_tabs,
            layout.random_tabs,
            layout.keyboard_top,
            layout.keyboard_bottom,
        ];
        let mut clicked = [None; 3];
        for (kind, ((selector, shown), bounds)) in selectors.iter().zip(all_bounds).enumerate() {
            let id = ui.scope_id().with(("modulation tabs", kind));
            let margin = top.widget_margin() as i32;
            let input = selector.interact(
                ui,
                pixels,
                &mut overlay.modulation,
                id,
                bounds,
                *shown,
                margin,
            );
            if kind < 3 {
                clicked[kind] = input.clicked;
            }
            if let Some((source, position)) = input.menu {
                let items = disconnect_menu(overlay.modulation.routes(), source);
                overlay.menu = Some(Menu::new(
                    items,
                    position,
                    MenuOwner::ModulationSource(source),
                ));
            }
        }
        for (selected, click) in self.selected.iter_mut().zip(clicked) {
            if let Some(index) = click {
                *selected = index;
            }
        }

        self.paint_background(pixels, top, &layout, &overlay.modulation);

        let [envelope, lfo, random] = self.selected;
        let scope = top.within(envelope::SKIN);
        let section_layout = EnvelopeLayout::new(&scope, layout.envelope);
        let mut panel = Panel::new(ui, host, pixels, scope, overlay, layout.envelope);
        self.envelopes[envelope].show(&mut panel, &section_layout);
        panel.finish();

        let scope = top.within(lfo::SKIN);
        let section_layout = LfoLayout::new(&scope, layout.lfo);
        let mut panel = Panel::new(ui, host, pixels, scope, overlay, layout.lfo);
        self.lfos[lfo].show(&mut panel, &section_layout);
        panel.finish();

        let scope = top.within(random::SKIN);
        let section_layout = RandomLayout::new(&scope, layout.random);
        let mut panel = Panel::new(ui, host, pixels, scope, overlay, layout.random);
        self.randoms[random].show(&mut panel, &section_layout);
        panel.finish();
    }

    /// The tab selectors, for tests: envelopes, LFOs, randoms, then the
    /// two keyboard rows.
    pub fn tab_selector(&self, which: usize) -> &ModulationTabSelector {
        [
            &self.envelope_tabs,
            &self.lfo_tabs,
            &self.random_tabs,
            &self.keyboard_top,
            &self.keyboard_bottom,
        ][which]
    }
}
