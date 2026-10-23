//! Input on the editor shell, driven through egui_kittest (no GPU needed).

use std::cell::RefCell;

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use sas_gui::layout::FullLayout;
use sas_gui::{EditorModel, Interface, Tab};

struct InitPreset;

impl EditorModel for InitPreset {
    fn preset_name(&self) -> String {
        String::new()
    }
}

fn harness<'a>(interface: &'a RefCell<Interface>, pixels_per_point: f32) -> Harness<'a> {
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_pixels_per_point(pixels_per_point)
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            let screen = ui.ctx().content_rect();
            let host = sas_gui::Host::new(&InitPreset, None);
            interface.borrow_mut().show_in(ui, screen, &host)
        });
    harness.step();
    harness
}

fn button(harness: &mut Harness, pos: Pos2, pressed: bool) {
    harness.event(Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    });
    harness.step();
}

/// The centre of tab `index` in points, for a window laid out in pixels at
/// `pixels_per_point`.
fn tab_centre(interface: &RefCell<Interface>, index: usize, pixels_per_point: f32) -> Pos2 {
    let width = (1400.0 * pixels_per_point) as i32;
    let height = (820.0 * pixels_per_point) as i32;
    let tabs = FullLayout::new(interface.borrow().skin(), width, height)
        .header
        .tabs;
    let cell = tabs.width as f32 / 4.0;
    let x = tabs.x as f32 + cell * (index as f32 + 0.5);
    let y = tabs.y as f32 + tabs.height as f32 / 2.0;
    Pos2::new(x / pixels_per_point, y / pixels_per_point)
}

#[test]
fn clicking_a_tab_selects_it() {
    for pixels_per_point in [1.0, 2.0] {
        let interface = RefCell::new(Interface::default());
        let mut harness = harness(&interface, pixels_per_point);
        for (index, tab) in Tab::ALL.iter().enumerate().rev() {
            let pos = tab_centre(&interface, index, pixels_per_point);
            harness.event(Event::PointerMoved(pos));
            button(&mut harness, pos, true);
            button(&mut harness, pos, false);
            assert_eq!(interface.borrow().tab(), *tab, "at {pixels_per_point}x");
        }
    }
}

/// `TabSelector::mouseDown` selects before the button is released.
#[test]
fn pressing_a_tab_selects_it_before_release() {
    let interface = RefCell::new(Interface::default());
    let mut harness = harness(&interface, 1.0);
    let pos = tab_centre(&interface, 3, 1.0);
    harness.event(Event::PointerMoved(pos));
    button(&mut harness, pos, true);
    assert_eq!(interface.borrow().tab(), Tab::Advanced);
}

/// `TabSelector::mouseDrag`: dragging across the tabs follows the pointer.
#[test]
fn dragging_across_tabs_follows_the_pointer() {
    let interface = RefCell::new(Interface::default());
    let mut harness = harness(&interface, 1.0);
    let start = tab_centre(&interface, 0, 1.0);
    harness.event(Event::PointerMoved(start));
    button(&mut harness, start, true);
    for index in 1..4 {
        harness.event(Event::PointerMoved(tab_centre(&interface, index, 1.0)));
        harness.step();
        assert_eq!(interface.borrow().tab(), Tab::ALL[index]);
    }
}
