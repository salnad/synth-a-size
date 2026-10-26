//! synth-a-size patch: files dragged onto the window reach egui.
//!
//! baseview reports file drags (`MouseEvent::DragEntered`, `DragMoved`,
//! `DragLeft`, `DragDropped`) and accepts one only when the handler returns
//! `EventStatus::AcceptDrop`. This turns them into egui's own file input:
//! `RawInput::hovered_files` while files are over the window and
//! `RawInput::dropped_files` on the drop, with the pointer following the
//! drag.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use baseview::dpi::LogicalPosition;
use baseview::{DropData, DropEffect, EventStatus, MouseEvent};
use egui::{DroppedFile, HoveredFile, Pos2, RawInput, pos2};

/// Applies a drag event to `input`, with `pointer` the window's last pointer
/// position (logical points) and `scale` the physical pixels per point.
/// Returns the status to give baseview, or `None` for any other event.
pub fn handle_drag_event(
    event: &MouseEvent,
    scale: f64,
    input: &mut RawInput,
    pointer: &Cell<Option<Pos2>>,
) -> Option<EventStatus> {
    let mut move_pointer = |position: &baseview::dpi::PhysicalPosition<f64>| {
        let logical: LogicalPosition<f32> = position.to_logical(scale);
        let pos = pos2(logical.x, logical.y);
        pointer.set(Some(pos));
        input.events.push(egui::Event::PointerMoved(pos));
    };
    match event {
        MouseEvent::DragEntered { position, data, .. }
        | MouseEvent::DragMoved { position, data, .. } => {
            move_pointer(position);
            input.hovered_files = files(data)
                .map(|path| HoveredFile {
                    path: Some(path.clone()),
                    mime: String::new(),
                })
                .collect();
            Some(accept(data))
        }
        MouseEvent::DragLeft => {
            input.hovered_files.clear();
            pointer.set(None);
            input.events.push(egui::Event::PointerGone);
            Some(EventStatus::Ignored)
        }
        MouseEvent::DragDropped { position, data, .. } => {
            move_pointer(position);
            input.hovered_files.clear();
            input.dropped_files.extend(
                files(data)
                    .map(|path| Arc::new(DroppedPath(path.clone())) as egui::DroppedFileHandle),
            );
            Some(accept(data))
        }
        _ => None,
    }
}

/// A file dropped from the desktop: its path, read on demand.
#[derive(Debug)]
pub struct DroppedPath(pub PathBuf);

impl DroppedFile for DroppedPath {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|error| error.to_string())
    }
}

fn files(data: &DropData) -> impl Iterator<Item = &std::path::PathBuf> {
    match data {
        DropData::Files(paths) => paths.iter(),
        _ => [].iter(),
    }
}

/// Files are copied in (the app reads them); anything else is refused.
fn accept(data: &DropData) -> EventStatus {
    match data {
        DropData::Files(paths) if !paths.is_empty() => EventStatus::AcceptDrop(DropEffect::Copy),
        _ => EventStatus::Ignored,
    }
}
