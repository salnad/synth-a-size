// Copyright synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6: juce_graphics, GPL-3.0-only).
// SPDX-License-Identifier: GPL-3.0-or-later AND GPL-3.0-only

//! How JUCE shortens a line of text too wide for its box
//! (`GlyphArrangement::addCurtailedLineOfText` and `addFittedText`), on the
//! glyphs' right edges in pixels from the line's start.

/// `Font::getDefaultMinimumHorizontalScaleFactor`, which
/// `drawFittedText`'s default minimum scale of 0 stands for.
pub const MINIMUM_HORIZONTAL_SCALE: f32 = 0.7;

/// What happens to a line too wide for its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overflow {
    /// `Graphics::drawText(..., false)`: glyphs from the first that passes
    /// the box by more than a pixel are dropped.
    Curtail,
    /// `Graphics::drawText` with ellipses (JUCE's default): curtailed, then
    /// ended with up to three full stops.
    Ellipsis,
    /// `Graphics::drawFittedText` on one line (`PlainTextComponent`): the
    /// trimmed line squeezed horizontally, to as little as
    /// [`MINIMUM_HORIZONTAL_SCALE`], then ended with full stops if that
    /// isn't enough.
    Fit,
}

/// Where a line sits across its box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Justify {
    /// `Justification::centredLeft`.
    Left,
    /// `Justification::centred`: whitespace is left out of the extent it
    /// centres.
    Centred,
}

/// A line as JUCE lays it out in a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineFit {
    /// The glyphs kept from the start of the line.
    pub kept: usize,
    /// Their horizontal scale.
    pub scale: f32,
    /// The full stops ending the line, each one full stop's advance wide.
    pub dots: usize,
    /// Where the first full stop starts.
    pub dots_x: f32,
}

impl LineFit {
    /// Fits a line whose glyphs end at `edges` (pixels from its start; one
    /// per glyph) into `width` pixels; `dot` is a full stop's advance.
    pub fn new(edges: &[f32], dot: f32, width: f32, overflow: Overflow) -> LineFit {
        let count = edges.len();
        let mut fit = LineFit {
            kept: count,
            scale: 1.0,
            dots: 0,
            dots_x: 0.0,
        };
        match overflow {
            Overflow::Curtail | Overflow::Ellipsis => {
                fit.kept = edges
                    .iter()
                    .take_while(|&&right| right <= width + 1.0)
                    .count();
                if fit.kept < count && overflow == Overflow::Ellipsis && count > 3 && fit.kept >= 3
                {
                    fit.insert_ellipsis(edges, dot, width);
                }
            }
            Overflow::Fit => {
                let line = edges.last().copied().unwrap_or(0.0);
                if line <= 0.0 {
                    return fit;
                }
                if line * MINIMUM_HORIZONTAL_SCALE < width {
                    if line > width {
                        fit.scale = width / line;
                    }
                } else if line > width {
                    // fitLineIntoSpace.
                    fit.scale = MINIMUM_HORIZONTAL_SCALE.max(width / line);
                    if line * fit.scale - 0.5 > width {
                        fit.insert_ellipsis(edges, dot, width);
                    }
                }
            }
        }
        fit
    }

    /// The left of kept glyph `index`.
    fn left(&self, edges: &[f32], index: usize) -> f32 {
        if index == 0 {
            0.0
        } else {
            edges[index - 1] * self.scale
        }
    }

    /// `GlyphArrangement::insertEllipsis`: drops glyphs from the end until
    /// three full stops fit before `max_x`, then adds full stops while they
    /// start before it.
    fn insert_ellipsis(&mut self, edges: &[f32], dot: f32, max_x: f32) {
        let mut x = 0.0;
        while self.kept > 0 {
            self.kept -= 1;
            x = self.left(edges, self.kept);
            if x + dot * 3.0 <= max_x {
                break;
            }
        }
        self.dots_x = x;
        self.dots = 0;
        for _ in 0..3 {
            self.dots += 1;
            x += dot;
            if x > max_x {
                break;
            }
        }
    }

    /// The left and right (pixels from the line's start) of what's drawn,
    /// as `justifyGlyphs` measures it: leaving out whitespace (`space(i)`
    /// for glyph `i`) when `justify` is [`Justify::Centred`].
    pub fn extent(
        &self,
        edges: &[f32],
        dot: f32,
        justify: Justify,
        space: impl Fn(usize) -> bool,
    ) -> Option<(f32, f32)> {
        let glyphs = (0..self.kept)
            .filter(|&i| justify == Justify::Left || !space(i))
            .map(|i| (self.left(edges, i), edges[i] * self.scale));
        let dots = (0..self.dots).map(|i| {
            let x = self.dots_x + i as f32 * dot;
            (x, x + dot)
        });
        glyphs
            .chain(dots)
            .reduce(|(left, right), (l, r)| (left.min(l), right.max(r)))
    }
}
