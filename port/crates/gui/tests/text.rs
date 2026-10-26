// SPDX-License-Identifier: GPL-3.0-or-later

//! Text drawn into a box narrower than it: `Graphics::drawText` curtails
//! the line, with or without an ellipsis, and `Graphics::drawFittedText`
//! squeezes it, rather than letting it overflow.

use sas_gui::colour::Colour;
use sas_gui::fonts::Face;
use sas_gui::paint::{PixelPainter, RectF};
use sas_gui::text::{Justify, LineFit, Overflow, MINIMUM_HORIZONTAL_SCALE};

/// Eleven glyphs ten pixels wide, like "0.000 cents" (the sixth a space).
const EDGES: [f32; 11] = [
    10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0,
];
const DOT: f32 = 4.0;

fn fit(width: f32, overflow: Overflow) -> LineFit {
    LineFit::new(&EDGES, DOT, width, overflow)
}

fn kept(kept: usize, scale: f32) -> LineFit {
    LineFit {
        kept,
        scale,
        dots: 0,
        dots_x: 0.0,
    }
}

#[test]
fn draw_text_drops_the_glyphs_from_the_first_more_than_a_pixel_past_the_box() {
    assert_eq!(fit(110.0, Overflow::Curtail), kept(11, 1.0));
    assert_eq!(
        fit(109.1, Overflow::Curtail),
        kept(11, 1.0),
        "a pixel's grace"
    );
    assert_eq!(fit(108.9, Overflow::Curtail), kept(10, 1.0));
    assert_eq!(fit(100.0, Overflow::Curtail), kept(10, 1.0));
    assert_eq!(fit(98.0, Overflow::Curtail), kept(9, 1.0));
    assert_eq!(fit(0.0, Overflow::Curtail), kept(0, 1.0));
}

#[test]
fn draw_text_with_ellipses_backs_off_until_three_full_stops_fit() {
    // Seven glyphs fit 75 pixels; dropping back to the sixth's left (60)
    // leaves room for three stops (72).
    let ellipsis = fit(75.0, Overflow::Ellipsis);
    assert_eq!(
        ellipsis,
        LineFit {
            kept: 6,
            scale: 1.0,
            dots: 3,
            dots_x: 60.0,
        }
    );
    assert_eq!(
        ellipsis.extent(&EDGES, DOT, Justify::Left, |_| false),
        Some((0.0, 72.0))
    );
    // Lines that fit, have three glyphs or fewer, or keep fewer than three
    // are only curtailed.
    assert_eq!(fit(110.0, Overflow::Ellipsis), kept(11, 1.0));
    assert_eq!(
        LineFit::new(&EDGES[..3], DOT, 15.0, Overflow::Ellipsis),
        kept(1, 1.0)
    );
    assert_eq!(fit(25.0, Overflow::Ellipsis), kept(2, 1.0));
}

#[test]
fn fitted_text_squeezes_down_to_the_minimum_scale_then_adds_an_ellipsis() {
    assert_eq!(fit(110.0, Overflow::Fit), kept(11, 1.0));
    assert_eq!(fit(100.0, Overflow::Fit), kept(11, 100.0 / 110.0));
    assert_eq!(fit(80.0, Overflow::Fit), kept(11, 80.0 / 110.0));
    // At the minimum scale the line is 77 wide; fitLineIntoSpace takes half
    // a pixel off it.
    assert_eq!(fit(76.8, Overflow::Fit), kept(11, MINIMUM_HORIZONTAL_SCALE));
    // Squeezed glyphs are 7 wide: back off to the eighth's left (56).
    assert_eq!(
        fit(70.0, Overflow::Fit),
        LineFit {
            kept: 8,
            scale: MINIMUM_HORIZONTAL_SCALE,
            dots: 3,
            dots_x: 56.0,
        }
    );
}

#[test]
fn centring_leaves_out_whitespace() {
    let space = |i: usize| i == 5 || i == 10;
    let line = kept(11, 1.0);
    assert_eq!(
        line.extent(&EDGES, DOT, Justify::Left, space),
        Some((0.0, 110.0))
    );
    assert_eq!(
        line.extent(&EDGES, DOT, Justify::Centred, space),
        Some((0.0, 100.0))
    );
    assert_eq!(
        kept(6, 1.0).extent(&EDGES, DOT, Justify::Centred, space),
        Some((0.0, 50.0))
    );
    assert_eq!(
        kept(0, 1.0).extent(&EDGES, DOT, Justify::Centred, space),
        None
    );
}

const SCALE: f32 = 2.0;
const SIZE: f32 = 22.0;
const WHITE: Colour = Colour(0xffffffff);

/// A galley painted: its text, and its left and width in pixels.
#[derive(Debug)]
struct Painted {
    text: String,
    left: f32,
    width: f32,
}

/// Runs `draw` with a painter at display scale [`SCALE`] and returns what
/// it painted.
fn painted(draw: impl Fn(&PixelPainter)) -> Vec<Painted> {
    let ctx = egui::Context::default();
    ctx.set_fonts(sas_gui::fonts::definitions());
    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 200.0),
        )),
        ..Default::default()
    };
    // Fonts set before a pass take effect from the next.
    ctx.run_ui(input(), |_| {}).textures_delta.clear();
    let mut output = ctx.run_ui(input(), |ui| {
        let area = ui.ctx().content_rect();
        let painter = ui.painter_at(area);
        draw(&PixelPainter::new(&painter, area, SCALE));
    });
    output.textures_delta.clear();
    output
        .shapes
        .into_iter()
        .filter_map(|clipped| match clipped.shape {
            egui::Shape::Text(text) => Some(Painted {
                text: text.galley.job.text.clone(),
                left: (text.pos.x + text.galley.rect.min.x) * SCALE,
                width: text.galley.rect.width() * SCALE,
            }),
            _ => None,
        })
        .collect()
}

/// The voice tune's reading, which upstream shows as "0.000 cent" where
/// the "s" would pass its box.
#[test]
fn a_reading_too_wide_for_its_box_is_drawn_curtailed_ellipsed_or_squeezed() {
    let line = |width: f32, y: f32, overflow| {
        move |pixels: &PixelPainter| {
            let rect = RectF::new(10.0, y, width, 40.0);
            let style = (Face::Light, SIZE, WHITE);
            pixels.text_line("0.000 cents", rect, style, Justify::Left, overflow);
        }
    };
    let measured = painted(|pixels| {
        line(400.0, 0.0, Overflow::Curtail)(pixels);
        pixels.text_centred(
            "...",
            RectF::new(10.0, 50.0, 400.0, 40.0),
            Face::Light,
            SIZE,
            WHITE,
        );
    });
    let full = measured[0].width;
    let dot = measured[1].width / 3.0;
    assert!(full > 60.0, "{full}");

    let texts = painted(|pixels| {
        let narrow = RectF::new(10.0, 10.0, full - 4.0, 40.0);
        pixels.text_centred("0.000 cents", narrow, Face::Light, SIZE, WHITE);
        line(full - 4.0, 60.0, Overflow::Ellipsis)(pixels);
        line(full * 0.8, 110.0, Overflow::Fit)(pixels);
        line(full + 4.0, 160.0, Overflow::Fit)(pixels);
    });
    let summary: Vec<&str> = texts.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(
        summary,
        [
            "0.000 cent",
            "0.000 cen",
            "...",
            "0.000 cents",
            "0.000 cents"
        ]
    );
    // The full stops follow the kept glyphs.
    let (kept, dots) = (&texts[1], &texts[2]);
    assert!(
        (dots.left - (kept.left + kept.width)).abs() < 1.0,
        "{texts:?}"
    );
    assert!(
        dots.left + 3.0 * dot <= 10.0 + full - 4.0 + 0.5,
        "{texts:?}"
    );
    // Squeezed into its box from its left.
    assert!((texts[3].width - full * 0.8).abs() < 0.5, "{texts:?}");
    assert!((texts[3].left - 10.0).abs() < 0.5, "{texts:?}");
    assert!((texts[4].width - full).abs() < 0.5, "{texts:?}");
}
