// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// Portions Copyright (c) 2020 Raw Material Software Limited (JUCE 6:
// juce_core, ISC).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The overlay that names a preset, or a wavetable, before saving it. Ports
//! `SaveSection` (`vital/src/interface/editor_sections/save_section.cpp`).

use std::path::{Path, PathBuf};

use egui::Sense;
use sas_preset::user_folders::{Kind, PRESET_EXTENSION, WAVETABLE_EXTENSION};

use super::Panel;
use crate::controls::button::SynthButton;
use crate::controls::text_editor::{self, TextStyle};
use crate::controls::ui_button;
use crate::fonts::Face;
use crate::interface::{user_folder, EditorModel};
use crate::layout::Rect;
use crate::local::{PRESET_STYLE_NAMES, SAVE_STYLE_BUTTONS};
use crate::skin::{ColorId, Section, ValueId};
use crate::text::{Justify, Overflow};

/// The overlay's skin override (`Overlay`: `Skin::kOverlay`).
pub const SKIN: &[Section] = &[Section::Overlay];

/// `SaveSection`'s sizes, before the size ratio.
const SAVE_WIDTH: f32 = 630.0;
const SAVE_PRESET_HEIGHT: f32 = 450.0;
const STYLE_PADDING_X: f32 = 4.0;
const STYLE_PADDING_Y: f32 = 4.0;
const STYLE_BUTTON_HEIGHT: f32 = 24.0;
const OVERWRITE_WIDTH: f32 = 340.0;
const OVERWRITE_HEIGHT: f32 = 160.0;
const TEXT_EDITOR_HEIGHT: f32 = 37.0;
const LABEL_HEIGHT: f32 = 15.0;
const BUTTON_HEIGHT: f32 = 40.0;
const DIVISION: f32 = 150.0;
const PADDING_X: f32 = 25.0;
const PADDING_Y: f32 = 20.0;
const EXTRA_TOP_PADDING: f32 = 10.0;

/// `LoadSave::kMaxCommentLength`.
const MAX_COMMENT_LENGTH: usize = 500;

/// `setSaveBounds`, over an interface at `bounds`. A file's box has no
/// style or comments (`saving_preset_` false).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SaveLayout {
    /// `getSaveRect`.
    pub body: Rect,
    pub name: Rect,
    pub author: Rect,
    pub styles: [Rect; 9],
    pub comments: Rect,
    pub save: Rect,
    pub cancel: Rect,
    pub name_label: Rect,
    pub author_label: Rect,
    pub style_label: Rect,
    pub comments_label: Rect,
    pub label_size: f32,
    /// `applyFontToAllText`: the light face at 0.6 of an editor's height.
    pub editor_font_size: f32,
}

impl SaveLayout {
    pub fn new(size_ratio: f32, bounds: Rect, preset: bool) -> SaveLayout {
        let body = save_rect(size_ratio, bounds, preset);
        // setSaveBounds pads vertically by kPaddingX too.
        let padding_x = (size_ratio * PADDING_X) as i32;
        let padding_y = (size_ratio * PADDING_X) as i32;
        let style_padding_x = (size_ratio * STYLE_PADDING_X) as i32;
        let style_padding_y = (size_ratio * STYLE_PADDING_Y) as i32;
        let division = (size_ratio * DIVISION) as i32;
        let extra_top_padding = (size_ratio * EXTRA_TOP_PADDING) as i32;
        let button_height = (size_ratio * BUTTON_HEIGHT) as i32;
        let style_button_height = (size_ratio * STYLE_BUTTON_HEIGHT) as i32;

        let button_width = (body.width - 3 * padding_x) as f32 / 2.0;
        let button_y = body.bottom() - padding_y - button_height;
        let save = Rect::new(
            (body.x as f32 + button_width + (2 * padding_x) as f32) as i32,
            button_y,
            button_width as i32,
            button_height,
        );
        let cancel = Rect::new(
            body.x + padding_x,
            button_y,
            button_width as i32,
            button_height,
        );

        let text_x = body.x + padding_x;
        let text_y = body.y + extra_top_padding;
        let editor_height = (TEXT_EDITOR_HEIGHT * size_ratio) as i32;
        let editor_x = body.x + padding_x + division;
        let editor_width = body.width - 2 * padding_x - division;
        let name = Rect::new(
            editor_x,
            body.y + padding_y + extra_top_padding,
            editor_width,
            editor_height,
        );
        let author = Rect::new(
            editor_x,
            body.y + 2 * padding_y + editor_height + extra_top_padding,
            editor_width,
            editor_height,
        );

        let style_width = editor_width + style_padding_x;
        let style_y = body.y + 3 * padding_y + 2 * editor_height + extra_top_padding;
        let num_in_row = 3;
        let styles = std::array::from_fn(|i| {
            let column = i as i32 % num_in_row;
            let x = editor_x + (style_width * column) / num_in_row;
            let next_x = editor_x + (style_width * (column + 1)) / num_in_row;
            let y = style_y + (i as i32 / num_in_row) * (style_button_height + style_padding_y);
            Rect::new(x, y, next_x - x - style_padding_x, style_button_height)
        });

        let comments_y = style_y + 3 * style_button_height + 2 * style_padding_y + padding_y;
        let comments = Rect::new(
            editor_x,
            comments_y,
            editor_width,
            save.y - comments_y - padding_y,
        );

        let text_width = (division as f32 - 10.0 * size_ratio) as i32;
        let name_label = Rect::new(text_x, text_y + padding_y, text_width, editor_height);
        let author_label = Rect::new(
            text_x,
            text_y + 2 * padding_y + editor_height,
            text_width,
            editor_height,
        );
        let style_height = 3 * style_button_height + 2 * style_padding_y;
        let style_label = Rect::new(
            text_x,
            text_y + 3 * padding_y + 2 * editor_height,
            text_width,
            style_height,
        );
        let comments_label = Rect::new(
            text_x,
            text_y + 4 * padding_y + style_height + 2 * editor_height,
            text_width,
            editor_height,
        );

        SaveLayout {
            body,
            name,
            author,
            styles,
            comments,
            save,
            cancel,
            name_label,
            author_label,
            style_label,
            comments_label,
            label_size: LABEL_HEIGHT * size_ratio,
            editor_font_size: editor_height as f32 * 0.6,
        }
    }
}

/// `getSaveRect`.
fn save_rect(size_ratio: f32, bounds: Rect, preset: bool) -> Rect {
    let width = (SAVE_WIDTH * size_ratio) as i32;
    let height = if preset {
        (SAVE_PRESET_HEIGHT * size_ratio) as i32
    } else {
        ((TEXT_EDITOR_HEIGHT * 2.0 + BUTTON_HEIGHT + PADDING_Y * 4.0 + EXTRA_TOP_PADDING)
            * size_ratio) as i32
    };
    Rect::new(
        bounds.x + (bounds.width - width) / 2,
        bounds.y + (bounds.height - height) / 2,
        width,
        height,
    )
}

/// `setOverwriteBounds`, over an interface at `bounds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverwriteLayout {
    /// `getOverwriteRect`.
    pub body: Rect,
    pub text: Rect,
    pub cancel: Rect,
    pub overwrite: Rect,
    pub label_size: f32,
}

impl OverwriteLayout {
    pub fn new(size_ratio: f32, bounds: Rect) -> OverwriteLayout {
        let width = (OVERWRITE_WIDTH * size_ratio) as i32;
        let height = (OVERWRITE_HEIGHT * size_ratio) as i32;
        let body = Rect::new(
            bounds.x + ((bounds.width as f32 - OVERWRITE_WIDTH * size_ratio) / 2.0) as i32,
            bounds.y + ((bounds.height as f32 - OVERWRITE_HEIGHT * size_ratio) / 2.0) as i32,
            width,
            height,
        );
        let padding_x = (PADDING_X * size_ratio) as i32;
        let padding_y = (PADDING_Y * size_ratio) as i32;
        let button_height = (BUTTON_HEIGHT * size_ratio) as i32;
        let text = Rect::new(
            body.x + padding_x,
            body.y + ((EXTRA_TOP_PADDING + 24.0) * size_ratio) as i32,
            body.width - 2 * padding_x,
            (24.0 * size_ratio) as i32,
        );
        let button_width = (body.width - 3 * padding_x) as f32 / 2.0;
        let button_y = body.bottom() - padding_y - button_height;
        let cancel = Rect::new(
            body.x + padding_x,
            button_y,
            button_width as i32,
            button_height,
        );
        let overwrite = Rect::new(
            (body.x as f32 + button_width + (2 * padding_x) as f32) as i32,
            button_y,
            button_width as i32,
            button_height,
        );
        OverwriteLayout {
            body,
            text,
            cancel,
            overwrite,
            label_size: LABEL_HEIGHT * size_ratio,
        }
    }
}

/// What is saved: the synth's preset, or a file written from JSON
/// (`setIsPreset(false)`, `setFileData`).
#[derive(Clone, Debug, PartialEq)]
pub enum SaveTarget {
    Preset,
    File {
        /// `setFileType`: "Wavetable".
        file_type: &'static str,
        extension: &'static str,
        folder: PathBuf,
        data: String,
    },
}

impl SaveTarget {
    /// `FullInterface::saveWavetable`: a `.vitaltable` in the user's
    /// Wavetables folder, made if missing, from the wavetable's JSON.
    pub fn wavetable(model: &dyn EditorModel, oscillator: usize) -> Option<SaveTarget> {
        let folder = user_folder(model, Kind::Wavetables)?;
        let _ = std::fs::create_dir_all(&folder);
        Some(SaveTarget::File {
            file_type: "Wavetable",
            extension: WAVETABLE_EXTENSION,
            folder,
            data: model.wavetable_json(oscillator)?,
        })
    }

    fn is_preset(&self) -> bool {
        matches!(self, SaveTarget::Preset)
    }

    fn file_type(&self) -> &'static str {
        match self {
            SaveTarget::Preset => "Preset",
            SaveTarget::File { file_type, .. } => file_type,
        }
    }
}

/// What the overlay did this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    Open,
    /// Cancel, or a click outside the box (`mouseUp`).
    Closed,
    /// The file was written (`Listener::save`).
    Saved(PathBuf),
}

/// `SaveSection`'s state while it shows.
#[derive(Clone, Debug, Default)]
pub struct SaveSection {
    target: Option<SaveTarget>,
    /// `overwrite_`: asking before replacing a file.
    overwrite: bool,
    name: String,
    /// The author field keeps its text between openings; it starts as
    /// `LoadSave::getAuthor`.
    author: Option<String>,
    comments: String,
    focus_name: bool,
}

impl SaveSection {
    /// `setVisible(true)`: the name, comments and style from the synth.
    pub fn open(&mut self, host: &crate::host::Host, target: SaveTarget) {
        let model = host.model();
        self.overwrite = false;
        self.name = model.preset_name();
        self.comments = model.preset_comments();
        if self.author.is_none() {
            self.author = Some(model.default_author());
        }
        let style = model.preset_style();
        for (name, label) in SAVE_STYLE_BUTTONS.iter().zip(PRESET_STYLE_NAMES) {
            host.set(name, if style == label { 1.0 } else { 0.0 });
        }
        self.target = Some(target);
        self.focus_name = true;
    }

    pub fn is_open(&self) -> bool {
        self.target.is_some()
    }

    /// Whether it's asking before replacing a file.
    pub fn is_overwriting(&self) -> bool {
        self.overwrite
    }

    pub fn close(&mut self) {
        self.target = None;
        self.overwrite = false;
    }

    /// Draws the overlay over the interface at `bounds` and handles its
    /// fields and buttons; it takes every click while open.
    pub fn show(&mut self, panel: &mut Panel, bounds: Rect) -> SaveOutcome {
        let Some(target) = self.target.clone() else {
            return SaveOutcome::Closed;
        };
        let size_ratio = panel.scope.size_ratio();
        let preset = target.is_preset();
        let layout = SaveLayout::new(size_ratio, bounds, preset);
        let screen = panel.ui.interact(
            panel.pixels.rect(bounds.into()),
            panel.ui.scope_id().with("save overlay"),
            Sense::click(),
        );
        let scope = panel.scope;
        panel
            .pixels
            .fill_rect(bounds, scope.color(ColorId::OverlayScreen));
        let rounding = scope.value(ValueId::BodyRounding);
        let body_text = scope.color(ColorId::BodyText);
        let id = panel.ui.scope_id().with("save buttons");

        let mut outcome = SaveOutcome::Open;
        let mut save = false;
        if self.overwrite {
            let overwrite = OverwriteLayout::new(size_ratio, bounds);
            panel
                .pixels
                .fill_rounded_rect(overwrite.body, rounding, scope.color(ColorId::Body));
            panel.pixels.text_line(
                "Overwrite existing file?",
                overwrite.text,
                (Face::Light, overwrite.label_size, body_text),
                Justify::Centred,
                Overflow::Fit,
            );
            let (ui, pixels) = (&mut *panel.ui, panel.pixels);
            if ui_button::show(
                ui,
                pixels,
                &scope,
                id.with("cancel"),
                "Cancel",
                false,
                overwrite.cancel,
            ) {
                outcome = SaveOutcome::Closed;
            }
            if ui_button::show(
                ui,
                pixels,
                &scope,
                id.with("overwrite"),
                "Overwrite",
                true,
                overwrite.overwrite,
            ) {
                save = true;
            }
        } else {
            panel
                .pixels
                .fill_rounded_rect(layout.body, rounding, scope.color(ColorId::Body));
            let mut labels = vec![("NAME", layout.name_label), ("AUTHOR", layout.author_label)];
            if preset {
                labels.push(("STYLE", layout.style_label));
                labels.push(("COMMENTS", layout.comments_label));
            }
            for (text, rect) in labels {
                panel.pixels.text_line(
                    text,
                    rect,
                    (Face::Light, layout.label_size, body_text),
                    Justify::Right,
                    Overflow::Fit,
                );
            }

            let style = TextStyle {
                font_size: layout.editor_font_size,
                colour: ColorId::PresetText,
            };
            // fillTextEditorBackground, under each field.
            text_editor::paint_background(panel.pixels, &scope, layout.name);
            text_editor::paint_background(panel.pixels, &scope, layout.author);
            if preset {
                text_editor::paint_background(panel.pixels, &scope, layout.comments);
            }
            let name_prompt = format!("{} Name", target.file_type());
            let (ui, pixels) = (&mut *panel.ui, panel.pixels);
            // A field drops focus on a click elsewhere, so the name takes the
            // keyboard once the click that opened the section is over.
            if self.focus_name && !ui.input(|input| input.pointer.any_released()) {
                self.focus_name = false;
                text_editor::focus_at_end(ui, "save name", &self.name);
            }
            let name = text_editor::single_line_styled(
                ui,
                pixels,
                &scope,
                "save name",
                layout.name,
                &mut self.name,
                &name_prompt,
                style,
            );
            let author = self.author.get_or_insert_with(String::new);
            let author = text_editor::single_line_styled(
                ui,
                pixels,
                &scope,
                "save author",
                layout.author,
                author,
                "Author",
                style,
            );
            save |= name.return_pressed || author.return_pressed;
            if preset {
                text_editor::multi_line(
                    ui,
                    pixels,
                    &scope,
                    "save comments",
                    layout.comments,
                    &mut self.comments,
                    "Comments",
                    style,
                    MAX_COMMENT_LENGTH,
                );
            }
            if ui_button::show(
                ui,
                pixels,
                &scope,
                id.with("cancel"),
                "Cancel",
                false,
                layout.cancel,
            ) {
                outcome = SaveOutcome::Closed;
            }
            if ui_button::show(
                ui,
                pixels,
                &scope,
                id.with("save"),
                "Save",
                true,
                layout.save,
            ) {
                save = true;
            }
            if preset {
                self.show_styles(panel, &layout);
            }
        }

        // mouseUp: a click outside the save box closes it, in either mode.
        let outside = screen
            .interact_pointer_pos()
            .filter(|pos| !panel.pixels.rect(layout.body.into()).contains(*pos));
        if screen.clicked() && outside.is_some() {
            outcome = SaveOutcome::Closed;
        }
        if save {
            if let Some(saved) = self.save(panel.host) {
                outcome = SaveOutcome::Saved(saved);
            }
        }
        if outcome != SaveOutcome::Open {
            self.close();
        }
        outcome
    }

    /// The style buttons, at most one on: turning one on turns the others
    /// off (`buttonClicked`).
    fn show_styles(&self, panel: &mut Panel, layout: &SaveLayout) {
        for ((name, label), bounds) in SAVE_STYLE_BUTTONS
            .iter()
            .zip(PRESET_STYLE_NAMES)
            .zip(layout.styles)
        {
            if panel.button(&SynthButton::text(name, label), bounds) {
                for other in SAVE_STYLE_BUTTONS.iter().filter(|other| *other != name) {
                    panel.host.set(other, 0.0);
                }
            }
        }
    }

    /// `save`: the file, once it's named and, if it exists, the overwrite
    /// is confirmed. Returns the file written, or `None` while it stays open.
    fn save(&mut self, host: &crate::host::Host) -> Option<PathBuf> {
        let target = self.target.clone()?;
        let model = host.model();
        let name = self.name.trim();
        if name.is_empty() {
            return None;
        }
        let (extension, folder) = match &target {
            SaveTarget::Preset => (PRESET_EXTENSION, user_folder(model, Kind::Presets)?),
            SaveTarget::File {
                extension, folder, ..
            } => (*extension, folder.clone()),
        };
        let file = folder.join(legal_file_name(&format!("{name}.{extension}")));
        if !self.overwrite && file.exists() {
            self.overwrite = true;
            return None;
        }

        let author = self.author.clone().unwrap_or_default();
        let author = author.trim();
        match &target {
            SaveTarget::Preset => {
                model.save_default_author(author);
                let style = SAVE_STYLE_BUTTONS
                    .iter()
                    .zip(PRESET_STYLE_NAMES)
                    .filter(|(button, _)| host.value(button) != 0.0)
                    .map(|(_, label)| label)
                    .next_back()
                    .unwrap_or("");
                model.set_preset_info(
                    &author.replace('"', ""),
                    &self.comments.trim().replace('"', ""),
                    &style.replace('"', ""),
                );
                let _ = model.save_preset_file(&file);
            }
            SaveTarget::File { data, .. } => {
                write_file_data(&file, data, name, author);
            }
        }
        Some(file)
    }
}

/// The non-preset branch of `save`: the JSON with its `"name"` and
/// `"author"` set, replacing the file.
fn write_file_data(file: &Path, data: &str, name: &str, author: &str) {
    let mut data = sas_common::json::parse(data)
        .ok()
        .filter(|data| data.is_object())
        .unwrap_or(sas_common::json::Value::Null);
    data.set("name", name.to_string());
    data.set("author", author.to_string());
    let _ = std::fs::write(file, data.dump());
}

/// `File::createLegalFileName` (juce_core): without the characters file
/// names can't have, and at most 128 characters, keeping a short
/// extension.
pub fn legal_file_name(original: &str) -> String {
    const MAX_LENGTH: usize = 128;
    let s: Vec<char> = original
        .chars()
        .filter(|c| !"\"#@,;:<>*^|?\\/".contains(*c))
        .collect();
    let len = s.len();
    if len <= MAX_LENGTH {
        return s.into_iter().collect();
    }
    let last_dot = s.iter().rposition(|c| *c == '.');
    match last_dot {
        Some(dot) if dot as i64 > (len as i64 - 12).max(0) => {
            let keep = MAX_LENGTH - (len - dot);
            s[..keep].iter().chain(&s[dot..]).collect()
        }
        _ => s[..MAX_LENGTH].iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `getSaveRect` and `setSaveBounds` at 1x over a 1400 x 820 interface,
    /// against upstream's (`save_section` in the layout dump).
    #[test]
    fn the_preset_box_matches_upstream() {
        let layout = SaveLayout::new(1.0, Rect::new(0, 0, 1400, 820), true);
        assert_eq!(layout.name, Rect::new(560, 220, 430, 37));
        assert_eq!(layout.author, Rect::new(560, 282, 430, 37));
        assert_eq!(layout.comments, Rect::new(560, 449, 430, 96));
        assert_eq!(layout.save, Rect::new(712, 570, 277, 40));
        assert_eq!(layout.cancel, Rect::new(410, 570, 277, 40));
        assert_eq!(layout.name_label, Rect::new(410, 220, 140, 37));
        assert_eq!(layout.style_label, Rect::new(410, 344, 140, 80));
        assert_eq!(layout.comments_label, Rect::new(410, 449, 140, 37));
        assert_eq!(layout.styles[0], Rect::new(560, 344, 140, 24));
        assert_eq!(layout.styles[1], Rect::new(704, 344, 141, 24));
        assert_eq!(layout.styles[5], Rect::new(849, 372, 141, 24));
    }

    #[test]
    fn legal_file_names_drop_what_files_cant_have() {
        assert_eq!(legal_file_name("a/b:c?.vital"), "abc.vital");
        assert_eq!(legal_file_name("Plain Name.vital"), "Plain Name.vital");
        let long = format!("{}.vital", "x".repeat(200));
        let legal = legal_file_name(&long);
        assert_eq!(legal.chars().count(), 128);
        assert!(legal.ends_with("x.vital"));
        let no_extension = "y".repeat(200);
        assert_eq!(legal_file_name(&no_extension), "y".repeat(128));
    }
}
