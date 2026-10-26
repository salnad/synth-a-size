//! synth-a-size patch: files dragged onto the window reach egui.
//!
//! baseview reports file drags (`MouseEvent::DragEntered`, `DragMoved`,
//! `DragLeft`, `DragDropped`) and accepts one only when the handler returns
//! `EventStatus::AcceptDrop`. This turns them into egui's own file input:
//! `RawInput::hovered_files` while files are over the window and
//! `RawInput::dropped_files` on the drop, with the pointer following the
//! drag.
//!
//! Like JUCE's `FileDragAndDropTarget`s, only the places that want a file
//! take it. Each frame the app registers its drop targets
//! ([`add_drop_target`]); the window keeps the last frame's, and a drag is
//! accepted only over a target that takes it. baseview needs the answer
//! during the event, before another frame runs, so it comes from the
//! previous frame's targets.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use baseview::dpi::LogicalPosition;
use baseview::{DropData, DropEffect, EventStatus, MouseEvent};
use egui::{DroppedFile, HoveredFile, Pos2, RawInput, pos2};

/// A place in the window that takes dropped files: where it is (points) and
/// the file extensions it takes (lowercase, without the dot). Like JUCE's
/// `AudioFileDropSource::isInterestedInFileDrag`, it takes a drag of exactly
/// one file with one of those extensions.
#[derive(Clone, Debug, PartialEq)]
pub struct DropTarget {
    pub rect: egui::Rect,
    pub extensions: &'static [&'static str],
}

impl DropTarget {
    /// Whether this target takes `files` dragged to `pos`.
    pub fn takes(&self, pos: Pos2, files: &[PathBuf]) -> bool {
        self.rect.contains(pos) && matches!(files, [file] if has_extension(file, self.extensions))
    }
}

/// Whether `file`'s extension is one of `extensions` (case-insensitive).
pub fn has_extension(file: &Path, extensions: &[&str]) -> bool {
    file.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extensions
                .iter()
                .any(|wanted| extension.eq_ignore_ascii_case(wanted))
        })
}

#[derive(Clone, Debug, Default)]
struct DropTargets(Vec<DropTarget>);

fn targets_id() -> egui::Id {
    egui::Id::NULL.with("synth-a-size file drop targets")
}

/// Registers `target` for this frame.
pub fn add_drop_target(ctx: &egui::Context, target: DropTarget) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<DropTargets>(targets_id())
            .0
            .push(target)
    });
}

/// The targets registered since the last call. The window takes them after
/// each frame and answers drags with them until the next.
pub fn take_drop_targets(ctx: &egui::Context) -> Vec<DropTarget> {
    ctx.data_mut(|data| data.remove_temp::<DropTargets>(targets_id()))
        .map(|targets| targets.0)
        .unwrap_or_default()
}

/// Applies a drag event to `input`, with `pointer` the window's last pointer
/// position (logical points), `scale` the physical pixels per point and
/// `targets` the drop targets of the last frame. Returns the status to give
/// baseview, or `None` for any other event.
pub fn handle_drag_event(
    event: &MouseEvent,
    scale: f64,
    input: &mut RawInput,
    pointer: &Cell<Option<Pos2>>,
    targets: &[DropTarget],
) -> Option<EventStatus> {
    let mut move_pointer = |position: &baseview::dpi::PhysicalPosition<f64>| {
        let logical: LogicalPosition<f32> = position.to_logical(scale);
        let pos = pos2(logical.x, logical.y);
        pointer.set(Some(pos));
        input.events.push(egui::Event::PointerMoved(pos));
        pos
    };
    match event {
        MouseEvent::DragEntered { position, data, .. }
        | MouseEvent::DragMoved { position, data, .. } => {
            let pos = move_pointer(position);
            input.hovered_files = files(data)
                .iter()
                .map(|path| HoveredFile {
                    path: Some(path.clone()),
                    mime: String::new(),
                })
                .collect();
            Some(answer(targets, pos, data))
        }
        MouseEvent::DragLeft => {
            input.hovered_files.clear();
            pointer.set(None);
            input.events.push(egui::Event::PointerGone);
            Some(EventStatus::Ignored)
        }
        MouseEvent::DragDropped { position, data, .. } => {
            let pos = move_pointer(position);
            input.hovered_files.clear();
            let status = answer(targets, pos, data);
            if status != EventStatus::Ignored {
                input.dropped_files.extend(
                    files(data)
                        .iter()
                        .map(|path| Arc::new(DroppedPath(path.clone())) as egui::DroppedFileHandle),
                );
            }
            Some(status)
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

fn files(data: &DropData) -> &[PathBuf] {
    match data {
        DropData::Files(paths) => paths,
        _ => &[],
    }
}

/// Files are copied in (the app reads them) where a target takes them;
/// anything else is refused.
fn answer(targets: &[DropTarget], pos: Pos2, data: &DropData) -> EventStatus {
    let files = files(data);
    if targets.iter().any(|target| target.takes(pos, files)) {
        EventStatus::AcceptDrop(DropEffect::Copy)
    } else {
        EventStatus::Ignored
    }
}
