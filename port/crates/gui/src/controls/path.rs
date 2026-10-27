// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_graphics, GPL-3.0-only; juce_gui_basics, GPL-3.0-only).
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
    /// `Path::addStar`: a closed star of `points` points round `centre`,
    /// alternating `outer` and `inner` radii, the first outer point straight
    /// up.
    pub fn add_star(&mut self, centre: (f32, f32), points: usize, inner: f32, outer: f32) {
        let between = std::f32::consts::TAU / points as f32;
        for i in 0..points * 2 {
            let angle = i as f32 * between * 0.5;
            let radius = if i & 1 == 1 { inner } else { outer };
            let (x, y) = (
                centre.0 + radius * angle.sin(),
                centre.1 - radius * angle.cos(),
            );
            if i == 0 {
                self.move_to(x, y);
            } else {
                self.line_to(x, y);
            }
        }
        self.close();
    }

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

    /// The dashes `PathStrokeType::createDashedStroke` strokes: the path cut
    /// into lengths alternating on and off (`dash_lengths`, repeating),
    /// along its segments as `PathFlatteningIterator` walks them, a closed
    /// subpath ending with its closing segment. As in JUCE, a dash running
    /// off the end of an open subpath goes on into the next. Stroke the
    /// result to draw it; no positive length gives an empty path.
    pub fn dashed(&self, dash_lengths: &[f32]) -> Path {
        let mut out = Path::new();
        if !dash_lengths.iter().any(|&length| length > 0.0) {
            return out;
        }
        let mut segments = Vec::new();
        for subpath in &self.subpaths {
            for pair in subpath.points.windows(2) {
                segments.push((pair[0], pair[1], false));
            }
            if let (true, Some(&first), Some(&last)) = (
                subpath.closed,
                subpath.points.first(),
                subpath.points.last(),
            ) {
                if first != last {
                    segments.push((last, first, true));
                }
            }
        }
        let mut segments = segments.into_iter();
        let mut first = true;
        let mut dash = 0usize;
        let (mut position, mut line_length, mut line_end) = (0.0f32, 0.0f32, 0.0f32);
        let (mut start, mut end) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
        loop {
            let solid = dash.is_multiple_of(2);
            let length = dash_lengths[dash % dash_lengths.len()];
            dash += 1;
            if length <= 0.0 {
                continue;
            }
            position += length;
            while position > line_end {
                let Some((from, to, closes)) = segments.next() else {
                    if solid && !first {
                        out.line_to(end.0, end.1);
                    }
                    return out;
                };
                if solid && !first {
                    out.line_to(from.0, from.1);
                } else {
                    out.move_to(from.0, from.1);
                }
                (start, end) = (from, to);
                line_length = (to.0 - from.0).hypot(to.1 - from.1);
                line_end += line_length;
                first = closes;
            }
            let alpha = (position - (line_end - line_length)) / line_length;
            let (x, y) = (
                start.0 + (end.0 - start.0) * alpha,
                start.1 + (end.1 - start.1) * alpha,
            );
            if solid {
                out.line_to(x, y);
            } else {
                out.move_to(x, y);
            }
        }
    }

    /// `PathStrokeType(thickness, joints, ends).createStrokedPath`: the
    /// outline of a stroke `thickness` wide, as pieces that are each wound
    /// the same way, so their non-zero fill is the stroke. Curved joints and
    /// rounded ends are discs at the vertices, which with the segments make
    /// exactly the round-joined, round-capped stroke.
    pub fn stroked(&self, thickness: f32, joints: JointStyle, ends: EndCapStyle) -> Path {
        self.stroked_with_arrowheads(thickness, joints, ends, (0.0, 0.0), (0.0, 0.0))
    }

    /// `PathStrokeType::createStrokeWithArrowheads`: each open subpath is
    /// shortened by the arrowheads' lengths (`start` and `end`, each a
    /// width and a length; a zero length has none and takes the end
    /// style), stroked, and capped with a triangle as wide as the
    /// arrowhead whose tip is the subpath's original end.
    pub fn stroked_with_arrowheads(
        &self,
        thickness: f32,
        joints: JointStyle,
        ends: EndCapStyle,
        start: (f32, f32),
        end: (f32, f32),
    ) -> Path {
        let half = thickness / 2.0;
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
            if subpath.closed {
                out.stroke_points(&points, true, half, joints, None, None);
                continue;
            }
            let tips = (points[0], points[points.len() - 1]);
            let points = shorten(&points, start.1, end.1);
            if points.len() < 2 {
                continue;
            }
            let start_cap = (start.1 <= 0.0).then_some(ends);
            let end_cap = (end.1 <= 0.0).then_some(ends);
            out.stroke_points(&points, false, half, joints, start_cap, end_cap);
            if start.1 > 0.0 {
                out.add_arrowhead(points[1], points[0], tips.0, start.0);
            }
            if end.1 > 0.0 {
                let n = points.len();
                out.add_arrowhead(points[n - 2], points[n - 1], tips.1, end.0);
            }
        }
        out
    }

    /// One subpath's stroke: a quad per segment, its joints, and its caps
    /// (None where an arrowhead or a closed path has none).
    fn stroke_points(
        &mut self,
        points: &[(f32, f32)],
        closed: bool,
        half: f32,
        joints: JointStyle,
        start_cap: Option<EndCapStyle>,
        end_cap: Option<EndCapStyle>,
    ) {
        // PathStrokeType's maxMiterExtensionSquared (9 × thickness²).
        let max_miter_squared = 9.0 * (2.0 * half) * (2.0 * half);
        let count = points.len();
        let segments = if closed { count } else { count - 1 };
        let offset = |a: (f32, f32), b: (f32, f32)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let length = (dx * dx + dy * dy).sqrt();
            (-dy / length * half, dx / length * half)
        };
        for i in 0..segments {
            let (a, b) = (points[i], points[(i + 1) % count]);
            let (ox, oy) = offset(a, b);
            self.add_polygon(&[
                (a.0 + ox, a.1 + oy),
                (b.0 + ox, b.1 + oy),
                (b.0 - ox, b.1 - oy),
                (a.0 - ox, a.1 - oy),
            ]);
        }
        let joint_count = if closed { count } else { count - 2 };
        for j in 0..joint_count {
            let index = if closed { j } else { j + 1 };
            let previous = points[(index + count - 1) % count];
            let corner = points[index];
            let next = points[(index + 1) % count];
            if joints == JointStyle::Curved {
                self.add_disc(corner, half);
                continue;
            }
            let (o1x, o1y) = offset(previous, corner);
            let (o2x, o2y) = offset(corner, next);
            // The outer side of the turn.
            let cross = (corner.0 - previous.0) * (next.1 - corner.1)
                - (corner.1 - previous.1) * (next.0 - corner.0);
            let sign = if cross > 0.0 { -1.0 } else { 1.0 };
            let e1 = (corner.0 + sign * o1x, corner.1 + sign * o1y);
            let e2 = (corner.0 + sign * o2x, corner.1 + sign * o2y);
            let mut joint = vec![corner, e1];
            if joints == JointStyle::Mitered {
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
            }
            joint.push(e2);
            self.add_polygon(&joint);
        }
        let caps = [
            (start_cap, points[0], points[1]),
            (end_cap, points[count - 1], points[count - 2]),
        ];
        for (cap, at, inward) in caps {
            match cap {
                Some(EndCapStyle::Rounded) => self.add_disc(at, half),
                Some(EndCapStyle::Square) => {
                    let (ox, oy) = offset(inward, at);
                    let (dx, dy) = (oy, -ox);
                    self.add_polygon(&[
                        (at.0 + ox, at.1 + oy),
                        (at.0 + ox + dx, at.1 + oy + dy),
                        (at.0 - ox + dx, at.1 - oy + dy),
                        (at.0 - ox, at.1 - oy),
                    ]);
                }
                Some(EndCapStyle::Butt) | None => {}
            }
        }
    }

    /// `PathStrokeHelpers::addArrowhead`: a triangle `width` wide across
    /// the line from `from` to its shortened end `base`, tipped at `tip`.
    fn add_arrowhead(&mut self, from: (f32, f32), base: (f32, f32), tip: (f32, f32), width: f32) {
        let (dx, dy) = (base.0 - from.0, base.1 - from.1);
        let length = (dx * dx + dy * dy).sqrt();
        if length <= 0.0 {
            return;
        }
        let (px, py) = (-dy / length * width / 2.0, dx / length * width / 2.0);
        self.add_polygon(&[(base.0 + px, base.1 + py), tip, (base.0 - px, base.1 - py)]);
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

    /// `Path::addCentredArc(cx, cy, rx, ry, 0, from, to, true)`: the arc as
    /// a new subpath, angles clockwise from twelve o'clock.
    #[allow(clippy::too_many_arguments)]
    pub fn centred_arc(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, from: f32, to: f32) {
        self.move_to(cx + rx * from.sin(), cy - ry * from.cos());
        self.add_centred_arc((cx, cy), (rx, ry), from, to);
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

/// `PathStrokeHelpers::shortenSubPath`: `points` with `start` and `end`
/// cut off along its length.
fn shorten(points: &[(f32, f32)], start: f32, end: f32) -> Vec<(f32, f32)> {
    let mut points = points.to_vec();
    let cut = |points: &mut Vec<(f32, f32)>, mut amount: f32| {
        while amount > 0.0 && points.len() >= 2 {
            let (a, b) = (points[0], points[1]);
            let length = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
            if length > amount {
                let t = amount / length;
                points[0] = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
                return;
            }
            amount -= length;
            points.remove(0);
        }
    };
    cut(&mut points, start);
    points.reverse();
    cut(&mut points, end);
    points.reverse();
    points
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

/// SVG path data's numbers and commands. A number ends where another sign,
/// a second decimal point or a letter starts (`-1.1.9` is -1.1 and 0.9).
fn path_tokens(data: &str) -> Result<Vec<PathToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = data.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            tokens.push(PathToken::Command(c));
            chars.next();
        } else if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            let mut number = String::new();
            let (mut dot, mut exponent) = (false, false);
            while let Some(&c) = chars.peek() {
                let sign_ok = number.is_empty() || number.ends_with(['e', 'E']);
                if c.is_ascii_digit()
                    || ((c == '-' || c == '+') && sign_ok)
                    || (c == '.' && !dot && !exponent)
                    || ((c == 'e' || c == 'E') && !exponent && !number.is_empty())
                {
                    dot |= c == '.';
                    exponent |= c == 'e' || c == 'E';
                    number.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            let value = number.parse().map_err(|_| format!("bad number {number}"))?;
            tokens.push(PathToken::Number(value));
        } else {
            chars.next();
        }
    }
    Ok(tokens)
}

enum PathToken {
    Command(char),
    Number(f32),
}

/// SVG path data (`Drawable::parseSVGPath`): absolute and relative M, L,
/// H, V, C, S, A and Z. Arcs are drawn as `Path::addCentredArc` draws them,
/// from their endpoint form (the SVG spec's F.6.5); upstream's icons don't
/// rotate their ellipses.
fn parse_path_data(data: &str) -> Result<Path, String> {
    let tokens = path_tokens(data)?;
    let mut path = Path::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut start_x, mut start_y) = (0.0f32, 0.0f32);
    // The last cubic's second control point, for S.
    let mut last_control: Option<(f32, f32)> = None;
    let mut index = 0;
    let mut command = None;
    while index < tokens.len() {
        if let PathToken::Command(c) = tokens[index] {
            command = Some(c);
            index += 1;
            if c == 'Z' || c == 'z' {
                path.close();
                (x, y) = (start_x, start_y);
                last_control = None;
                continue;
            }
        }
        let Some(c) = command else {
            return Err("path data starts with a number".to_string());
        };
        let arity = match c.to_ascii_uppercase() {
            'M' | 'L' => 2,
            'H' | 'V' => 1,
            'C' => 6,
            'S' => 4,
            'A' => 7,
            _ => return Err(format!("unsupported path command {c}")),
        };
        let mut n = [0.0f32; 7];
        for value in n.iter_mut().take(arity) {
            match tokens.get(index) {
                Some(PathToken::Number(number)) => *value = *number,
                _ => return Err(format!("{c} takes {arity} numbers")),
            }
            index += 1;
        }
        let relative = c.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        let mut control = None;
        match c.to_ascii_uppercase() {
            'M' => {
                (x, y) = (ox + n[0], oy + n[1]);
                path.move_to(x, y);
                (start_x, start_y) = (x, y);
                // Further pairs are lines.
                command = Some(if relative { 'l' } else { 'L' });
            }
            'L' => {
                (x, y) = (ox + n[0], oy + n[1]);
                path.line_to(x, y);
            }
            'H' => {
                x = ox + n[0];
                path.line_to(x, y);
            }
            'V' => {
                y = oy + n[0];
                path.line_to(x, y);
            }
            'C' => {
                let c1 = (ox + n[0], oy + n[1]);
                let c2 = (ox + n[2], oy + n[3]);
                (x, y) = (ox + n[4], oy + n[5]);
                path.cubic_to(c1, c2, (x, y));
                control = Some(c2);
            }
            'S' => {
                let c1 = last_control.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let c2 = (ox + n[0], oy + n[1]);
                (x, y) = (ox + n[2], oy + n[3]);
                path.cubic_to(c1, c2, (x, y));
                control = Some(c2);
            }
            _ => {
                if n[2] != 0.0 {
                    return Err("rotated arcs are not supported".to_string());
                }
                let end = (ox + n[5], oy + n[6]);
                arc_to(
                    &mut path,
                    (x, y),
                    (n[0], n[1]),
                    n[3] != 0.0,
                    n[4] != 0.0,
                    end,
                );
                (x, y) = end;
            }
        }
        last_control = control;
    }
    Ok(path)
}

/// An SVG arc from `from` to `to` with radii `radii`, as its centre form
/// (SVG F.6.5 and F.6.6), continuing the current subpath.
fn arc_to(
    path: &mut Path,
    from: (f32, f32),
    radii: (f32, f32),
    large: bool,
    sweep: bool,
    to: (f32, f32),
) {
    let (mut rx, mut ry) = (radii.0.abs(), radii.1.abs());
    if rx == 0.0 || ry == 0.0 || from == to {
        path.line_to(to.0, to.1);
        return;
    }
    let (x1, y1) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let scale = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if scale > 1.0 {
        rx *= scale.sqrt();
        ry *= scale.sqrt();
    }
    let numerator = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut factor = (numerator / denominator).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let (cx1, cy1) = (factor * rx * y1 / ry, -factor * ry * x1 / rx);
    let centre = (cx1 + (from.0 + to.0) / 2.0, cy1 + (from.1 + to.1) / 2.0);
    let angle = |ux: f32, uy: f32| uy.atan2(ux);
    let start = angle((x1 - cx1) / rx, (y1 - cy1) / ry);
    let end = angle((-x1 - cx1) / rx, (-y1 - cy1) / ry);
    let mut delta = end - start;
    if sweep && delta < 0.0 {
        delta += 2.0 * PI;
    } else if !sweep && delta > 0.0 {
        delta -= 2.0 * PI;
    }
    // addCentredArc measures clockwise from twelve o'clock.
    let from_angle = start + PI / 2.0;
    path.add_centred_arc(centre, (rx, ry), from_angle, from_angle + delta);
    path.line_to(to.0, to.1);
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

    fn line(to: (f32, f32)) -> Path {
        let mut line = Path::new();
        line.move_to(0.0, 0.0);
        line.line_to(to.0, to.1);
        line
    }

    fn close_to(actual: (f32, f32, f32, f32), expected: (f32, f32, f32, f32)) {
        let pairs = [
            (actual.0, expected.0),
            (actual.1, expected.1),
            (actual.2, expected.2),
            (actual.3, expected.3),
        ];
        assert!(
            pairs.iter().all(|(a, e)| (a - e).abs() < 1e-3),
            "{actual:?} is not {expected:?}"
        );
    }

    /// Rounded and square ends reach half the thickness past the line's
    /// ends; butt ends stop at them.
    #[test]
    fn end_caps_extend_by_half_the_thickness() {
        let line = line((10.0, 0.0));
        let rounded = line.stroked(2.0, JointStyle::Curved, EndCapStyle::Rounded);
        close_to(rounded.bounds().unwrap(), (-1.0, -1.0, 11.0, 1.0));
        let square = line.stroked(2.0, JointStyle::Mitered, EndCapStyle::Square);
        close_to(square.bounds().unwrap(), (-1.0, -1.0, 11.0, 1.0));
        let butt = line.stroked(2.0, JointStyle::Mitered, EndCapStyle::Butt);
        close_to(butt.bounds().unwrap(), (0.0, -1.0, 10.0, 1.0));
    }

    /// An end arrowhead 4 wide and 3 long: the stroke stops 3 short of the
    /// end, where a triangle 4 wide points to the end.
    #[test]
    fn arrowheads_cap_the_shortened_end() {
        let arrow = line((10.0, 0.0)).stroked_with_arrowheads(
            1.0,
            JointStyle::Curved,
            EndCapStyle::Butt,
            (0.0, 0.0),
            (4.0, 3.0),
        );
        close_to(arrow.bounds().unwrap(), (0.0, -2.0, 10.0, 2.0));
        // The triangle is the last piece: base at x 7, tip at the end.
        let triangle = arrow.subpaths().last().unwrap();
        assert!(triangle.points.contains(&(10.0, 0.0)));
        assert!(triangle.points.iter().all(|p| p.0 >= 7.0 - 1e-4));
    }

    /// Curved joints round the outside of a turn; mitred ones square it.
    #[test]
    fn curved_joints_round_the_corner() {
        let mut corner = Path::new();
        corner.move_to(0.0, 0.0);
        corner.line_to(10.0, 0.0);
        corner.line_to(10.0, 10.0);
        let curved = corner.stroked(2.0, JointStyle::Curved, EndCapStyle::Butt);
        let mitred = corner.stroked(2.0, JointStyle::Mitered, EndCapStyle::Butt);
        close_to(curved.bounds().unwrap(), (0.0, -1.0, 11.0, 10.0));
        close_to(mitred.bounds().unwrap(), (0.0, -1.0, 11.0, 10.0));
        // The corner's outer point (11, -1) is in the mitre only.
        let reaches = |path: &Path| {
            path.subpaths()
                .iter()
                .flat_map(|s| s.points.iter())
                .any(|p| (p.0 - 11.0).abs() < 1e-3 && (p.1 + 1.0).abs() < 1e-3)
        };
        assert!(reaches(&mitred));
        assert!(!reaches(&curved));
    }

    /// A full centred arc is a circle starting at twelve o'clock.
    #[test]
    fn centred_arcs_start_at_twelve() {
        let mut arc = Path::new();
        arc.centred_arc(5.0, 5.0, 2.0, 2.0, 0.0, 2.0 * PI);
        close_to(arc.bounds().unwrap(), (3.0, 3.0, 7.0, 7.0));
        assert_eq!(arc.subpaths()[0].points[0], (5.0, 3.0));
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

    /// `createDashedStroke`'s cuts: on 4, off 2 along a line; round a
    /// closed square, its closing side included; and on from one open
    /// subpath into the next.
    #[test]
    fn dashes_follow_juces_walk() {
        let points = |path: &Path| -> Vec<Vec<(f32, f32)>> {
            path.subpaths()
                .iter()
                .map(|subpath| subpath.points.clone())
                .collect()
        };
        let mut line = Path::new();
        line.move_to(0.0, 0.0);
        line.line_to(10.0, 0.0);
        assert_eq!(
            points(&line.dashed(&[4.0, 2.0])),
            [vec![(0.0, 0.0), (4.0, 0.0)], vec![(6.0, 0.0), (10.0, 0.0)]]
        );

        let mut square = Path::new();
        square.add_rectangle(0.0, 0.0, 2.0, 2.0);
        // The last gap ends in a lone start point, as in JUCE, which strokes
        // to nothing.
        let dashes = square.dashed(&[3.0, 1.0]);
        let drawn: Vec<usize> = dashes
            .subpaths()
            .iter()
            .map(|subpath| subpath.points.len())
            .filter(|&points| points > 1)
            .collect();
        // The second dash starts on a corner, which JUCE's walk adds again
        // as the next side's start.
        assert_eq!(drawn, [3, 4]);

        let mut two = Path::new();
        two.move_to(0.0, 0.0);
        two.line_to(1.0, 0.0);
        two.move_to(0.0, 5.0);
        two.line_to(1.0, 5.0);
        assert_eq!(
            points(&two.dashed(&[1.5, 10.0])),
            // JUCE adds a segment's start, not its end, so the first
            // subpath's end is skipped on the way into the second.
            [vec![(0.0, 0.0), (0.0, 5.0), (0.5, 5.0)]]
        );
        assert!(line.dashed(&[0.0, 0.0]).subpaths().is_empty());
    }
}
