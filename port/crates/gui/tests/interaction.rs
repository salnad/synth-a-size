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

fn click(harness: &mut Harness, pos: Pos2) {
    harness.event(Event::PointerMoved(pos));
    for pressed in [true, false] {
        harness.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
        harness.step();
    }
}

#[test]
fn clicking_a_tab_selects_it() {
    let interface = RefCell::new(Interface::default());
    let layout = FullLayout::new(interface.borrow().skin(), 1400, 820);
    let mut harness = Harness::builder()
        .with_size((1400.0, 820.0))
        .with_context_setup(|ctx| ctx.set_fonts(sas_gui::fonts::definitions()))
        .build_ui(|ui| {
            let screen = ui.ctx().content_rect();
            interface.borrow_mut().show_in(ui, screen, &InitPreset)
        });
    harness.step();

    let tabs = layout.header.tabs;
    let cell = tabs.width as f32 / 4.0;
    for (index, tab) in Tab::ALL.iter().enumerate().rev() {
        let x = tabs.x as f32 + cell * (index as f32 + 0.5);
        click(
            &mut harness,
            Pos2::new(x, tabs.y as f32 + tabs.height as f32 / 2.0),
        );
        assert_eq!(interface.borrow().tab(), *tab);
    }
}
