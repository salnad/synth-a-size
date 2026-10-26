// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Upstream's hand-built icons (`Paths` in `look_and_feel/paths.h`), each
//! in its unit square and built once. Fill them with
//! [`fill_path_to_fit`](super::icon::fill_path_to_fit).

use std::f32::consts::PI;
use std::sync::OnceLock;

use super::path::{EndCapStyle, JointStyle, Path};

/// Pads an icon to the square from `from` to `to`, as upstream's icon paths
/// do with zero-length line segments.
fn pad(path: &mut Path, from: f32, to: f32, thickness: f32) {
    path.add_line_segment((from, from), (from, from), thickness);
    path.add_line_segment((to, to), (to, to), thickness);
}

/// `Paths::clock`.
pub fn clock() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const CLOCK_ANGLE: f32 = 1.0;
        const CLOCK_WIDTH: f32 = 0.4;
        const BUFFER: f32 = (1.0 - CLOCK_WIDTH) / 2.0;
        let mut path = Path::new();
        path.add_pie_segment(
            BUFFER,
            BUFFER,
            CLOCK_WIDTH,
            CLOCK_WIDTH,
            0.0,
            CLOCK_ANGLE - 2.0 * PI,
            0.0,
        );
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::note`.
pub fn note() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const LEFT_ADJUSTMENT: f32 = 1.0 / 32.0;
        const NOTE_WIDTH: f32 = 1.0 / 4.0;
        const NOTE_HEIGHT: f32 = 1.0 / 5.0;
        const BAR_HEIGHT: f32 = 2.0 / 5.0;
        const BAR_WIDTH: f32 = 1.0 / 20.0;
        const Y: f32 = 1.0 - (1.0 - BAR_HEIGHT + NOTE_HEIGHT / 2.0) / 2.0;
        const X: f32 = (1.0 - NOTE_WIDTH) / 2.0 - LEFT_ADJUSTMENT;
        let mut path = Path::new();
        path.add_ellipse(X, Y - NOTE_HEIGHT / 2.0, NOTE_WIDTH, NOTE_HEIGHT);
        path.add_rectangle(
            X + NOTE_WIDTH - BAR_WIDTH,
            Y - BAR_HEIGHT,
            BAR_WIDTH,
            BAR_HEIGHT,
        );
        path.close();
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::tripletNotes`.
pub fn triplet_notes() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const NOTE_WIDTH: f32 = 1.0 / 5.0;
        const NOTE_HEIGHT: f32 = 1.0 / 6.0;
        const X: f32 = (1.0 - 3.0 * NOTE_WIDTH) / 2.0;
        const BAR_WIDTH: f32 = 1.0 / 20.0;
        const BAR_HEIGHT: f32 = 1.0 / 4.0;
        const Y: f32 = 1.0 - (1.0 - BAR_HEIGHT) / 2.0;
        let mut path = Path::new();
        path.add_rectangle(
            X + NOTE_WIDTH - BAR_WIDTH,
            Y - BAR_HEIGHT - BAR_WIDTH,
            2.0 * NOTE_WIDTH + BAR_WIDTH,
            BAR_WIDTH,
        );
        for i in 0..3 {
            let offset = i as f32 * NOTE_WIDTH;
            path.add_ellipse(X + offset, Y - NOTE_HEIGHT / 2.0, NOTE_WIDTH, NOTE_HEIGHT);
            path.add_rectangle(
                X + NOTE_WIDTH - BAR_WIDTH + offset,
                Y - BAR_HEIGHT,
                BAR_WIDTH,
                BAR_HEIGHT,
            );
        }
        path.close();
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// The three keys of [`keyboard`], each a closed outline.
fn add_keys(path: &mut Path) {
    const KEYS: [&[(f32, f32)]; 3] = [
        &[
            (0.24, 0.35),
            (0.24, 0.65),
            (0.41, 0.65),
            (0.41, 0.5),
            (0.35, 0.5),
            (0.35, 0.35),
        ],
        &[
            (0.48, 0.65),
            (0.48, 0.35),
            (0.52, 0.35),
            (0.52, 0.5),
            (0.58, 0.5),
            (0.58, 0.65),
            (0.42, 0.65),
            (0.42, 0.5),
            (0.48, 0.5),
        ],
        &[
            (0.65, 0.35),
            (0.76, 0.35),
            (0.76, 0.65),
            (0.59, 0.65),
            (0.59, 0.5),
            (0.65, 0.5),
            (0.65, 0.35),
        ],
    ];
    for key in KEYS {
        path.move_to(key[0].0, key[0].1);
        for &(x, y) in &key[1..] {
            path.line_to(x, y);
        }
        path.close();
    }
}

/// `Paths::keyboard`: three keys, padded to the square from 0.2 to 0.8
/// (the sample section's keytrack button).
pub fn keyboard() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut path = Path::new();
        pad(&mut path, 0.2, 0.8, 0.01);
        add_keys(&mut path);
        path
    })
}

/// `Paths::keyboardBordered`: [`keyboard`] padded to the unit square
/// (`TempoSelector`'s keytrack mode).
pub fn keyboard_bordered() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut path = keyboard().clone();
        pad(&mut path, 0.0, 1.0, 0.01);
        path
    })
}

/// `Paths::createFilterStroke`: `outline` stroked `line_thickness` wide
/// with curved joints and rounded ends.
fn create_filter_stroke(outline: &Path, line_thickness: f32) -> Path {
    outline.stroked(line_thickness, JointStyle::Curved, EndCapStyle::Rounded)
}

/// An open outline through `points`.
fn outline(points: &[(f32, f32)]) -> Path {
    let mut path = Path::new();
    path.move_to(points[0].0, points[0].1);
    for &(x, y) in &points[1..] {
        path.line_to(x, y);
    }
    path
}

/// `Paths::lowPass()` (its default line thickness, 0.1).
pub fn low_pass() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut path = create_filter_stroke(
            &outline(&[(1.0, 0.8), (0.7, 0.3), (0.5, 0.5), (0.0, 0.5)]),
            0.1,
        );
        path.add_line_segment((1.0, 0.2), (1.0, 0.2), 0.2);
        path
    })
}

/// `Paths::highPass`.
pub fn high_pass() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut path = create_filter_stroke(
            &outline(&[(0.0, 0.8), (0.3, 0.3), (0.5, 0.5), (1.0, 0.5)]),
            0.1,
        );
        path.add_line_segment((1.0, 0.2), (1.0, 0.2), 0.2);
        path
    })
}

/// `Paths::notch`.
pub fn notch() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const TOP: f32 = 2.0 / 5.0;
        const BOTTOM: f32 = 4.0 / 5.0;
        let mut path = create_filter_stroke(
            &outline(&[
                (0.0, TOP),
                (1.0 / 3.0, TOP),
                (1.0 / 2.0, BOTTOM),
                (2.0 / 3.0, TOP),
                (1.0, TOP),
            ]),
            0.1,
        );
        path.add_line_segment((1.0, 0.2), (1.0, 0.2), 0.1);
        path
    })
}

/// `Paths::magnifyingGlass`.
pub fn magnifying_glass() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const RADIUS: f32 = 0.22;
        const WIDTH_RATIO: f32 = 0.3;
        const GLASS_OFFSET: f32 = 0.2;
        #[allow(clippy::approx_constant, clippy::excessive_precision)]
        const SQRT_2: f32 = 1.414_213_562_37;
        let mut path = Path::new();
        pad(&mut path, 0.0, 1.0, 0.2);
        let diameter = 2.0 * RADIUS;
        path.add_pie_segment(
            GLASS_OFFSET,
            GLASS_OFFSET,
            diameter,
            diameter,
            0.0,
            2.0 * PI,
            1.0 - WIDTH_RATIO,
        );
        let line_width = WIDTH_RATIO * RADIUS;
        let line_start = GLASS_OFFSET + SQRT_2 * RADIUS + line_width / 2.0;
        path.add_line_segment(
            (line_start, line_start),
            (1.0 - GLASS_OFFSET, 1.0 - GLASS_OFFSET),
            line_width,
        );
        path
    })
}

/// `Paths::loop`.
pub fn loop_arrow() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let outline = outline(&[
            (0.68, 0.3),
            (0.85, 0.3),
            (0.85, 0.7),
            (0.15, 0.7),
            (0.15, 0.3),
            (0.61, 0.3),
        ]);
        let mut path = outline.stroked_with_arrowheads(
            0.12,
            JointStyle::Curved,
            EndCapStyle::Rounded,
            (0.0, 0.0),
            (0.4, 0.26),
        );
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::bounce`: two arrows out from the middle.
pub fn bounce() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut outline = outline(&[(0.5, 0.5), (0.0, 0.5)]);
        outline.move_to(0.5, 0.5);
        outline.line_to(1.0, 0.5);
        let mut path = outline.stroked_with_arrowheads(
            0.12,
            JointStyle::Curved,
            EndCapStyle::Butt,
            (0.0, 0.0),
            (0.4, 0.26),
        );
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::shuffle`.
pub fn shuffle() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        let over = outline(&[(0.1, 0.7), (0.25, 0.7), (0.55, 0.3), (0.95, 0.3)]);
        let under_first = outline(&[(0.1, 0.3), (0.25, 0.3), (0.325, 0.4)]);
        let under_second = outline(&[(0.475, 0.6), (0.55, 0.7), (0.95, 0.7)]);
        let arrow = |outline: &Path| {
            outline.stroked_with_arrowheads(
                0.12,
                JointStyle::Curved,
                EndCapStyle::Butt,
                (0.0, 0.0),
                (0.35, 0.26),
            )
        };
        let mut path = arrow(&over);
        path.add_path(&under_first.stroked(0.12, JointStyle::Curved, EndCapStyle::Butt));
        path.add_path(&arrow(&under_second));
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::pencil`.
pub fn pencil() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const DIMENSION: f32 = 0.14;
        const ERASER_LENGTH: f32 = 0.6 * DIMENSION;
        const SEPARATOR_WIDTH: f32 = 0.15 * DIMENSION;
        const REMOVAL: f32 = ERASER_LENGTH + SEPARATOR_WIDTH;
        const BORDER: f32 = 0.22;
        let mut path = outline(&[
            (BORDER, 1.0 - BORDER),
            (BORDER + DIMENSION, 1.0 - BORDER),
            (1.0 - BORDER - REMOVAL, BORDER + DIMENSION + REMOVAL),
            (1.0 - BORDER - REMOVAL - DIMENSION, BORDER + REMOVAL),
            (BORDER, 1.0 - BORDER - DIMENSION),
        ]);
        path.close();
        path.move_to(1.0 - BORDER - DIMENSION, BORDER);
        path.line_to(1.0 - BORDER, BORDER + DIMENSION);
        path.line_to(
            1.0 - BORDER - ERASER_LENGTH,
            BORDER + DIMENSION + ERASER_LENGTH,
        );
        path.line_to(
            1.0 - BORDER - ERASER_LENGTH - DIMENSION,
            BORDER + ERASER_LENGTH,
        );
        path.close();
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::halfSinCurve`. Upstream starts the curve at (`start_x`,
/// `end_x`), which this keeps.
pub fn half_sin_curve() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const BORDER: f32 = 0.15;
        const LINE_WIDTH: f32 = 0.1;
        const ENDPOINT_STROKE_WIDTH: f32 = 0.08;
        const ENDPOINT_RADIUS: f32 = 0.09;
        const NUM_CURVE_POINTS: usize = 16;
        const BUMP_IN: f32 = ENDPOINT_RADIUS;
        const ADJUST_X_IN: f32 = BUMP_IN + ENDPOINT_RADIUS / 2.0;
        let start_x = BORDER + ADJUST_X_IN;
        let start_y = 1.0 - BORDER - BUMP_IN;
        let end_x = 1.0 - BORDER - ADJUST_X_IN;
        let end_y = BORDER + BUMP_IN;
        let mut curve = Path::new();
        curve.move_to(start_x, end_x);
        for i in 0..NUM_CURVE_POINTS {
            let t = (1.0 + i as f32) / NUM_CURVE_POINTS as f32;
            let x = t * end_x + (1.0 - t) * start_x;
            let y_t = ((t - 0.5) * PI).sin() * 0.5 + 0.5;
            let y = y_t * end_y + (1.0 - y_t) * start_y;
            curve.line_to(x, y);
        }
        let mut path = curve.stroked(LINE_WIDTH, JointStyle::Curved, EndCapStyle::Butt);
        let mut arc = Path::new();
        arc.centred_arc(
            end_x + BUMP_IN,
            end_y,
            ENDPOINT_RADIUS,
            ENDPOINT_RADIUS,
            0.0,
            2.0 * PI,
        );
        arc.centred_arc(
            start_x - BUMP_IN,
            start_y,
            ENDPOINT_RADIUS,
            ENDPOINT_RADIUS,
            0.0,
            2.0 * PI,
        );
        path.add_path(&arc.stroked(ENDPOINT_STROKE_WIDTH, JointStyle::Curved, EndCapStyle::Butt));
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// `Paths::paintBrush`.
pub fn paint_brush() -> &'static Path {
    static PATH: OnceLock<Path> = OnceLock::new();
    PATH.get_or_init(|| {
        const WIDE: f32 = 0.3;
        const HANDLE: f32 = 0.08;
        const LENGTH: f32 = 0.6 * WIDE;
        const SEPARATOR: f32 = 0.15 * WIDE;
        const CONNECTION: f32 = LENGTH + SEPARATOR;
        const HANDLE_DISTANCE: f32 = CONNECTION + WIDE * 0.8;
        const BORDER: f32 = 0.15;
        let mut path = outline(&[
            (BORDER, 1.0 - BORDER),
            (BORDER + HANDLE, 1.0 - BORDER),
            (
                1.0 - BORDER - HANDLE_DISTANCE,
                BORDER + HANDLE + HANDLE_DISTANCE,
            ),
            (1.0 - BORDER - CONNECTION, BORDER + WIDE + CONNECTION),
            (1.0 - BORDER - CONNECTION - WIDE, BORDER + CONNECTION),
            (
                1.0 - BORDER - HANDLE_DISTANCE - HANDLE,
                BORDER + HANDLE_DISTANCE,
            ),
            (BORDER, 1.0 - BORDER - HANDLE),
        ]);
        path.close();
        path.move_to(1.0 - BORDER - WIDE, BORDER);
        path.line_to(1.0 - BORDER, BORDER + WIDE);
        path.line_to(1.0 - BORDER - LENGTH, BORDER + WIDE + LENGTH);
        path.line_to(1.0 - BORDER - LENGTH - WIDE, BORDER + LENGTH);
        path.close();
        pad(&mut path, 0.0, 1.0, 0.2);
        path
    })
}

/// A filter icon's shape with its zero-length pads (`addLineSegment` at
/// each of `pads`, `thickness` thick).
fn filter_icon(points: &[(f32, f32)], pads: &[(f32, f32)], thickness: f32) -> Path {
    let mut path = create_filter_stroke(&outline(points), 0.1);
    for &pad in pads {
        path.add_line_segment(pad, pad, thickness);
    }
    path
}

/// The filter blend's icons (`FilterSection`'s `getLeftMorphPath` /
/// `getRightMorphPath`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterIcon {
    LowPass,
    HighPass,
    BandPass,
    LeftArrow,
    RightArrow,
    Phaser1,
    Phaser3,
    Notch,
    NarrowBand,
    WideBand,
}

impl FilterIcon {
    pub fn key(self) -> &'static str {
        match self {
            FilterIcon::LowPass => "filter low pass",
            FilterIcon::HighPass => "filter high pass",
            FilterIcon::BandPass => "filter band pass",
            FilterIcon::LeftArrow => "filter left arrow",
            FilterIcon::RightArrow => "filter right arrow",
            FilterIcon::Phaser1 => "filter phaser 1",
            FilterIcon::Phaser3 => "filter phaser 3",
            FilterIcon::Notch => "filter notch",
            FilterIcon::NarrowBand => "filter narrow band",
            FilterIcon::WideBand => "filter wide band",
        }
    }

    /// `Paths::lowPass` and the rest.
    pub fn path(self) -> &'static Path {
        static PATHS: OnceLock<Vec<Path>> = OnceLock::new();
        let paths = PATHS.get_or_init(|| {
            const MIDDLE: f32 = 3.0 / 5.0;
            const BOTTOM: f32 = 4.0 / 5.0;
            const TOP: f32 = 2.0 / 5.0;
            const ARROW: f32 = 1.0 / 3.0;
            let corner = [(1.0, 0.2)];
            let phaser_pads = [(1.0, 0.15), (1.0, 0.85)];
            let mut phaser3 = vec![(0.0, 0.5)];
            let delta = 1.0 / (2.0 * 5.0 + 2.0);
            let mut position = 0.0;
            for _ in 0..5 {
                position += delta;
                phaser3.push((position, 3.0 / 4.0));
                position += delta;
                phaser3.push((position, 1.0 / 4.0));
            }
            position += delta;
            phaser3.push((position, 3.0 / 4.0));
            phaser3.push((1.0, 0.5));
            let arrow = |tip: f32, back: f32| {
                let mut outline = outline(&[(1.0, 0.5), (0.0, 0.5)]);
                outline.move_to(tip, 0.5);
                outline.line_to(back, 0.5 - ARROW);
                outline.move_to(tip, 0.5);
                outline.line_to(back, 0.5 + ARROW);
                create_filter_stroke(&outline, 0.1)
            };
            vec![
                low_pass().clone(),
                high_pass().clone(),
                filter_icon(
                    &[
                        (0.0, BOTTOM),
                        (1.0 / 3.0, MIDDLE),
                        (0.5, 0.25),
                        (2.0 / 3.0, MIDDLE),
                        (1.0, BOTTOM),
                    ],
                    &corner,
                    0.2,
                ),
                arrow(0.0, ARROW),
                arrow(1.0, 1.0 - ARROW),
                filter_icon(
                    &[
                        (0.0, 0.5),
                        (1.0 / 3.0, 3.0 / 4.0),
                        (1.0 / 2.0, 1.0 / 4.0),
                        (2.0 / 3.0, 3.0 / 4.0),
                        (1.0, 0.5),
                    ],
                    &phaser_pads,
                    0.1,
                ),
                filter_icon(&phaser3, &phaser_pads, 0.1),
                notch().clone(),
                filter_icon(
                    &[
                        (0.0, BOTTOM),
                        (1.0 / 3.0, BOTTOM),
                        (0.5, TOP),
                        (2.0 / 3.0, BOTTOM),
                        (1.0, BOTTOM),
                    ],
                    &corner,
                    0.1,
                ),
                filter_icon(
                    &[
                        (0.0, BOTTOM),
                        (1.0 / 3.0, TOP),
                        (2.0 / 3.0, TOP),
                        (1.0, BOTTOM),
                    ],
                    &corner,
                    0.1,
                ),
            ]
        });
        &paths[self as usize]
    }
}

/// The hand-built icons the Voice tab's buttons show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Keyboard,
    Loop,
    Bounce,
    Shuffle,
    Pencil,
    PaintBrush,
    HalfSinCurve,
    MagnifyingGlass,
}

impl Icon {
    /// The icon's cache key for `fill_path`.
    pub fn key(self) -> &'static str {
        match self {
            Icon::Keyboard => "icon keyboard",
            Icon::Loop => "icon loop",
            Icon::Bounce => "icon bounce",
            Icon::Shuffle => "icon shuffle",
            Icon::Pencil => "icon pencil",
            Icon::PaintBrush => "icon paint brush",
            Icon::HalfSinCurve => "icon half sin curve",
            Icon::MagnifyingGlass => "icon magnifying glass",
        }
    }

    pub fn path(self) -> &'static Path {
        match self {
            Icon::Keyboard => keyboard(),
            Icon::Loop => loop_arrow(),
            Icon::Bounce => bounce(),
            Icon::Shuffle => shuffle(),
            Icon::Pencil => pencil(),
            Icon::PaintBrush => paint_brush(),
            Icon::HalfSinCurve => half_sin_curve(),
            Icon::MagnifyingGlass => magnifying_glass(),
        }
    }
}

/// `Paths::menu(width)`: three lines across a `width`-pixel square,
/// snapped to whole pixels.
pub fn menu(width: i32) -> Path {
    const BUFFER: f32 = 0.2;
    const LINE_WIDTH: f32 = 0.04;
    const SPACING: f32 = 0.2;
    let buffer = (width as f32 * BUFFER).round();
    let line_width = ((width as f32 * LINE_WIDTH) as i32).max(1);
    let spacing = (width as f32 * SPACING) as i32 as f32;
    let center = (line_width % 2) as f32 * 0.5 + (width / 2) as f32;
    let (w, lw) = (width as f32, line_width as f32);
    let mut path = Path::new();
    for y in [center - spacing, center, center + spacing] {
        path.add_line_segment((buffer, y), (w - buffer, y), lw);
    }
    path.add_line_segment((0.0, 0.0), (0.0, 0.0), 0.2);
    path.add_line_segment((w, w), (w, w), 0.2);
    path
}

/// `Paths::save(width)`: a downward arrow into an open box, stroked with
/// curved joints and rounded ends, in a `width`-pixel square.
pub fn save(width: i32) -> Path {
    const LINE_WIDTH: f32 = 0.04;
    const SPACING_X: f32 = 0.3;
    const SPACING_Y: f32 = 0.25;
    const ARROW_MOVEMENT: f32 = 0.14;
    const BOX_WRAP: f32 = 0.07;
    let w = width as f32;
    let line_width = ((w * LINE_WIDTH) as i32).max(1);
    let spacing_x = (w * SPACING_X) as i32 as f32;
    let spacing_y = (w * SPACING_Y) as i32 as f32;
    let movement = w * ARROW_MOVEMENT / 2.0;
    let wrap = w * BOX_WRAP;
    let center = (line_width % 2) as f32 * 0.5 + (width / 2) as f32;

    let mut outline = Path::new();
    outline.move_to(center, center - spacing_y);
    outline.line_to(center, center + movement);
    outline.move_to(center - 2.0 * movement, center - movement);
    outline.line_to(center, center + movement);
    outline.line_to(center + 2.0 * movement, center - movement);
    outline.move_to(center - spacing_x + wrap, center);
    outline.line_to(center - spacing_x, center);
    outline.line_to(center - spacing_x, center + spacing_y);
    outline.line_to(center + spacing_x, center + spacing_y);
    outline.line_to(center + spacing_x, center);
    outline.line_to(center + spacing_x - wrap, center);

    let mut path = outline.stroked(line_width as f32, JointStyle::Curved, EndCapStyle::Rounded);
    path.add_line_segment((0.0, 0.0), (0.0, 0.0), 0.2);
    path.add_line_segment((w, w), (w, w), 0.2);
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each icon's padding sets its bounds, which `getTransformToScaleToFit`
    /// scales to the component (a zero-length segment adds only its point).
    #[test]
    fn icons_span_their_squares() {
        for path in [clock(), note(), triplet_notes(), keyboard_bordered()] {
            assert_eq!(path.bounds(), Some((0.0, 0.0, 1.0, 1.0)));
        }
        assert_eq!(keyboard().bounds(), Some((0.2, 0.2, 0.8, 0.8)));
    }

    /// The keyboard's three keys are closed outlines inside its padding.
    #[test]
    fn the_keyboard_has_three_keys() {
        let keys: Vec<_> = keyboard()
            .subpaths()
            .iter()
            .filter(|subpath| subpath.points.len() > 4)
            .collect();
        assert_eq!(keys.len(), 3);
        assert!(keys.iter().all(|key| key.closed));
    }
}
