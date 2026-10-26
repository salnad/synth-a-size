// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Voice tab's left column: three oscillators, the sample and the two
//! filters. Ports `SynthesisInterface`
//! (`vital/src/interface/editor_sections/synthesis_interface.cpp`).
//!

use egui::Ui;

use super::filter::FilterSection;
use super::oscillator::{self, OscillatorLayout, OscillatorSection, NUM_OSCILLATORS};
use super::sample::{self, SampleLayout, SampleSection};
use super::{Overlay, Panel};
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::skin::ValueId;

/// `SynthesisInterface::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SynthesisLayout {
    pub bounds: Rect,
    pub oscillators: [Rect; NUM_OSCILLATORS],
    pub sample: Rect,
    pub filter_1: Rect,
    pub filter_2: Rect,
}

impl SynthesisLayout {
    /// The interface at `bounds` (`FullLayout::main`). `top` is the
    /// interface's scope; the oscillators' and sample's sizes come from
    /// their own skin overrides, as upstream asks those sections.
    pub fn new(top: &Scope, bounds: Rect) -> SynthesisLayout {
        let padding = top.padding() as i32;
        let width = bounds.width;
        let height = bounds.height;
        let active_width = width - padding;
        let width_left = (active_width - padding) / 2;
        let width_right = active_width - width_left;
        let right_x = width_left + padding;

        let oscillator_scope = top.within(oscillator::SKIN);
        let oscillator_margin = oscillator_scope.widget_margin() as i32;
        let oscillator_height =
            2 * oscillator_scope.knob_section_height() as i32 - oscillator_margin;
        let oscillators: [Rect; NUM_OSCILLATORS] = std::array::from_fn(|i| {
            Rect::new(
                0,
                i as i32 * (oscillator_height + padding),
                width,
                oscillator_height,
            )
        });

        let sample_y = oscillators[NUM_OSCILLATORS - 1].bottom() + padding;
        let sample_height = top.within(sample::SKIN).knob_section_height() as i32;
        let filter_y = sample_y + sample_height + top.value(ValueId::LargePadding) as i32;
        let filter_height = height - filter_y;

        let (dx, dy) = (bounds.x, bounds.y);
        let at = |r: Rect| r.translated(dx, dy);
        SynthesisLayout {
            bounds,
            oscillators: oscillators.map(at),
            sample: at(Rect::new(0, sample_y, width, sample_height)),
            filter_1: at(Rect::new(0, filter_y, width_left, filter_height)),
            filter_2: at(Rect::new(right_x, filter_y, width_right, filter_height)),
        }
    }
}

/// The Voice tab's synthesis column.
pub struct SynthesisInterface {
    oscillators: [OscillatorSection; NUM_OSCILLATORS],
    sample: SampleSection,
    filters: [FilterSection; 2],
}

impl Default for SynthesisInterface {
    fn default() -> SynthesisInterface {
        SynthesisInterface {
            oscillators: std::array::from_fn(OscillatorSection::new),
            sample: SampleSection::default(),
            filters: [FilterSection::voice(1), FilterSection::voice(2)],
        }
    }
}

impl SynthesisInterface {
    /// The oscillator sections' layouts for the current wave morph types.
    pub fn oscillator_layouts(
        &self,
        host: &Host,
        top: &Scope,
        layout: &SynthesisLayout,
    ) -> [OscillatorLayout; NUM_OSCILLATORS] {
        let scope = top.within(oscillator::SKIN);
        std::array::from_fn(|i| self.oscillators[i].layout(host, &scope, layout.oscillators[i]))
    }

    /// Paints each section's background and shows its controls.
    pub fn show(
        &self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        top: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
    ) {
        let layout = SynthesisLayout::new(top, bounds);
        let scope = top.within(oscillator::SKIN);
        let oscillator_layouts = self.oscillator_layouts(host, top, &layout);
        for (section, section_layout) in self.oscillators.iter().zip(&oscillator_layouts) {
            section.paint_background(pixels, &scope, section_layout);
            let mut panel = Panel::new(ui, host, pixels, scope, overlay, section_layout.bounds);
            section.show(&mut panel, section_layout);
            panel.finish();
        }

        let scope = top.within(sample::SKIN);
        let sample_layout = SampleLayout::new(&scope, layout.sample);
        self.sample.paint_background(pixels, &scope, &sample_layout);
        let mut panel = Panel::new(ui, host, pixels, scope, overlay, layout.sample);
        self.sample.show(&mut panel, &sample_layout);
        panel.finish();

        // Filter 2 is added first upstream, then filter 1.
        for (filter, bounds) in self
            .filters
            .iter()
            .zip([layout.filter_1, layout.filter_2])
            .rev()
        {
            let scope = top.within(filter.skin());
            let filter_layout = filter.layout(&scope, bounds);
            filter.paint_background(pixels, &scope, &filter_layout);
            let mut panel = Panel::new(ui, host, pixels, scope, overlay, bounds);
            filter.show(&mut panel, &filter_layout);
            panel.finish();
        }
    }
}
