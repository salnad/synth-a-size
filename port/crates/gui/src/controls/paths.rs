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
