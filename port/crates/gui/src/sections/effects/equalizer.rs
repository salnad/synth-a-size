// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The EQ section. Ports `EqualizerSection`
//! (`vital/src/interface/editor_sections/equalizer_section.cpp`).

use std::sync::OnceLock;

use egui::Ui;
use sas_simd::utils;

use super::equalizer_response::{EqBand, EqualizerResponse, DEFAULT_DB_BUFFER_RATIO};
use super::viewers::Live;
use super::{divided_area_buffered, divided_area_unbuffered, paint_section_background};
use super::{viewer_corners, with_left, Effect};
use crate::controls::path::Path;
use crate::controls::paths;
use crate::controls::shape_button::{Shape, ShapeButton, ShapeColours};
use crate::controls::slider::{Slider, SliderStyle};
use crate::controls::spectrogram::{Spectrogram, SpectrogramAnalysis};
use crate::controls::tab_selector::TabSelector;
use crate::host::Host;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::section::{draw_label, place_knobs_in_area};
use crate::sections::{Overlay, Panel};
use crate::skin::{ColorId, ValueId};

/// `EqualizerSection::resized`, in interface pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EqualizerLayout {
    pub bounds: Rect,
    pub on: Rect,
    /// The response display (and the spectrogram under it).
    pub response: Rect,
    pub low_mode: Rect,
    pub band_mode: Rect,
    pub high_mode: Rect,
    /// The selected band's knobs (each band's sit in the same places).
    pub gain: Rect,
    pub cutoff: Rect,
    pub resonance: Rect,
    pub selected_band: Rect,
}

impl EqualizerLayout {
    pub fn new(scope: &Scope, bounds: Rect) -> EqualizerLayout {
        let widget_margin = scope.widget_margin() as i32;
        let title = crate::section::title_width(scope) as i32;
        let (width, height) = (bounds.width, bounds.height);
        let area = with_left(Rect::new(0, 0, width, height), title);
        let widget_bounds = divided_area_unbuffered(area, 2, 1, widget_margin);
        let response = Rect::new(
            widget_bounds.x,
            widget_bounds.y + widget_margin,
            widget_bounds.width,
            widget_bounds.height - 2 * widget_margin,
        );

        let knobs_area = divided_area_buffered(area, 2, 0, widget_margin);
        let button_height = scope.value(ValueId::TextComponentHeight) as i32;
        let section_height = scope.knob_section_height() as i32;
        let knob_y = height - section_height;
        let button_y = widget_margin;
        let knobs = place_knobs_in_area(
            scope,
            Rect::new(knobs_area.x, knob_y, knobs_area.width, section_height),
            3,
        );
        let (gain, cutoff, resonance) = (knobs[0], knobs[1], knobs[2]);
        let low_mode = Rect::new(gain.x, button_y, gain.width, button_height);
        let band_mode = Rect::new(cutoff.x, button_y, cutoff.width, button_height);
        let high_mode = Rect::new(resonance.x, button_y, resonance.width, button_height);
        let selected_y = low_mode.bottom() + widget_margin;
        let selected_height = knob_y - selected_y + widget_margin;
        let selected_band = Rect::new(
            title + widget_margin,
            selected_y,
            knobs_area.width - 2 * widget_margin,
            selected_height,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        EqualizerLayout {
            bounds,
            on: Rect::new(dx, dy, title, title),
            response: response.translated(dx, dy),
            low_mode: low_mode.translated(dx, dy),
            band_mode: band_mode.translated(dx, dy),
            high_mode: high_mode.translated(dx, dy),
            gain: gain.translated(dx, dy),
            cutoff: cutoff.translated(dx, dy),
            resonance: resonance.translated(dx, dy),
            selected_band: selected_band.translated(dx, dy),
        }
    }
}

/// The anonymous `bufferPath`: the path padded 0.3 above and below, so
/// its icon draws smaller.
fn buffer_path(path: &Path) -> Path {
    const BUFFER: f32 = 0.3;
    let mut result = path.clone();
    if let Some((left, top, _, bottom)) = path.bounds() {
        result.add_line_segment((left, top - BUFFER), (left, top - BUFFER), 0.1);
        result.add_line_segment((left, bottom + BUFFER), (left, bottom + BUFFER), 0.1);
    }
    result
}

/// The mode buttons' icons: `Paths::highPass`, `notch` and `lowPass`,
/// buffered.
fn mode_icons() -> &'static [Path; 3] {
    static ICONS: OnceLock<[Path; 3]> = OnceLock::new();
    ICONS.get_or_init(|| {
        [
            buffer_path(paths::high_pass()),
            buffer_path(paths::notch()),
            buffer_path(paths::low_pass()),
        ]
    })
}

/// Each band's controls: low, band, high.
const BANDS: [EqBand; 3] = [
    EqBand {
        cutoff: "eq_low_cutoff",
        resonance: Some("eq_low_resonance"),
        gain: "eq_low_gain",
    },
    EqBand {
        cutoff: "eq_band_cutoff",
        resonance: Some("eq_band_resonance"),
        gain: "eq_band_gain",
    },
    EqBand {
        cutoff: "eq_high_cutoff",
        resonance: Some("eq_high_resonance"),
        gain: "eq_high_gain",
    },
];

/// The mode controls: a high pass for the low band, a notch for the
/// middle, a low pass for the high.
const MODES: [&str; 3] = ["eq_low_mode", "eq_band_mode", "eq_high_mode"];

/// The section's controls (`EqualizerSection`'s constructor), the band its
/// knobs show (`selected_band_`, which isn't a parameter), and the band the
/// response draws selected (none after a press away from its handles).
pub struct EqualizerSection {
    cutoffs: [Slider; 3],
    resonances: [Slider; 3],
    gains: [Slider; 3],
    selected_band: usize,
    response_band: Option<usize>,
    /// The spectrogram of the EQ's output under the response.
    spectrogram: SpectrogramAnalysis,
}

impl Default for EqualizerSection {
    fn default() -> EqualizerSection {
        EqualizerSection {
            cutoffs: BANDS.map(|band| Slider::new(band.cutoff, SliderStyle::Rotary).hz_alternate()),
            resonances: BANDS.map(|band| {
                Slider::new(
                    band.resonance.expect("each EQ band has a resonance"),
                    SliderStyle::Rotary,
                )
            }),
            gains: BANDS.map(|band| Slider::new(band.gain, SliderStyle::Rotary).bipolar()),
            selected_band: 0,
            response_band: Some(0),
            spectrogram: SpectrogramAnalysis::default(),
        }
    }
}

impl EqualizerSection {
    /// The spectrogram's settings (`EqualizerSection`'s constructor and
    /// `renderOpenGlComponents`), with its amplitudes moved towards the
    /// equalizer's latest output.
    fn update_spectrogram(&mut self, host: &Host) -> Spectrogram {
        let cutoff = Host::info(BANDS[0].cutoff);
        let mut spectrogram = Spectrogram {
            min_frequency: utils::midi_note_to_frequency(cutoff.min),
            max_frequency: utils::midi_note_to_frequency(cutoff.max),
            ..Spectrogram::default()
        };
        if let Some(telemetry) = host.model().telemetry() {
            let (sample_rate, oversampling) =
                telemetry.read(|snapshot| (snapshot.sample_rate, snapshot.oversampling_amount));
            spectrogram.sample_rate = sample_rate;
            if oversampling >= 1 {
                spectrogram.oversample_amount = oversampling;
            }
            self.spectrogram
                .update(&spectrogram, telemetry.equalizer_memory());
        }
        spectrogram
    }

    /// `EqualizerSection::paintBackground`, then its controls.
    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        host: &Host,
        pixels: &PixelPainter,
        scope: &Scope,
        overlay: &mut Overlay,
        bounds: Rect,
        clip: Rect,
    ) {
        let l = EqualizerLayout::new(scope, bounds);
        let band = self.selected_band;
        let modes = MODES.map(|mode| host.value(mode) != 0.0);
        // setGainActive: a band in its pass or notch mode has no gain.
        let gain = Slider {
            active: !modes[band],
            ..self.gains[band]
        };
        let rotaries = [
            (&gain, l.gain),
            (&self.cutoffs[band], l.cutoff),
            (&self.resonances[band], l.resonance),
        ];
        paint_section_background(pixels, scope, Effect::Eq, bounds, &rotaries, &[l.response]);
        for (text, rect) in [
            ("GAIN", l.gain),
            ("RESONANCE", l.resonance),
            ("CUTOFF", l.cutoff),
        ] {
            draw_label(pixels, scope, text, rect, false);
        }
        let button_rounding = scope.value(ValueId::LabelBackgroundRounding);
        for rect in [l.low_mode, l.band_mode, l.high_mode] {
            pixels.fill_rounded_rect(
                rect,
                button_rounding,
                scope.color(ColorId::TextComponentBackground),
            );
        }

        let live = Live::new(host);
        let response = EqualizerResponse {
            low: BANDS[0],
            band: Some(BANDS[1]),
            high: BANDS[2],
            high_pass: modes[0],
            notch: modes[1],
            low_pass: modes[2],
            active: true,
            draw_frequency_lines: true,
            db_buffer_ratio: DEFAULT_DB_BUFFER_RATIO,
        };
        let hz = host.model().hz_frequency_display();
        let popup = Some((self.cutoffs[self.selected_band].displayed(hz), l.cutoff));
        let spectrogram = self.update_spectrogram(host);
        let analysis = &self.spectrogram;
        // paintBackground and renderOpenGlComponents: a line in the lighten
        // colour over the widget background lightened, fully opaque.
        let fill = scope
            .color(ColorId::WidgetBackground)
            .overlaid_with(scope.color(ColorId::LightenScreen))
            .with_alpha(255);
        let colours = (scope.color(ColorId::LightenScreen), fill);
        let body = scope.color(ColorId::Body);
        let mut under = |pixels: &PixelPainter| {
            spectrogram.paint(pixels, scope, l.response, analysis, colours, body);
        };
        if let Some(band) = response.show(
            ui,
            &live,
            pixels,
            scope,
            overlay,
            l.response,
            &mut self.response_band,
            popup,
            &mut under,
        ) {
            self.selected_band = band;
        }

        let mut panel = Panel::new(ui, host, pixels, *scope, overlay, clip);
        for (index, rect) in [l.low_mode, l.band_mode, l.high_mode]
            .into_iter()
            .enumerate()
        {
            // OpenGlShapeButton with useOnColors: a click toggles the mode
            // and selects its band (buttonClicked).
            let colours = if modes[index] {
                ShapeColours::icon_on(&panel.scope)
            } else {
                ShapeColours::icon(&panel.scope)
            };
            let button = ShapeButton {
                shape: Shape::Icon {
                    key: MODES[index],
                    path: &mode_icons()[index],
                },
                colours,
            };
            if panel.shape_button(MODES[index], button, rect).clicked {
                host.set_once(MODES[index], if modes[index] { 0.0 } else { 1.0 });
                self.selected_band = index;
                self.response_band = Some(index);
            }
        }
        let band = self.selected_band;
        let gain = Slider {
            active: !modes[band],
            ..self.gains[band]
        };
        panel.slider(&gain, l.gain);
        panel.slider(&self.cutoffs[band], l.cutoff);
        panel.slider(&self.resonances[band], l.resonance);
        let tabs = TabSelector {
            font_height_percent: 0.4,
            ..TabSelector::new(&["LOW", "BAND", "HIGH"])
        };
        let id = panel.ui.scope_id().with("eq selected band");
        if tabs.interact(
            panel.ui,
            pixels,
            id,
            l.selected_band,
            &mut self.selected_band,
        ) {
            // setSelectedBand.
            self.response_band = Some(self.selected_band);
        }
        tabs.paint(pixels, &panel.scope, l.selected_band, self.selected_band);
        viewer_corners(&mut panel, l.response);
        panel.finish();
    }
}
