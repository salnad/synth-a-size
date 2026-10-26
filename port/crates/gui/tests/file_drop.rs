//! The vendored egui-baseview's file-drop patch (`file_drop`): synthetic
//! baseview drag events reach egui as hovered and dropped files, and
//! baseview is told to accept file drags.

use std::cell::Cell;
use std::path::PathBuf;

use keyboard_types::Modifiers;
use nice_plug_egui::baseview::dpi::PhysicalPosition;
use nice_plug_egui::baseview::{DropData, DropEffect, EventStatus, MouseEvent};
use nice_plug_egui::file_drop::handle_drag_event;

fn files() -> DropData {
    DropData::Files(vec![
        PathBuf::from("/tmp/a.wav"),
        PathBuf::from("/tmp/b.wav"),
    ])
}

fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
    PhysicalPosition::new(x, y)
}

#[test]
fn a_file_drag_hovers_then_drops() {
    let mut input = egui::RawInput::default();
    let pointer = Cell::new(None);
    let modifiers = Modifiers::empty();

    let entered = MouseEvent::DragEntered {
        position: at(200.0, 100.0),
        modifiers,
        data: files(),
    };
    let status = handle_drag_event(&entered, 2.0, &mut input, &pointer);
    assert_eq!(status, Some(EventStatus::AcceptDrop(DropEffect::Copy)));
    let hovered: Vec<_> = input
        .hovered_files
        .iter()
        .map(|file| file.path.clone().unwrap())
        .collect();
    assert_eq!(
        hovered,
        [PathBuf::from("/tmp/a.wav"), PathBuf::from("/tmp/b.wav")]
    );
    // Physical pixels to points, as the pointer.
    assert_eq!(pointer.get(), Some(egui::pos2(100.0, 50.0)));
    assert_eq!(
        input.events,
        [egui::Event::PointerMoved(egui::pos2(100.0, 50.0))]
    );

    let moved = MouseEvent::DragMoved {
        position: at(220.0, 140.0),
        modifiers,
        data: files(),
    };
    let status = handle_drag_event(&moved, 2.0, &mut input, &pointer);
    assert_eq!(status, Some(EventStatus::AcceptDrop(DropEffect::Copy)));
    assert_eq!(pointer.get(), Some(egui::pos2(110.0, 70.0)));
    assert_eq!(input.hovered_files.len(), 2);

    // A frame takes the input: hovered files stay, events go.
    let frame = input.take();
    assert_eq!(frame.hovered_files.len(), 2);
    assert!(input.events.is_empty());
    assert_eq!(input.hovered_files.len(), 2);

    let dropped = MouseEvent::DragDropped {
        position: at(220.0, 140.0),
        modifiers,
        data: files(),
    };
    let status = handle_drag_event(&dropped, 2.0, &mut input, &pointer);
    assert_eq!(status, Some(EventStatus::AcceptDrop(DropEffect::Copy)));
    assert!(input.hovered_files.is_empty());
    let paths: Vec<_> = input
        .dropped_files
        .iter()
        .map(|file| file.path().to_owned())
        .collect();
    assert_eq!(
        paths,
        [PathBuf::from("/tmp/a.wav"), PathBuf::from("/tmp/b.wav")]
    );

    // The drop reaches one frame only.
    assert_eq!(input.take().dropped_files.len(), 2);
    assert!(input.take().dropped_files.is_empty());
}

#[test]
fn leaving_clears_the_hovered_files() {
    let mut input = egui::RawInput::default();
    let pointer = Cell::new(None);
    let entered = MouseEvent::DragEntered {
        position: at(10.0, 10.0),
        modifiers: Modifiers::empty(),
        data: files(),
    };
    handle_drag_event(&entered, 1.0, &mut input, &pointer);
    let status = handle_drag_event(&MouseEvent::DragLeft, 1.0, &mut input, &pointer);
    assert_eq!(status, Some(EventStatus::Ignored));
    assert!(input.hovered_files.is_empty());
    assert!(input.dropped_files.is_empty());
    assert_eq!(pointer.get(), None);
    assert_eq!(input.events.last(), Some(&egui::Event::PointerGone));
}

#[test]
fn drags_without_files_are_refused_and_other_events_pass() {
    let mut input = egui::RawInput::default();
    let pointer = Cell::new(None);
    let entered = MouseEvent::DragEntered {
        position: at(10.0, 10.0),
        modifiers: Modifiers::empty(),
        data: DropData::None,
    };
    assert_eq!(
        handle_drag_event(&entered, 1.0, &mut input, &pointer),
        Some(EventStatus::Ignored)
    );
    assert!(input.hovered_files.is_empty());
    assert_eq!(
        handle_drag_event(&MouseEvent::CursorLeft, 1.0, &mut input, &pointer),
        None
    );
}

#[test]
fn a_dropped_file_reads_its_bytes() {
    let path = std::env::temp_dir().join(format!("sas-file-drop-{}.bin", std::process::id()));
    std::fs::write(&path, [1u8, 2, 3]).unwrap();
    let mut input = egui::RawInput::default();
    let dropped = MouseEvent::DragDropped {
        position: at(0.0, 0.0),
        modifiers: Modifiers::empty(),
        data: DropData::Files(vec![path.clone()]),
    };
    handle_drag_event(&dropped, 1.0, &mut input, &Cell::new(None));
    assert_eq!(input.dropped_files[0].bytes().unwrap(), [1, 2, 3]);
    std::fs::remove_file(path).unwrap();
}
