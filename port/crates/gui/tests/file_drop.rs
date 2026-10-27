//! The vendored egui-baseview's file-drop patch (`file_drop`): synthetic
//! baseview drag events reach egui as hovered and dropped files, and
//! baseview is told to accept a drag only over a drop target that takes it,
//! as JUCE's `FileDragAndDropTarget`s decide.

use std::cell::Cell;
use std::path::PathBuf;

use keyboard_types::Modifiers;
use nice_plug_egui::baseview::dpi::PhysicalPosition;
use nice_plug_egui::baseview::{DropData, DropEffect, EventStatus, MouseEvent};
use nice_plug_egui::file_drop::{
    add_drop_target, handle_drag_event, take_drop_targets, DropTarget,
};

const ACCEPT: Option<EventStatus> = Some(EventStatus::AcceptDrop(DropEffect::Copy));
const REFUSE: Option<EventStatus> = Some(EventStatus::Ignored);

/// A target over points (50, 25) to (150, 75), taking WAV files.
fn wav_target() -> DropTarget {
    DropTarget {
        rect: egui::Rect::from_min_max(egui::pos2(50.0, 25.0), egui::pos2(150.0, 75.0)),
        extensions: &["wav"],
    }
}

fn files(names: &[&str]) -> DropData {
    DropData::Files(names.iter().map(PathBuf::from).collect())
}

fn at(x: f64, y: f64) -> PhysicalPosition<f64> {
    PhysicalPosition::new(x, y)
}

fn entered(x: f64, y: f64, data: DropData) -> MouseEvent {
    MouseEvent::DragEntered {
        position: at(x, y),
        modifiers: Modifiers::empty(),
        data,
    }
}

fn dropped(x: f64, y: f64, data: DropData) -> MouseEvent {
    MouseEvent::DragDropped {
        position: at(x, y),
        modifiers: Modifiers::empty(),
        data,
    }
}

#[test]
fn one_file_over_a_target_that_takes_it_hovers_then_drops() {
    let mut input = egui::RawInput::default();
    let pointer = Cell::new(None);
    let targets = [wav_target()];

    // (200, 100) physical at scale 2 is (100, 50) points: inside.
    let status = handle_drag_event(
        &entered(200.0, 100.0, files(&["/tmp/a.wav"])),
        2.0,
        &mut input,
        &pointer,
        &targets,
    );
    assert_eq!(status, ACCEPT);
    let hovered: Vec<_> = input
        .hovered_files
        .iter()
        .map(|file| file.path.clone().unwrap())
        .collect();
    assert_eq!(hovered, [PathBuf::from("/tmp/a.wav")]);
    assert_eq!(pointer.get(), Some(egui::pos2(100.0, 50.0)));
    assert_eq!(
        input.events,
        [egui::Event::PointerMoved(egui::pos2(100.0, 50.0))]
    );

    let moved = MouseEvent::DragMoved {
        position: at(220.0, 140.0),
        modifiers: Modifiers::empty(),
        data: files(&["/tmp/a.wav"]),
    };
    assert_eq!(
        handle_drag_event(&moved, 2.0, &mut input, &pointer, &targets),
        ACCEPT
    );
    assert_eq!(pointer.get(), Some(egui::pos2(110.0, 70.0)));

    // A frame takes the input: hovered files stay, events go.
    let frame = input.take();
    assert_eq!(frame.hovered_files.len(), 1);
    assert!(input.events.is_empty());
    assert_eq!(input.hovered_files.len(), 1);

    let status = handle_drag_event(
        &dropped(220.0, 140.0, files(&["/tmp/a.wav"])),
        2.0,
        &mut input,
        &pointer,
        &targets,
    );
    assert_eq!(status, ACCEPT);
    assert!(input.hovered_files.is_empty());
    let paths: Vec<_> = input
        .dropped_files
        .iter()
        .map(|file| file.path().to_owned())
        .collect();
    assert_eq!(paths, [PathBuf::from("/tmp/a.wav")]);

    // The drop reaches one frame only.
    assert_eq!(input.take().dropped_files.len(), 1);
    assert!(input.take().dropped_files.is_empty());
}

#[test]
fn drags_a_target_doesnt_take_are_refused_and_drop_nothing() {
    let targets = [wav_target()];
    let refused = [
        // Outside the target.
        (20.0, 20.0, files(&["/tmp/a.wav"])),
        // Two files (`isInterestedInFileDrag` wants exactly one).
        (100.0, 50.0, files(&["/tmp/a.wav", "/tmp/b.wav"])),
        // An extension the target doesn't take.
        (100.0, 50.0, files(&["/tmp/a.flac"])),
        (100.0, 50.0, files(&["/tmp/wav"])),
        // No files at all.
        (100.0, 50.0, DropData::None),
    ];
    for (x, y, data) in refused {
        let mut input = egui::RawInput::default();
        let pointer = Cell::new(None);
        assert_eq!(
            handle_drag_event(
                &entered(x, y, data.clone()),
                1.0,
                &mut input,
                &pointer,
                &targets
            ),
            REFUSE
        );
        assert_eq!(
            handle_drag_event(&dropped(x, y, data), 1.0, &mut input, &pointer, &targets),
            REFUSE
        );
        assert!(input.dropped_files.is_empty());
    }
    // With no targets at all, even a WAV over the middle is refused.
    let mut input = egui::RawInput::default();
    let status = handle_drag_event(
        &entered(100.0, 50.0, files(&["/tmp/a.wav"])),
        1.0,
        &mut input,
        &Cell::new(None),
        &[],
    );
    assert_eq!(status, REFUSE);
}

#[test]
fn extensions_match_without_case() {
    let mut input = egui::RawInput::default();
    let status = handle_drag_event(
        &entered(100.0, 50.0, files(&["/tmp/LOUD.WAV"])),
        1.0,
        &mut input,
        &Cell::new(None),
        &[wav_target()],
    );
    assert_eq!(status, ACCEPT);
}

#[test]
fn leaving_clears_the_hovered_files() {
    let mut input = egui::RawInput::default();
    let pointer = Cell::new(None);
    let targets = [wav_target()];
    handle_drag_event(
        &entered(100.0, 50.0, files(&["/tmp/a.wav"])),
        1.0,
        &mut input,
        &pointer,
        &targets,
    );
    let status = handle_drag_event(&MouseEvent::DragLeft, 1.0, &mut input, &pointer, &targets);
    assert_eq!(status, REFUSE);
    assert!(input.hovered_files.is_empty());
    assert!(input.dropped_files.is_empty());
    assert_eq!(pointer.get(), None);
    assert_eq!(input.events.last(), Some(&egui::Event::PointerGone));
    assert_eq!(
        handle_drag_event(&MouseEvent::CursorLeft, 1.0, &mut input, &pointer, &targets),
        None
    );
}

#[test]
fn a_frame_registers_targets_the_window_takes() {
    let ctx = egui::Context::default();
    add_drop_target(&ctx, wav_target());
    let other = DropTarget {
        rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0)),
        extensions: &["vitaltable"],
    };
    add_drop_target(&ctx, other.clone());
    assert_eq!(take_drop_targets(&ctx), [wav_target(), other]);
    // Taken: the next frame starts empty.
    assert!(take_drop_targets(&ctx).is_empty());
}

#[test]
fn a_dropped_file_reads_its_bytes() {
    let path = std::env::temp_dir().join(format!("sas-file-drop-{}.wav", std::process::id()));
    std::fs::write(&path, [1u8, 2, 3]).unwrap();
    let mut input = egui::RawInput::default();
    let data = DropData::Files(vec![path.clone()]);
    handle_drag_event(
        &dropped(100.0, 50.0, data),
        1.0,
        &mut input,
        &Cell::new(None),
        &[wav_target()],
    );
    assert_eq!(input.dropped_files[0].bytes().unwrap(), [1, 2, 3]);
    std::fs::remove_file(path).unwrap();
}
