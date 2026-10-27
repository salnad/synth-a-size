// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The overlay asking before a preset is deleted. Ports `DeleteSection`
//! (`vital/src/interface/editor_sections/delete_section.cpp`).

use std::path::PathBuf;

use egui::{Sense, Ui};

use crate::controls::ui_button;
use crate::fonts::Face;
use crate::layout::Rect;
use crate::paint::PixelPainter;
use crate::scope::Scope;
use crate::skin::{ColorId, Section, ValueId};
use crate::text::{Justify, Overflow};

/// The overlay's skin override (`Overlay`: `Skin::kOverlay`).
pub const SKIN: &[Section] = &[Section::Overlay];

/// `DeleteSection`'s sizes, before the size ratio.
const DELETE_WIDTH: f32 = 340.0;
const DELETE_HEIGHT: f32 = 140.0;
const TEXT_HEIGHT: f32 = 15.0;
const PADDING_X: f32 = 25.0;
const PADDING_Y: f32 = 20.0;
const BUTTON_HEIGHT: f32 = 30.0;

/// `DeleteSection::resized`, over an interface at `bounds`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeleteLayout {
    /// `getDeleteRect`.
    pub body: Rect,
    pub cancel: Rect,
    pub delete: Rect,
    pub question: Rect,
    pub preset: Rect,
    pub text_size: f32,
}

impl DeleteLayout {
    pub fn new(size_ratio: f32, bounds: Rect) -> DeleteLayout {
        let width = (DELETE_WIDTH * size_ratio) as i32;
        let height = (DELETE_HEIGHT * size_ratio) as i32;
        let body = Rect::new(
            bounds.x + (bounds.width - width) / 2,
            bounds.y + (bounds.height - height) / 2,
            width,
            height,
        );
        let padding_x = (size_ratio * PADDING_X) as i32;
        let padding_y = (size_ratio * PADDING_Y) as i32;
        let button_height = (size_ratio * BUTTON_HEIGHT) as i32;
        let button_width = (body.width - 3 * padding_x) as f32 / 2.0;
        let button_y = body.bottom() - padding_y - button_height;
        let cancel = Rect::new(
            body.x + padding_x,
            button_y,
            button_width as i32,
            button_height,
        );
        let delete = Rect::new(
            (body.x as f32 + button_width) as i32 + 2 * padding_x,
            button_y,
            button_width as i32,
            button_height,
        );
        let text_height = (22.0 * size_ratio) as i32;
        let question = Rect::new(
            body.x + padding_x,
            body.y + padding_y,
            body.width - 2 * padding_x,
            text_height,
        );
        let preset = Rect::new(
            body.x + padding_x,
            body.y + padding_y + (30.0 * size_ratio) as i32,
            body.width - 2 * padding_x,
            text_height,
        );
        DeleteLayout {
            body,
            cancel,
            delete,
            question,
            preset,
            text_size: TEXT_HEIGHT * size_ratio,
        }
    }
}

/// What the overlay's buttons did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteOutcome {
    Open,
    /// Cancel, or a click outside the box (`mouseUp`).
    Cancelled,
    /// Delete: the file is gone (`buttonClicked`, `fileDeleted`).
    Deleted,
}

/// The overlay for `file`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteSection {
    pub file: PathBuf,
}

impl DeleteSection {
    /// Draws the overlay over the interface at `bounds` and handles its
    /// buttons; it takes every click while open.
    pub fn show(
        &self,
        ui: &mut Ui,
        pixels: &PixelPainter,
        top: &Scope,
        bounds: Rect,
    ) -> DeleteOutcome {
        let scope = top.within(SKIN);
        let layout = DeleteLayout::new(top.size_ratio(), bounds);
        let screen = ui.interact(
            pixels.rect(bounds.into()),
            ui.scope_id().with("delete overlay"),
            // Drags too, so none reaches a control beneath: egui gives a
            // press to the top widget that senses drags as well as to the
            // one that senses clicks.
            Sense::click_and_drag(),
        );

        pixels.fill_rect(bounds, scope.color(ColorId::OverlayScreen));
        pixels.fill_rounded_rect(
            layout.body,
            scope.value(ValueId::BodyRounding),
            scope.color(ColorId::Body),
        );
        let body_text = scope.color(ColorId::BodyText);
        let name = self
            .file
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (text, rect) in [
            (
                "Are you sure you want to delete this preset?",
                layout.question,
            ),
            (name.as_str(), layout.preset),
        ] {
            pixels.text_line(
                text,
                rect,
                (Face::Light, layout.text_size, body_text),
                Justify::Centred,
                Overflow::Fit,
            );
        }

        let id = ui.scope_id().with("delete buttons");
        if ui_button::show(
            ui,
            pixels,
            &scope,
            id.with("cancel"),
            "Cancel",
            false,
            layout.cancel,
        ) {
            return DeleteOutcome::Cancelled;
        }
        if ui_button::show(
            ui,
            pixels,
            &scope,
            id.with("delete"),
            "Delete",
            true,
            layout.delete,
        ) {
            // The list only offers files.
            let _ = std::fs::remove_file(&self.file);
            return DeleteOutcome::Deleted;
        }
        let outside = screen
            .interact_pointer_pos()
            .filter(|pos| !pixels.rect(layout.body.into()).contains(*pos));
        // mouseUp, after a click or a drag.
        if (screen.clicked() || screen.drag_stopped()) && outside.is_some() {
            return DeleteOutcome::Cancelled;
        }
        DeleteOutcome::Open
    }
}
