// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset browser, shown in place of the main sections: the preset's
//! name, author and comments, the search box, the style filters, the folder
//! list and the preset list. Ports `PresetBrowser` and `PresetList`
//! (`vital/src/interface/editor_sections/preset_browser.cpp`).

use egui::Key;

use super::Panel;
use crate::controls::button::SynthButton;
use crate::controls::icon;
use crate::controls::paths;
use crate::fonts::Face;
use crate::layout::Rect;
use crate::local::{PRESET_STYLE_FILTERS, PRESET_STYLE_NAMES};
use crate::paint::{PixelPainter, RectF};
use crate::scope::Scope;
use crate::section::{paint_body, paint_border, paint_tab_shadow};
use crate::skin::{ColorId, Section, ValueId};
use crate::text::{Justify, Overflow};

/// The browser's skin override (`setSkinOverride(Skin::kPresetBrowser)`).
pub const SKIN: &[Section] = &[Section::PresetBrowser];

/// `PresetBrowser`'s padding constants, before the size ratio.
const LEFT_PADDING: f32 = 24.0;
const TOP_PADDING: f32 = 24.0;
const MIDDLE_PADDING: f32 = 15.0;
const NAME_FONT_HEIGHT: f32 = 26.0;
const AUTHOR_FONT_HEIGHT: f32 = 19.0;

/// `PresetList`'s column widths, as fractions of the list's width.
const STAR_WIDTH_PERCENT: f32 = 0.04;
const NAME_WIDTH_PERCENT: f32 = 0.35;
const STYLE_WIDTH_PERCENT: f32 = 0.18;
const AUTHOR_WIDTH_PERCENT: f32 = 0.25;
const DATE_WIDTH_PERCENT: f32 = 0.18;
/// `PresetList::kRowSizeHeightPercent`.
const ROW_SIZE_HEIGHT_PERCENT: f32 = 0.04;

/// `PresetBrowser::resized` and `PresetList`'s geometry, in interface
/// coordinates.
#[derive(Clone, Copy, Debug)]
pub struct PresetBrowserLayout {
    pub bounds: Rect,
    /// `getInfoRect`: the preset's name, author and comments.
    pub info: Rect,
    /// `getSearchRect`: the search box, style filters and folder list.
    pub search_area: Rect,
    /// `preset_text_` and `author_text_`, with their text sizes.
    pub preset_text: Rect,
    pub preset_text_size: f32,
    pub author_text: Rect,
    pub author_text_size: f32,
    /// The divider lines' left edge, width and rows.
    pub text_x: i32,
    pub text_width: i32,
    pub author_line_y: i32,
    pub comments_line_y: i32,
    pub search: Rect,
    pub styles: [Rect; 9],
    pub folders: Rect,
    pub comments: Rect,
    /// `preset_list_`, with its title strip's height and its row height.
    pub list: Rect,
    pub title_width: i32,
    pub row_height: i32,
}

impl PresetBrowserLayout {
    /// `PresetBrowser::resized` with the browser at `bounds`; `top` is the
    /// interface's scope.
    pub fn new(top: &Scope, bounds: Rect) -> PresetBrowserLayout {
        const BROWSE_WIDTH_RATIO: f32 = 0.68;
        const SEARCH_BOX_ROW_HEIGHT_RATIO: f32 = 1.3;
        const INFO_HEIGHT_RATIO: f32 = 0.43;
        let scope = top.within(SKIN);
        let ratio = scope.size_ratio();
        let (width, height) = (bounds.width, bounds.height);
        let padding = scope.value(ValueId::LargePadding) as i32;

        let list_width = (width as f32 * BROWSE_WIDTH_RATIO) as i32;
        let list = Rect::new(width - list_width - padding, 0, list_width, height);
        let row_height = (height as f32 * ROW_SIZE_HEIGHT_PERCENT) as i32;
        let title_width = scope.value(ValueId::TitleWidth) as i32;

        // getInfoRect subtracts the unrounded padding.
        let info = Rect::new(
            0,
            0,
            (list.x as f32 - scope.value(ValueId::LargePadding)) as i32,
            (height as f32 * INFO_HEIGHT_RATIO) as i32,
        );
        let search_y = info.bottom() + padding;
        let search_area = Rect::new(0, search_y, info.width, height - search_y);

        let top_padding = (TOP_PADDING * ratio) as i32;
        let left_padding = (LEFT_PADDING * ratio) as i32;
        let middle_padding = (MIDDLE_PADDING * ratio) as i32;
        let name_y = info.y + top_padding;
        let name_height = (NAME_FONT_HEIGHT * ratio) as i32;
        let author_y = name_y + name_height + middle_padding;
        let author_height = (AUTHOR_FONT_HEIGHT * ratio) as i32;
        let text_x = info.x + left_padding;
        let text_width = info.width - 2 * left_padding;
        let preset_text = Rect::new(
            text_x,
            name_y - middle_padding,
            text_width,
            name_height + 2 * middle_padding,
        );
        let author_text = Rect::new(
            text_x,
            author_y,
            text_width / 2,
            author_height + 2 * middle_padding,
        );

        let search_height = (SEARCH_BOX_ROW_HEIGHT_RATIO * row_height as f32) as i32;
        let search = Rect::new(
            search_area.x + left_padding,
            search_area.y + top_padding,
            text_width,
            search_height,
        );
        let style_y = search.bottom() + top_padding;

        let widget_margin = scope.widget_margin() as i32;
        let style_height = row_height;
        let style_x = search_area.x + left_padding;
        let style_width = search_area.width - 2 * left_padding + widget_margin;
        let per_row = (PRESET_STYLE_NAMES.len() / 3) as i32;
        let styles = std::array::from_fn(|i| {
            let i = i as i32;
            let column = i % per_row;
            let x = style_x + (style_width * column) / per_row;
            let next_x = style_x + (style_width * (column + 1)) / per_row;
            let y = style_y + (i / per_row) * (style_height + widget_margin);
            Rect::new(x, y, next_x - x - widget_margin, style_height)
        });

        let folder_y = style_y + 3 * style_height + 2 * widget_margin + top_padding + 1;
        let folders = Rect::new(
            style_x,
            folder_y,
            text_width,
            search_area.bottom() - top_padding - folder_y - 1,
        );

        // setCommentsBounds, with the store button hidden.
        let top_info_height =
            ((NAME_FONT_HEIGHT + AUTHOR_FONT_HEIGHT + MIDDLE_PADDING * 4.0) * ratio) as i32;
        let comments_y = info.y + top_info_height + top_padding;
        let comments = Rect::new(
            text_x,
            comments_y,
            text_width,
            info.bottom() - comments_y - top_padding,
        );

        let (dx, dy) = (bounds.x, bounds.y);
        PresetBrowserLayout {
            bounds,
            info: info.translated(dx, dy),
            search_area: search_area.translated(dx, dy),
            preset_text: preset_text.translated(dx, dy),
            preset_text_size: name_height as f32,
            author_text: author_text.translated(dx, dy),
            author_text_size: author_height as f32,
            text_x: text_x + dx,
            text_width,
            author_line_y: author_y + dy,
            comments_line_y: author_y + author_height + 2 * middle_padding + dy,
            search: search.translated(dx, dy),
            styles: styles.map(|style| style.translated(dx, dy)),
            folders: folders.translated(dx, dy),
            comments: comments.translated(dx, dy),
            list: list.translated(dx, dy),
            title_width,
            row_height,
        }
    }
}

/// `DefaultLookAndFeel::fillTextEditorBackground`: a text editor's rounded
/// background and its one-pixel border.
fn paint_text_editor_background(pixels: &PixelPainter, scope: &Scope, bounds: Rect) {
    let rounding = scope.value(ValueId::WidgetRoundedCorner);
    pixels.fill_rounded_rect(bounds, rounding, scope.color(ColorId::TextEditorBackground));
    pixels.draw_rounded_rect(
        RectF::from(bounds).reduced(0.5),
        rounding,
        1.0,
        scope.color(ColorId::TextEditorBorder),
    );
}

/// `PresetList::paintBackground`: the list's background, its title strip
/// with the column dividers, and the column titles.
fn paint_list_background(pixels: &PixelPainter, scope: &Scope, layout: &PresetBrowserLayout) {
    let list = layout.list;
    let title_width = layout.title_width;
    let rounding = scope.value(ValueId::BodyRounding);
    pixels.fill_rounded_rect(list, rounding, scope.color(ColorId::WidgetBackground));

    let width = list.width as f32;
    let star_width = (STAR_WIDTH_PERCENT * width) as i32;
    let name_width = (NAME_WIDTH_PERCENT * width) as i32;
    let style_width = (STYLE_WIDTH_PERCENT * width) as i32;
    let author_width = (AUTHOR_WIDTH_PERCENT * width) as i32;
    let date_width = (DATE_WIDTH_PERCENT * width) as i32;
    let text_padding = layout.row_height / 2;

    // The body colour's rounded top, clipped to the title strip.
    let strip = Rect::new(list.x, list.y, list.width, title_width);
    pixels.with_clip(RectF::from(strip), |pixels| {
        let top = Rect::new(list.x, list.y, list.width, title_width * 2);
        pixels.fill_rounded_rect(top, rounding, scope.color(ColorId::Body));
    });

    let lighten = scope.color(ColorId::LightenScreen);
    for x in [
        star_width,
        star_width + name_width,
        star_width + name_width + style_width,
        list.width - date_width,
    ] {
        pixels.fill_rect(Rect::new(list.x + x, list.y, 1, title_width), lighten);
    }

    let text = scope.color(ColorId::TextComponentText);
    let size = title_width as f32 * 0.5;
    let star_draw_width = title_width as f32 * 0.8;
    let star_y = (title_width as f32 - star_draw_width) / 2.0;
    let star = RectF::new(
        list.x as f32 + (star_width as f32 - star_draw_width) / 2.0,
        list.y as f32 + star_y,
        star_draw_width,
        star_draw_width,
    );
    icon::fill_path_to_fit(pixels, "preset list star", paths::star(), star, text);

    let style = (Face::Regular, size, text);
    let title = |x: i32, w: i32| Rect::new(list.x + x, list.y, w, title_width);
    let left = |label: &str, rect: Rect| {
        pixels.text_line(label, rect, style, Justify::Left, Overflow::Ellipsis);
    };
    left("Name", title(text_padding + star_width, name_width));
    let style_x = star_width + name_width + text_padding;
    left("Style", title(style_x, style_width));
    left("Author", title(style_x + style_width, author_width));
    pixels.text_line(
        "Date",
        title(list.width - date_width, date_width - text_padding),
        style,
        Justify::Right,
        Overflow::Ellipsis,
    );

    paint_border(pixels, scope, list);
}

/// `PresetBrowser::paintBackgroundShadow` and `paintBackground`, with its
/// children's backgrounds.
pub fn paint_background(pixels: &PixelPainter, top: &Scope, layout: &PresetBrowserLayout) {
    let scope = top.within(SKIN);
    paint_tab_shadow(pixels, &scope, layout.search_area);
    paint_tab_shadow(pixels, &scope, layout.info);
    // PresetList::paintBackgroundShadow.
    paint_tab_shadow(pixels, &scope, layout.list);
    for area in [layout.search_area, layout.info] {
        paint_body(pixels, &scope, area);
        paint_border(pixels, &scope, area);
    }

    let lighten = scope.color(ColorId::LightenScreen);
    for y in [layout.author_line_y, layout.comments_line_y] {
        pixels.fill_rect(Rect::new(layout.text_x, y, layout.text_width, 1), lighten);
    }

    pixels.fill_rounded_rect(
        RectF::from(layout.folders).reduced(-1.0),
        scope.value(ValueId::WidgetRoundedCorner),
        scope.color(ColorId::WidgetBackground),
    );

    paint_list_background(pixels, &scope, layout);
    paint_text_editor_background(pixels, &scope, layout.search);
    paint_text_editor_background(pixels, &scope, layout.comments);
}

/// `PresetBrowser`: what the browser shows and how it answers the mouse
/// and keys.
#[derive(Default)]
pub struct PresetBrowser;

impl PresetBrowser {
    /// Draws the browser's parts over its background. Returns true when
    /// Escape asks to hide it (`keyPressed`).
    pub fn show(&mut self, panel: &mut Panel, layout: &PresetBrowserLayout) -> bool {
        let scope = panel.scope;
        let model = panel.host.model();

        // loadPresetInfo: the synth's name and author, or their prompts in
        // the lightened body colour.
        let lighten = scope
            .color(ColorId::Body)
            .overlaid_with(scope.color(ColorId::LightenScreen));
        let regular = scope.color(ColorId::BodyText);
        let name = model.preset_name();
        let author = model.preset_author();
        for (text, prompt, rect, size) in [
            (
                &name,
                "Preset name",
                layout.preset_text,
                layout.preset_text_size,
            ),
            (
                &author,
                "Author",
                layout.author_text,
                layout.author_text_size,
            ),
        ] {
            let (text, colour) = if text.is_empty() {
                (prompt, lighten)
            } else {
                (text.as_str(), regular)
            };
            panel.pixels.text_line(
                text,
                rect,
                (Face::Light, size, colour),
                Justify::Left,
                Overflow::Fit,
            );
        }

        // The search box's prompt (`setTextToShowWhenEmpty`), in its font
        // (half its height, `OpenGlTextEditor::applyFont`) inside its
        // indents.
        let search = layout.search;
        let body_text = scope.color(ColorId::BodyText);
        let left_indent = (search.height as f32 * 0.2) as i32;
        let top_indent = (search.height as f32 * 0.15) as i32;
        panel.pixels.text_line(
            "Search",
            Rect::new(
                search.x + left_indent,
                search.y + top_indent,
                search.width - left_indent,
                search.height - top_indent,
            ),
            (
                Face::Light,
                search.height as f32 / 2.0,
                body_text.with_multiplied_alpha(0.5),
            ),
            Justify::Left,
            Overflow::Curtail,
        );

        for ((name, label), bounds) in PRESET_STYLE_FILTERS
            .iter()
            .zip(PRESET_STYLE_NAMES)
            .zip(layout.styles)
        {
            panel.button(&SynthButton::text(name, label), bounds);
        }

        panel.ui.input(|input| input.key_pressed(Key::Escape))
    }
}
