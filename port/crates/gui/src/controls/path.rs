// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_graphics, GPL-3.0-only).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! Filled shapes: the parts of JUCE's `Path` and `PathStrokeType` the
//! interface's icons use, and the outlines of upstream's icon files.
//!
//! Upstream builds icons as `juce::Path`s, either in code (`Paths::clock`,
//! `Paths::note`, the preset arrows in `look_and_feel/paths.h`) or from the
//! SVG files in `vital/icons/` (`Paths::fromSvgData`, which takes the
//! drawable's outline), and fills them with the non-zero winding rule. A
//! [`Path`] keeps the same outlines, with curves flattened, for
//! [`super::icon`] to rasterise.

use std::f32::consts::PI;

/// `Path::addEllipse`'s control point distance for a quarter circle.
const ELLIPSE_KAPPA: f32 = 0.552_284_8;
/// `Path::addCentredArc`'s step between line segments, in radians.
const ARC_ANGLE_INCREMENT: f32 = 0.05;
/// Line segments per cubic curve.
const CURVE_SEGMENTS: usize = 16;

/// One subpath's outline, in path coordinates.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubPath {
    pub points: Vec<(f32, f32)>,
    pub closed: bool,
}

/// `juce::Path`: subpaths of straight segments (curves flattened), filled
/// with the non-zero winding rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    subpaths: Vec<SubPath>,
    /// `Path::getBounds`: every point added, curve control points included.
    bounds: Option<(f32, f32, f32, f32)>,
}

impl Path {
    pub fn new() -> Path {
        Path::default()
    }

    pub fn subpaths(&self) -> &[SubPath] {
        &self.subpaths
    }

    /// `Path::getBounds` as (left, top, right, bottom).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        self.bounds
    }

    fn extend(&mut self, x: f32, y: f32) {
        self.bounds = Some(match self.bounds {
            None => (x, y, x, y),
            Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
        });
    }

    fn current(&mut self) -> &mut SubPath {
        if self.subpaths.last().is_none_or(|subpath| subpath.closed) {
            self.subpaths.push(SubPath::default());
        }
        self.subpaths.last_mut().expect("just pushed")
    }

    /// `Path::startNewSubPath`.
    pub fn move_to(&mut self, x: f32, y: f32) {
        if self
            .subpaths
            .last()
            .is_some_and(|subpath| !subpath.closed && subpath.points.len() <= 1)
        {
            self.subpaths.pop();
        }
        self.subpaths.push(SubPath {
            points: vec![(x, y)],
            closed: false,
        });
        self.extend(x, y);
    }

    /// Before drawing on: `Path::lineTo` and `cubicTo` on an empty path
    /// start at (0, 0); after `closeSubPath` they go on from the closed
    /// sub-path's start, as JUCE's flattening does.
    fn continue_from_close(&mut self) {
        match self.subpaths.last() {
            None => self.move_to(0.0, 0.0),
            Some(subpath) if subpath.closed => {
                let start = subpath.points[0];
                self.subpaths.push(SubPath {
                    points: vec![start],
                    closed: false,
                });
            }
            Some(_) => {}
        }
    }

    /// `Path::lineTo`.
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.continue_from_close();
        self.current().points.push((x, y));
        self.extend(x, y);
    }

    /// `Path::cubicTo`, flattened.
    pub fn cubic_to(&mut self, c1: (f32, f32), c2: (f32, f32), end: (f32, f32)) {
        self.continue_from_close();
        let start = *self.current().points.last().expect("a subpath has a point");
        for i in 1..=CURVE_SEGMENTS {
            let t = i as f32 / CURVE_SEGMENTS as f32;
            let u = 1.0 - t;
            let a = u * u * u;
            let b = 3.0 * u * u * t;
            let c = 3.0 * u * t * t;
            let d = t * t * t;
            let point = (
                a * start.0 + b * c1.0 + c * c2.0 + d * end.0,
                a * start.1 + b * c1.1 + c * c2.1 + d * end.1,
            );
            self.current().points.push(point);
        }
        for (x, y) in [c1, c2, end] {
            self.extend(x, y);
        }
    }

    /// `Path::closeSubPath`.
    pub fn close(&mut self) {
        if let Some(subpath) = self.subpaths.last_mut() {
            subpath.closed = true;
        }
    }

    /// `Path::addRectangle`: clockwise from the bottom left corner.
    pub fn add_rectangle(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let (x1, x2) = (x.min(x + width), x.max(x + width));
        let (y1, y2) = (y.min(y + height), y.max(y + height));
        self.move_to(x1, y2);
        self.line_to(x1, y1);
        self.line_to(x2, y1);
        self.line_to(x2, y2);
        self.close();
    }

    /// `Path::addEllipse`: four cubic curves, clockwise from the top.
    pub fn add_ellipse(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let (hw, hh) = (width * 0.5, height * 0.5);
        let (hw55, hh55) = (hw * ELLIPSE_KAPPA, hh * ELLIPSE_KAPPA);
        let (cx, cy) = (x + hw, y + hh);
        self.move_to(cx, cy - hh);
        self.cubic_to((cx + hw55, cy - hh), (cx + hw, cy - hh55), (cx + hw, cy));
        self.cubic_to((cx + hw, cy + hh55), (cx + hw55, cy + hh), (cx, cy + hh));
        self.cubic_to((cx - hw55, cy + hh), (cx - hw, cy + hh55), (cx - hw, cy));
        self.cubic_to((cx - hw, cy - hh55), (cx - hw55, cy - hh), (cx, cy - hh));
        self.close();
    }

    /// `Path::addCentredArc` (no rotation), continuing the current subpath:
    /// angles are clockwise from twelve o'clock.
    fn add_centred_arc(&mut self, centre: (f32, f32), radius: (f32, f32), from: f32, to: f32) {
        if radius.0 <= 0.0 || radius.1 <= 0.0 {
            return;
        }
        let point = |angle: f32| {
            (
                centre.0 + radius.0 * angle.sin(),
                centre.1 - radius.1 * angle.cos(),
            )
        };
        let mut angle = from;
        if from < to {
            while angle < to {
                let (x, y) = point(angle);
                self.line_to(x, y);
                angle += ARC_ANGLE_INCREMENT;
            }
        } else {
            while angle > to {
                let (x, y) = point(angle);
                self.line_to(x, y);
                angle -= ARC_ANGLE_INCREMENT;
            }
        }
        let (x, y) = point(to);
        self.line_to(x, y);
    }

    /// `Path::addPieSegment`: a slice of the ellipse in the rectangle, with
    /// a hole `inner` times its size.
    #[allow(clippy::too_many_arguments)]
    pub fn add_pie_segment(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        from: f32,
        to: f32,
        inner: f32,
    ) {
        let (mut rx, mut ry) = (width / 2.0, height / 2.0);
        let centre = (x + rx, y + ry);
        self.move_to(centre.0 + rx * from.sin(), centre.1 - ry * from.cos());
        self.add_centred_arc(centre, (rx, ry), from, to);
        if (from - to).abs() > PI * 1.999 {
            self.close();
            if inner > 0.0 {
                rx *= inner;
                ry *= inner;
                self.move_to(centre.0 + rx * to.sin(), centre.1 - ry * to.cos());
                self.add_centred_arc(centre, (rx, ry), to, from);
            }
        } else if inner > 0.0 {
            rx *= inner;
            ry *= inner;
            self.add_centred_arc(centre, (rx, ry), to, from);
        } else {
            self.line_to(centre.0, centre.1);
        }
        self.close();
    }

    /// `Path::addLineSegment`: a rectangle `thickness` wide around the line.
    /// A zero-length line adds no area, only its point to the bounds (as
    /// upstream uses it to pad icons to the unit square).
    pub fn add_line_segment(&mut self, start: (f32, f32), end: (f32, f32), thickness: f32) {
        let half = thickness * 0.5;
        let along = |from: (f32, f32), to: (f32, f32), perpendicular: f32| {
            // Line::getPointAlongLine(0, perpendicular).
            let (dx, dy) = (to.0 - from.0, to.1 - from.1);
            let length = (dx * dx + dy * dy).sqrt();
            if length <= 0.0 {
                return from;
            }
            (
                from.0 - perpendicular * dy / length,
                from.1 + perpendicular * dx / length,
            )
        };
        let p1 = along(start, end, half);
        self.move_to(p1.0, p1.1);
        let p2 = along(start, end, -half);
        self.line_to(p2.0, p2.1);
        let p3 = along(end, start, half);
        self.line_to(p3.0, p3.1);
        let p4 = along(end, start, -half);
        self.line_to(p4.0, p4.1);
        self.close();
    }

    /// `Path::addPath`.
    pub fn add_path(&mut self, other: &Path) {
        self.subpaths.extend(other.subpaths.iter().cloned());
        if let Some((l, t, r, b)) = other.bounds {
            self.extend(l, t);
            self.extend(r, b);
        }
    }

    /// `PathStrokeType(thickness, joints, ends).createStrokedPath`: the
    /// outline of a stroke `thickness` wide, as pieces that are each wound
    /// the same way, so their non-zero fill is the stroke.
    ///
    /// # Panics
    /// For the styles not ported yet, bevelled joints and square ends.
    /// Mitred and curved joints, and butt and rounded ends, are.
    pub fn stroked(&self, thickness: f32, joints: JointStyle, ends: EndCapStyle) -> Path {
        assert!(
            joints != JointStyle::Beveled && ends != EndCapStyle::Square,
            "PathStrokeType {joints:?} joints with {ends:?} ends are not ported yet"
        );
        let curved = joints == JointStyle::Curved;
        let rounded = ends == EndCapStyle::Rounded;
        let half = thickness / 2.0;
        // PathStrokeType's maxMiterExtensionSquared (9 × thickness²).
        let max_miter_squared = 9.0 * thickness * thickness;
        let mut out = Path::new();
        for subpath in &self.subpaths {
            let mut points = subpath.points.clone();
            points.dedup();
            if subpath.closed && points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            if points.len() < 2 {
                continue;
            }
            let count = points.len();
            let segments = if subpath.closed { count } else { count - 1 };
            let offset = |a: (f32, f32), b: (f32, f32)| {
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let length = (dx * dx + dy * dy).sqrt();
                (-dy / length * half, dx / length * half)
            };
            for i in 0..segments {
                let (a, b) = (points[i], points[(i + 1) % count]);
                let (ox, oy) = offset(a, b);
                out.add_polygon(&[
                    (a.0 + ox, a.1 + oy),
                    (b.0 + ox, b.1 + oy),
                    (b.0 - ox, b.1 - oy),
                    (a.0 - ox, a.1 - oy),
                ]);
            }
            if rounded && !subpath.closed {
                // EndCapStyle::rounded: a half disc past each end, which
                // under the non-zero fill is a whole disc at the end.
                out.add_disc(points[0], half);
                out.add_disc(points[count - 1], half);
            }
            let joints = if subpath.closed { count } else { count - 2 };
            for j in 0..joints {
                let index = if subpath.closed { j } else { j + 1 };
                if curved {
                    // JointStyle::curved: an arc round the outer side of
                    // the turn, which with the segments' overlap is a disc
                    // at the corner.
                    out.add_disc(points[index], half);
                    continue;
                }
                let previous = points[(index + count - 1) % count];
                let corner = points[index];
                let next = points[(index + 1) % count];
                let (o1x, o1y) = offset(previous, corner);
                let (o2x, o2y) = offset(corner, next);
                // The outer side of the turn.
                let cross = (corner.0 - previous.0) * (next.1 - corner.1)
                    - (corner.1 - previous.1) * (next.0 - corner.0);
                let sign = if cross > 0.0 { -1.0 } else { 1.0 };
                let e1 = (corner.0 + sign * o1x, corner.1 + sign * o1y);
                let e2 = (corner.0 + sign * o2x, corner.1 + sign * o2y);
                let mut joint = vec![corner, e1];
                // Where the two outer edges meet, if close enough.
                let d1 = (corner.0 - previous.0, corner.1 - previous.1);
                let d2 = (next.0 - corner.0, next.1 - corner.1);
                let denominator = d1.0 * d2.1 - d1.1 * d2.0;
                if denominator.abs() > 1e-9 {
                    let t = ((e2.0 - e1.0) * d2.1 - (e2.1 - e1.1) * d2.0) / denominator;
                    let miter = (e1.0 + t * d1.0, e1.1 + t * d1.1);
                    let beyond = (miter.0 - e1.0).powi(2) + (miter.1 - e1.1).powi(2);
                    if t > 0.0 && beyond < max_miter_squared {
                        joint.push(miter);
                    }
                }
                joint.push(e2);
                out.add_polygon(&joint);
            }
        }
        out
    }

    /// A disc of `radius` at `centre`, flattened like the arcs.
    fn add_disc(&mut self, centre: (f32, f32), radius: f32) {
        if radius <= 0.0 {
            return;
        }
        let steps = ((2.0 * PI / ARC_ANGLE_INCREMENT) as usize).max(8);
        let points: Vec<(f32, f32)> = (0..steps)
            .map(|i| {
                let angle = 2.0 * PI * i as f32 / steps as f32;
                (
                    centre.0 + radius * angle.sin(),
                    centre.1 - radius * angle.cos(),
                )
            })
            .collect();
        self.add_polygon(&points);
    }

    /// A closed polygon, wound clockwise on screen whatever its order.
    fn add_polygon(&mut self, points: &[(f32, f32)]) {
        let area: f32 = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
            .sum();
        let mut ordered = points.to_vec();
        if area < 0.0 {
            ordered.reverse();
        }
        self.move_to(ordered[0].0, ordered[0].1);
        for &(x, y) in &ordered[1..] {
            self.line_to(x, y);
        }
        self.close();
    }

    /// `Path::getTransformToScaleToFit(area, true)` (centred): the scale
    /// and offset that fit the bounds into `area` (x, y, width, height)
    /// keeping their proportions.
    pub fn transform_to_fit(&self, area: (f32, f32, f32, f32)) -> Transform {
        let Some((l, t, r, b)) = self.bounds else {
            return Transform::IDENTITY;
        };
        let (width, height) = (r - l, b - t);
        if width <= 0.0 || height <= 0.0 {
            return Transform::IDENTITY;
        }
        let scale = (area.2 / width).min(area.3 / height);
        Transform {
            scale,
            dx: area.0 + (area.2 - width * scale) / 2.0 - l * scale,
            dy: area.1 + (area.3 - height * scale) / 2.0 - t * scale,
        }
    }

    /// `Drawable::createFromImageData(svg)->getOutlineAsPath()` for
    /// upstream's icon files: each `<path>` (directly or through `<use>`),
    /// stroked if it has a visible stroke, else as it is.
    pub fn from_svg(svg: &str) -> Result<Path, String> {
        let mut path = Path::new();
        for element in elements(svg) {
            let shape = match element.name {
                "path" if attribute(element.text, "id").is_none() => element.text,
                "use" => {
                    let href = attribute(element.text, "xlink:href")
                        .or_else(|| attribute(element.text, "href"))
                        .ok_or("a <use> without an href")?;
                    let id = href.strip_prefix('#').ok_or("a <use> outside the file")?;
                    elements(svg)
                        .find(|e| e.name == "path" && attribute(e.text, "id") == Some(id))
                        .ok_or_else(|| format!("no path #{id}"))?
                        .text
                }
                _ => continue,
            };
            let data = attribute(shape, "d").ok_or("a <path> without data")?;
            let outline = parse_path_data(data)?;
            // DrawableShape::getOutlineAsPath: the stroke's outline if the
            // stroke shows, else the path. The <use> carries the styling.
            let style = element.text;
            let stroke = attribute(style, "stroke").filter(|s| *s != "none");
            let width: f32 = attribute(style, "stroke-width")
                .map(|w| w.parse().map_err(|_| format!("bad stroke width {w}")))
                .transpose()?
                .unwrap_or(1.0);
            let opacity = |name: &str| -> f32 {
                attribute(style, name)
                    .and_then(|o| o.parse().ok())
                    .unwrap_or(1.0)
            };
            // The stroke's colour carries stroke-opacity times the element's
            // opacity; an invisible one leaves the path itself.
            let stroke_alpha = opacity("stroke-opacity") * opacity("opacity");
            if stroke.is_some() && width > 0.0 && stroke_alpha > 0.0 {
                // SVG's default joints and ends; upstream's icons set no
                // others.
                path.add_path(&outline.stroked(width, JointStyle::Mitered, EndCapStyle::Butt));
            } else {
                path.add_path(&outline);
            }
        }
        Ok(path)
    }
}

/// `PathStrokeType::JointStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointStyle {
    Mitered,
    Curved,
    Beveled,
}

/// `PathStrokeType::EndCapStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndCapStyle {
    Butt,
    Square,
    Rounded,
}

/// A uniform scale then an offset, path coordinates to pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub scale: f32,
    pub dx: f32,
    pub dy: f32,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        scale: 1.0,
        dx: 0.0,
        dy: 0.0,
    };

    pub fn apply(&self, (x, y): (f32, f32)) -> (f32, f32) {
        (x * self.scale + self.dx, y * self.scale + self.dy)
    }
}

struct Element<'a> {
    name: &'a str,
    /// The whole start tag.
    text: &'a str,
}

/// The start tags in `svg`, in order.
fn elements(svg: &str) -> impl Iterator<Item = Element<'_>> {
    svg.match_indices('<').filter_map(move |(start, _)| {
        let rest = &svg[start + 1..];
        if rest.starts_with(['/', '?', '!']) {
            return None;
        }
        let end = rest.find('>')?;
        let text = &rest[..end];
        let name = text.split(|c: char| c.is_whitespace() || c == '/').next()?;
        Some(Element { name, text })
    })
}

/// The value of `name="…"` in a start tag.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut search = tag;
    while let Some(index) = search.find(name) {
        let before = search[..index].chars().last();
        let after = &search[index + name.len()..];
        if before.is_some_and(char::is_whitespace) {
            if let Some(value) = after.strip_prefix("=\"") {
                return value.split('"').next();
            }
        }
        search = &search[index + name.len()..];
    }
    None
}

/// SVG path data with M, L, C and Z (absolute), as upstream's icons use.
fn parse_path_data(data: &str) -> Result<Path, String> {
    let mut path = Path::new();
    let mut numbers = Vec::new();
    let mut command = None;
    let flush = |path: &mut Path, command: Option<char>, numbers: &mut Vec<f32>| {
        let Some(command) = command else {
            return Ok(());
        };
        let arity = match command {
            'M' | 'L' => 2,
            'C' => 6,
            'Z' | 'z' => 0,
            other => return Err(format!("unsupported path command {other}")),
        };
        if arity == 0 {
            path.close();
        } else {
            if numbers.is_empty() || !numbers.len().is_multiple_of(arity) {
                return Err(format!("{command} takes {arity} numbers"));
            }
            for (i, chunk) in numbers.chunks(arity).enumerate() {
                match (command, i) {
                    ('M', 0) => path.move_to(chunk[0], chunk[1]),
                    ('M' | 'L', _) => path.line_to(chunk[0], chunk[1]),
                    _ => path.cubic_to(
                        (chunk[0], chunk[1]),
                        (chunk[2], chunk[3]),
                        (chunk[4], chunk[5]),
                    ),
                }
            }
        }
        numbers.clear();
        Ok(())
    };
    let mut token = String::new();
    for c in data.chars().chain(std::iter::once(' ')) {
        let starts_number = c == '-' && !token.is_empty() && !token.ends_with(['e', 'E']);
        if c.is_ascii_digit() || c == '.' || (c == '-' && !starts_number) || c == 'e' {
            token.push(c);
            continue;
        }
        if !token.is_empty() {
            numbers.push(token.parse().map_err(|_| format!("bad number {token}"))?);
            token.clear();
        }
        if starts_number {
            token.push(c);
        } else if c.is_ascii_alphabetic() {
            flush(&mut path, command, &mut numbers)?;
            command = Some(c);
        }
    }
    flush(&mut path, command, &mut numbers)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_length_lines_only_pad_the_bounds() {
        let mut path = Path::new();
        path.add_rectangle(0.4, 0.4, 0.2, 0.2);
        path.add_line_segment((0.0, 0.0), (0.0, 0.0), 0.2);
        path.add_line_segment((1.0, 1.0), (1.0, 1.0), 0.2);
        assert_eq!(path.bounds(), Some((0.0, 0.0, 1.0, 1.0)));
        let fit = path.transform_to_fit((10.0, 0.0, 40.0, 20.0));
        assert_eq!(
            fit,
            Transform {
                scale: 20.0,
                dx: 20.0,
                dy: 0.0
            }
        );
    }

    #[test]
    fn curves_count_their_control_points_in_the_bounds() {
        let mut path = Path::new();
        path.move_to(0.0, 0.0);
        path.cubic_to((0.0, 10.0), (10.0, 10.0), (10.0, 0.0));
        assert_eq!(path.bounds(), Some((0.0, 0.0, 10.0, 10.0)));
        assert_eq!(path.subpaths()[0].points.len(), CURVE_SEGMENTS + 1);
    }

    #[test]
    fn svg_paths_resolve_through_use() {
        let svg = r##"<svg viewBox="0 0 10 10"><defs><path d="M0 0L10 0L10 10Z" id="a"></path>
            <path d="M1 1C2 2 3 3 4 4L5 5" id="b"></path></defs>
            <g><use xlink:href="#a" fill="#000000" fill-opacity="1"></use></g>
            <g><use xlink:href="#b" fill-opacity="0" stroke="#000000" stroke-width="2"></use></g></svg>"##;
        let path = Path::from_svg(svg).unwrap();
        assert_eq!(
            path.subpaths()[0].points,
            vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]
        );
        assert!(path.subpaths()[0].closed);
        // The stroked path: pieces 2 wide around the line.
        assert!(path.subpaths().len() > 2);
        let (_, top, _, _) = path.bounds().unwrap();
        assert_eq!(top, 0.0);
    }

    #[test]
    fn upstream_icons_parse() {
        for svg in super::super::icon::UPSTREAM_SVGS {
            let path = Path::from_svg(svg).unwrap();
            assert!(!path.subpaths().is_empty());
        }
    }

    #[test]
    fn strokes_are_wound_one_way() {
        let mut line = Path::new();
        line.move_to(0.0, 0.0);
        line.line_to(10.0, 0.0);
        line.line_to(10.0, 10.0);
        let stroke = line.stroked(2.0, JointStyle::Mitered, EndCapStyle::Butt);
        for subpath in stroke.subpaths() {
            let points = &subpath.points;
            let area: f32 = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
                .sum();
            assert!(area >= 0.0);
        }
        // The outer corner is mitred out to (11, -1).
        assert_eq!(stroke.bounds(), Some((0.0, -1.0, 11.0, 10.0)));
    }

    #[test]
    #[should_panic(expected = "not ported yet")]
    fn unported_stroke_styles_refuse() {
        let mut line = Path::new();
        line.move_to(0.0, 0.0);
        line.line_to(10.0, 0.0);
        let _ = line.stroked(2.0, JointStyle::Beveled, EndCapStyle::Square);
    }

    /// Curved joints and rounded ends (`Paths::createFilterStroke`): the
    /// stroke reaches half its thickness past each end and round the outer
    /// side of each turn, never further.
    #[test]
    fn curved_joints_and_rounded_ends_stay_within_half_the_thickness() {
        let mut line = Path::new();
        line.move_to(0.0, 0.0);
        line.line_to(10.0, 0.0);
        line.line_to(10.0, 10.0);
        let stroke = line.stroked(2.0, JointStyle::Curved, EndCapStyle::Rounded);
        let (l, t, r, b) = stroke.bounds().unwrap();
        // Rounded ends reach 1 past (0, 0) and (10, 10).
        assert!((l + 1.0).abs() < 1e-3 && (t + 1.0).abs() < 1e-3);
        assert!((r - 11.0).abs() < 1e-3 && (b - 11.0).abs() < 1e-3);
        // The curved corner stays within 1 of (10, 0), where a mitre would
        // reach (11, -1), √2 away.
        let farthest = stroke
            .subpaths()
            .iter()
            .flat_map(|subpath| subpath.points.iter())
            .map(|&(x, y)| ((x - 10.0).powi(2) + y.powi(2)).sqrt())
            .filter(|&distance| distance < 2.0)
            .fold(0.0f32, f32::max);
        assert!(farthest <= 1.0 + 1e-3, "{farthest}");
        // Every piece is wound the same way.
        for subpath in stroke.subpaths() {
            let points = &subpath.points;
            let area: f32 = points
                .iter()
                .zip(points.iter().cycle().skip(1))
                .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
                .sum();
            assert!(area >= 0.0);
        }
    }

    /// JUCE's flattening goes on from a closed sub-path's start, so a line
    /// after `closeSubPath` neither starts at nor bounds (0, 0).
    #[test]
    fn lines_after_close_go_on_from_the_subpath_start() {
        let mut path = Path::new();
        path.move_to(5.0, 5.0);
        path.line_to(8.0, 5.0);
        path.line_to(8.0, 8.0);
        path.close();
        path.line_to(5.0, 9.0);
        assert_eq!(path.bounds(), Some((5.0, 5.0, 8.0, 9.0)));
        assert_eq!(path.subpaths()[1].points, vec![(5.0, 5.0), (5.0, 9.0)]);
    }

    /// `Path::addCentredArc` adds nothing without a positive radius.
    #[test]
    fn arcs_need_a_radius() {
        let mut path = Path::new();
        path.move_to(1.0, 1.0);
        path.add_centred_arc((1.0, 1.0), (0.0, 2.0), 0.0, PI);
        assert_eq!(path.subpaths()[0].points, vec![(1.0, 1.0)]);
    }

    /// A stroke that its element's opacity hides leaves the path itself
    /// (`DrawableShape::getOutlineAsPath` on an invisible stroke).
    #[test]
    fn hidden_strokes_leave_the_path() {
        let svg = |opacity: &str| {
            format!(
                r##"<svg><defs><path id="a" d="M 0 0 L 10 0"></path></defs>
                <use xlink:href="#a" stroke="#000000" stroke-width="2" opacity="{opacity}"></use></svg>"##
            )
        };
        let shown = Path::from_svg(&svg("1")).unwrap();
        assert_eq!(shown.bounds(), Some((0.0, -1.0, 10.0, 1.0)));
        let hidden = Path::from_svg(&svg("0")).unwrap();
        assert_eq!(hidden.bounds(), Some((0.0, 0.0, 10.0, 0.0)));
    }
}
