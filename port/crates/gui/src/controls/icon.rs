// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! Filling [`Path`]s: JUCE's `Graphics::fillPath`, and upstream's icons.
//!
//! Upstream fills icon paths with JUCE's software renderer into the images
//! its components paint to (`paintToImage`, the effect order's background).
//! Here a path is rasterised once per size and position on the CPU (non-zero
//! winding, exact coverage across each scanline and 16 scanlines a pixel),
//! kept as a white texture, and drawn tinted with the colour.

use std::sync::OnceLock;

use egui::{Color32, ColorImage, Id, TextureHandle, TextureOptions};

use super::path::{Path, Transform};
use crate::colour::Colour;
use crate::paint::{PixelPainter, RectF};

/// Sub-scanlines per pixel row.
const SUBSCANLINES: usize = 16;
/// Rasterised paths kept at once (each window size and position is one).
const CACHE_SIZE: usize = 256;

/// Coverage (0 to 1) of `path` mapped by `transform`, over the `width` ×
/// `height` pixels whose top left corner is pixel `origin`, row by row.
pub fn rasterise(
    path: &Path,
    transform: Transform,
    origin: (i32, i32),
    width: usize,
    height: usize,
) -> Vec<f32> {
    let edges: Vec<((f32, f32), (f32, f32))> = path
        .subpaths()
        .iter()
        .filter(|subpath| subpath.points.len() > 2)
        .flat_map(|subpath| {
            // Filling closes every subpath.
            let points: Vec<(f32, f32)> = subpath
                .points
                .iter()
                .map(|&point| {
                    let (x, y) = transform.apply(point);
                    (x - origin.0 as f32, y - origin.1 as f32)
                })
                .collect();
            let count = points.len();
            (0..count).map(move |i| (points[i], points[(i + 1) % count]))
        })
        .filter(|(a, b)| a.1 != b.1)
        .collect();

    let mut coverage = vec![0.0f32; width * height];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    let weight = 1.0 / SUBSCANLINES as f32;
    for row in 0..height {
        let line = &mut coverage[row * width..(row + 1) * width];
        for sub in 0..SUBSCANLINES {
            let y = row as f32 + (sub as f32 + 0.5) / SUBSCANLINES as f32;
            crossings.clear();
            for &((x0, y0), (x1, y1)) in &edges {
                let (top, bottom, direction) = if y0 < y1 { (y0, y1, 1) } else { (y1, y0, -1) };
                if y >= top && y < bottom {
                    let t = (y - y0) / (y1 - y0);
                    crossings.push((x0 + t * (x1 - x0), direction));
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in crossings.windows(2) {
                winding += pair[0].1;
                if winding != 0 {
                    add_span(line, pair[0].0, pair[1].0, weight);
                }
            }
        }
    }
    coverage
}

/// Adds `weight` × the covered fraction of each pixel between `from` and
/// `to` (pixels from the row's start).
fn add_span(line: &mut [f32], from: f32, to: f32, weight: f32) {
    let width = line.len() as f32;
    let (from, to) = (from.clamp(0.0, width), to.clamp(0.0, width));
    if to <= from {
        return;
    }
    let (first, last) = (from.floor() as usize, to.floor() as usize);
    if first == last {
        line[first] += (to - from) * weight;
        return;
    }
    line[first] += (first as f32 + 1.0 - from) * weight;
    for pixel in &mut line[first + 1..last] {
        *pixel += weight;
    }
    if last < line.len() {
        line[last] += (to - last as f32) * weight;
    }
}

/// `Graphics::fillPath(path, transform)` in `colour`. `key` names the path
/// (rasterisations are cached by it, the transform and the size).
pub fn fill_path(
    pixels: &PixelPainter,
    key: &str,
    path: &Path,
    transform: Transform,
    colour: Colour,
) {
    let Some((l, t, r, b)) = path.bounds() else {
        return;
    };
    let (x0, y0) = transform.apply((l, t));
    let (x1, y1) = transform.apply((r, b));
    let origin = (x0.floor() as i32, y0.floor() as i32);
    let width = (x1.ceil() as i32 - origin.0).clamp(0, 4096) as usize;
    let height = (y1.ceil() as i32 - origin.1).clamp(0, 4096) as usize;
    if width == 0 || height == 0 {
        return;
    }
    let ctx = pixels.painter().ctx();
    let id = Id::unique((
        "sas-gui path",
        key,
        transform.scale.to_bits(),
        transform.dx.to_bits(),
        transform.dy.to_bits(),
    ));
    let texture = ctx.data_mut(|data| data.get_temp::<TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let coverage = rasterise(path, transform, origin, width, height);
        let pixels: Vec<Color32> = coverage
            .iter()
            .map(|&alpha| {
                let alpha = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
                Color32::from_rgba_premultiplied(alpha, alpha, alpha, alpha)
            })
            .collect();
        let image = ColorImage::new([width, height], pixels);
        let texture = ctx.load_texture(format!("path {key}"), image, TextureOptions::NEAREST);
        ctx.data_mut(|data| {
            data.insert_temp(id, texture.clone());
            let order = data.get_temp_mut_or_default::<Vec<Id>>(Id::unique("sas-gui path cache"));
            order.push(id);
            if order.len() > CACHE_SIZE {
                let evicted = order.remove(0);
                data.remove::<TextureHandle>(evicted);
            }
        });
        texture
    });
    let rect = RectF::new(
        origin.0 as f32,
        origin.1 as f32,
        width as f32,
        height as f32,
    );
    pixels.painter().image(
        texture.id(),
        pixels.rect(rect),
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        colour.to_egui(),
    );
}

/// `Graphics::fillPath(path, path.getTransformToScaleToFit(area, true))`.
pub fn fill_path_to_fit(
    pixels: &PixelPainter,
    key: &str,
    path: &Path,
    area: RectF,
    colour: Colour,
) {
    let transform = path.transform_to_fit((area.x, area.y, area.width, area.height));
    fill_path(pixels, key, path, transform, colour);
}

/// Upstream's icon files (`vital/icons/`) the interface draws; the logo
/// files are never used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpstreamIcon {
    Chorus,
    Compressor,
    Delay,
    Distortion,
    EffectsFilter,
    Equalizer,
    Flanger,
    Phaser,
    Reverb,
    DragDropArrows,
    Folder,
}

pub(crate) const UPSTREAM_SVGS: [&str; 11] = [
    include_str!("../../../../../vital/icons/chorus.svg"),
    include_str!("../../../../../vital/icons/compressor.svg"),
    include_str!("../../../../../vital/icons/delay.svg"),
    include_str!("../../../../../vital/icons/distortion.svg"),
    include_str!("../../../../../vital/icons/effects_filter.svg"),
    include_str!("../../../../../vital/icons/equalizer.svg"),
    include_str!("../../../../../vital/icons/flanger.svg"),
    include_str!("../../../../../vital/icons/phaser.svg"),
    include_str!("../../../../../vital/icons/reverb.svg"),
    include_str!("../../../../../vital/icons/drag_drop_arrows.svg"),
    include_str!("../../../../../vital/icons/folder.svg"),
];

impl UpstreamIcon {
    pub const ALL: [UpstreamIcon; 11] = [
        UpstreamIcon::Chorus,
        UpstreamIcon::Compressor,
        UpstreamIcon::Delay,
        UpstreamIcon::Distortion,
        UpstreamIcon::EffectsFilter,
        UpstreamIcon::Equalizer,
        UpstreamIcon::Flanger,
        UpstreamIcon::Phaser,
        UpstreamIcon::Reverb,
        UpstreamIcon::DragDropArrows,
        UpstreamIcon::Folder,
    ];

    /// The icon's outline (`Paths::chorus()` and so on).
    pub fn path(self) -> &'static Path {
        static PATHS: OnceLock<Vec<Path>> = OnceLock::new();
        let paths = PATHS.get_or_init(|| {
            UPSTREAM_SVGS
                .iter()
                .map(|svg| Path::from_svg(svg).expect("upstream's icons parse"))
                .collect()
        });
        &paths[self as usize]
    }

    /// The cache key for [`fill_path`].
    pub fn key(self) -> &'static str {
        match self {
            UpstreamIcon::Chorus => "chorus",
            UpstreamIcon::Compressor => "compressor",
            UpstreamIcon::Delay => "delay",
            UpstreamIcon::Distortion => "distortion",
            UpstreamIcon::EffectsFilter => "effects_filter",
            UpstreamIcon::Equalizer => "equalizer",
            UpstreamIcon::Flanger => "flanger",
            UpstreamIcon::Phaser => "phaser",
            UpstreamIcon::Reverb => "reverb",
            UpstreamIcon::DragDropArrows => "drag_drop_arrows",
            UpstreamIcon::Folder => "folder",
        }
    }

    /// Fills the icon scaled to fit `area`, keeping its proportions.
    pub fn fill(self, pixels: &PixelPainter, area: RectF, colour: Colour) {
        fill_path_to_fit(pixels, self.key(), self.path(), area, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_square_covers_its_pixels_exactly() {
        let mut path = Path::new();
        path.add_rectangle(1.5, 1.0, 2.0, 2.0);
        let coverage = rasterise(&path, Transform::IDENTITY, (0, 0), 5, 4);
        let at = |x: usize, y: usize| coverage[y * 5 + x];
        assert_eq!(at(0, 1), 0.0);
        assert!((at(1, 1) - 0.5).abs() < 1e-5);
        assert!((at(2, 1) - 1.0).abs() < 1e-5);
        assert!((at(3, 2) - 0.5).abs() < 1e-5);
        assert_eq!(at(2, 0), 0.0);
        assert_eq!(at(2, 3), 0.0);
    }

    #[test]
    fn non_zero_winding_fills_overlaps_and_reversed_holes() {
        // Two overlapping squares wound the same way: one filled area.
        let mut overlap = Path::new();
        overlap.add_rectangle(0.0, 0.0, 4.0, 4.0);
        overlap.add_rectangle(2.0, 0.0, 4.0, 4.0);
        let coverage = rasterise(&overlap, Transform::IDENTITY, (0, 0), 6, 4);
        assert!(coverage.iter().all(|&c| (c - 1.0).abs() < 1e-5));

        // A square inside one wound the other way (add_rectangle goes
        // clockwise on screen): a hole.
        let mut hole = Path::new();
        hole.add_rectangle(0.0, 0.0, 6.0, 6.0);
        hole.move_to(2.0, 2.0);
        hole.line_to(2.0, 4.0);
        hole.line_to(4.0, 4.0);
        hole.line_to(4.0, 2.0);
        hole.close();
        let coverage = rasterise(&hole, Transform::IDENTITY, (0, 0), 6, 6);
        assert_eq!(coverage[3 * 6 + 3], 0.0);
        assert!((coverage[6 + 1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn every_upstream_icon_has_ink() {
        for icon in UpstreamIcon::ALL {
            let path = icon.path();
            let transform = path.transform_to_fit((0.0, 0.0, 32.0, 32.0));
            let coverage = rasterise(path, transform, (0, 0), 32, 32);
            let ink: f32 = coverage.iter().sum();
            // Some of the square, never all of it.
            assert!(ink > 32.0 && ink < 0.9 * 32.0 * 32.0, "{icon:?}: {ink}");
        }
    }

    #[test]
    fn stroked_icons_are_filled_outlines() {
        // The equalizer's curves are strokes 39 units wide in a 640 square:
        // over two pixels at 32 pixels, not hairlines.
        let path = UpstreamIcon::Equalizer.path();
        let transform = path.transform_to_fit((0.0, 0.0, 32.0, 32.0));
        let coverage = rasterise(path, transform, (0, 0), 32, 32);
        let solid = coverage.iter().filter(|&&c| c > 0.99).count();
        assert!(solid > 20, "only {solid} solid pixels");
    }
}
