// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin editor window: [`Interface`] in a nice-plug egui editor drawn
//! with wgpu (Metal on macOS).
//!
//! Window behaviour follows upstream's plugin editor
//! (`vital/src/plugin/synth_editor.cpp`, `common/border_bounds_constrainer.cpp`):
//! it opens at the default 1400 × 820 and the host may resize it down to
//! 350 × 205, keeping the 1400:820 aspect ratio. Upstream also scales the
//! first size by a factor saved in its config file; the port does not yet.

use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::mem::ManuallyDrop;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::Arc;

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::dpi::NativeSize;
use nice_plug::editor::{
    HostMethods, Modifiers, ParentWindowHandle, ResizeHint, SpawnedEditor, VirtualKeyCode,
};
use nice_plug::prelude::TrackInfo;
use nice_plug_egui::baseview::dpi::LogicalSize;
use nice_plug_egui::{
    baseview, create_egui_editor, EguiEditor, EguiEditorHandle, EguiEditorState, EguiNiceSettings,
    Frame, NiceEguiApp,
};

use crate::chat::ChatPanel;
use crate::fonts;
use crate::host::{Host, OpenGestures};
use crate::interface::{EditorModel, Interface, PRODUCT_NAME};
use crate::layout::{DEFAULT_HEIGHT, DEFAULT_WIDTH};

/// `vital::kMinWindowWidth` / `kMinWindowHeight`.
pub const MIN_WIDTH: f32 = 350.0;
pub const MIN_HEIGHT: f32 = 205.0;

/// The editor's size state at the default size. Keep one per plugin
/// instance so the size survives closing and reopening the window.
pub fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(
        LogicalSize::new(DEFAULT_WIDTH as f64, DEFAULT_HEIGHT as f64),
        1.0,
    )
}

/// The editor for one plugin instance.
pub fn create(state: Arc<EguiEditorState>, model: Arc<dyn EditorModel>) -> Option<Editor> {
    let resize_hint = nice_plug::prelude::ResizeHint {
        preserve_aspect_ratio: true,
        ..nice_plug::prelude::ResizeHint::resizable()
            .with_min_logical_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
            .with_aspect_ratio(DEFAULT_WIDTH as u32, DEFAULT_HEIGHT as u32)
    };
    let settings = EguiNiceSettings::default()
        .with_tile(PRODUCT_NAME)
        .with_resize_hint(resize_hint);
    create_egui_editor(state, Default::default(), settings, App::new(model)).map(Editor)
}

/// The egui app behind [`Editor`].
pub struct App {
    model: Arc<dyn EditorModel>,
    interface: Interface,
    /// The host connection, from the window's `build` until it closes.
    gui_context: Option<GuiContext>,
    open_gestures: Arc<OpenGestures>,
    chat: ChatPanel,
    #[cfg(feature = "focus-spike")]
    focus_spike: crate::focus_spike::FocusSpike,
}

impl App {
    fn new(model: Arc<dyn EditorModel>) -> App {
        App {
            model,
            interface: Interface::default(),
            gui_context: None,
            open_gestures: Arc::default(),
            chat: ChatPanel::default(),
            #[cfg(feature = "focus-spike")]
            focus_spike: Default::default(),
        }
    }
}

impl NiceEguiApp for App {
    fn build(
        &mut self,
        egui_ctx: egui::Context,
        gui_context: GuiContext,
        frame: &mut Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        egui_ctx.set_fonts(fonts::definitions());
        // Upstream's browser reads the presets when the editor is built.
        self.interface.reload_presets();
        // Open and save dialogs attach to this window.
        crate::controls::file_dialog::set_parent(
            &egui_ctx,
            Some(frame.baseview_window().platform_handle()),
        );
        // A drag cut short by the window closing must not leave the host
        // waiting for the end of its gesture. nice-plug-egui keeps this app
        // across windows without telling it one closed; the patched
        // egui-baseview does.
        // Likewise a key the on-screen keyboard is still playing, and a
        // modulation a drag has connected but not yet written to the host;
        // all of them also when the window stops after a panic (it stays open,
        // inert, until the host closes it).
        let release = {
            let (model, open) = (self.model.clone(), self.open_gestures.clone());
            let context = gui_context.clone();
            let playing_note = self.interface.playing_note();
            let pending = self.interface.pending_connection();
            move || {
                let host = Host::tracking(model.as_ref(), Some(&context), &open);
                host.end_open_gestures();
                pending.commit(&host);
                playing_note.release(model.as_ref());
            }
        };
        // A window that stopped after a panic no longer forwards learned MIDI
        // moves, so it stops counting as open then rather than at close.
        let window = OpenWindow::new(self.model.clone());
        frame.on_failure({
            let (window, release) = (window.clone(), release.clone());
            move || {
                window.close();
                release();
            }
        });
        let dialogs = egui_ctx.clone();
        let closing = window.clone();
        frame.on_close(move || {
            // The window handle (on X11, with its connection) goes with the
            // window.
            crate::controls::file_dialog::set_parent(&dialogs, None);
            closing.close();
            release();
        });
        let repaint = egui_ctx.clone();
        self.model.attach(
            gui_context.clone(),
            Box::new(move || repaint.request_repaint()),
        );
        self.gui_context = Some(gui_context);
        window.open();
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut Frame) {
        let area = ui.max_rect();
        let host = Host::tracking(
            self.model.as_ref(),
            self.gui_context.as_ref(),
            &self.open_gestures,
        );
        // Parameters a learned MIDI controller moved reach the host as
        // gestures, before this frame draws them.
        host.forward_midi_moves();
        self.interface.show(ui, &host);
        if crate::controls::text_editor::take_window_focus_request(ui.ctx()) {
            // As the chat's fields: ask for the keyboard when the focus
            // setting does (its default without a chat).
            let grab = self
                .model
                .chat()
                .is_none_or(|chat| chat.settings().focus == crate::FocusMode::GrabOnClick);
            if grab {
                let _ = frame.baseview_window().focus();
            }
        }
        if let Some(chat) = self.model.chat() {
            let placement = self
                .interface
                .chat_placement(area, ui.ctx().pixels_per_point());
            self.chat.show(ui.ctx(), &placement, chat, &mut || {
                let _ = frame.baseview_window().focus();
            });
        }
        #[cfg(feature = "focus-spike")]
        self.focus_spike.show(ui, frame);
    }
}

/// One window's share of the model's open-editor count
/// ([`EditorModel::set_editor_open`]): taken by [`open`](Self::open) and
/// given back once, by the first [`close`](Self::close) after it.
#[derive(Clone)]
struct OpenWindow {
    model: Arc<dyn EditorModel>,
    counted: Rc<Cell<bool>>,
}

impl OpenWindow {
    fn new(model: Arc<dyn EditorModel>) -> OpenWindow {
        OpenWindow {
            model,
            counted: Rc::default(),
        }
    }

    fn open(&self) {
        if !self.counted.replace(true) {
            self.model.set_editor_open(true);
        }
    }

    fn close(&self) {
        if self.counted.replace(false) {
            self.model.set_editor_open(false);
        }
    }
}

/// The editor type `sas-plugin` hands to nice-plug: nice-plug-egui's editor
/// with every call the host makes into it under `catch_unwind`. nice-plug
/// calls these from the host's plugin API (VST3's `extern "system"`
/// methods), where a panic aborts the host. egui-baseview guards the
/// window's own callbacks; this covers nice-plug-egui's setup in `spawn`,
/// baseview's window creation and teardown, and the rest of the editor API.
pub struct Editor(EguiEditor<App>);

/// [`Editor`]'s handle, its calls guarded the same way.
pub struct EditorHandle(EguiEditorHandle);

/// The editor's window, dropped (closed) under `catch_unwind`: closing it
/// drops the window's app and renderer, and on X11 joins its thread,
/// re-raising a panic from it.
pub struct EditorWindow(ManuallyDrop<baseview::Window>);

impl Drop for EditorWindow {
    fn drop(&mut self) {
        // SAFETY: the window is not used again.
        let window = unsafe { ManuallyDrop::take(&mut self.0) };
        guarded("closing the window", || (), move || drop(window));
    }
}

/// An [`EditorHandle`] call that failed or panicked.
#[derive(Debug)]
pub enum EditorError {
    Window(baseview::Error),
    Panicked(&'static str),
}

impl fmt::Display for EditorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditorError::Window(error) => write!(f, "{error}"),
            EditorError::Panicked(what) => write!(f, "{what} panicked"),
        }
    }
}

impl Error for EditorError {}

/// `call`'s result, or `fallback`'s (logged) if it panics.
fn guarded<R>(what: &str, fallback: impl FnOnce() -> R, call: impl FnOnce() -> R) -> R {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        nice_plug::nice_error!("the editor panicked while {what}");
        fallback()
    })
}

/// A window call under [`guarded`].
fn window_call(
    what: &'static str,
    call: impl FnOnce() -> Result<(), baseview::Error>,
) -> Result<(), EditorError> {
    guarded(
        what,
        || Err(EditorError::Panicked(what)),
        || call().map_err(EditorError::Window),
    )
}

impl nice_plug::editor::Editor for Editor {
    type Handle = EditorHandle;

    fn spawn(
        &self,
        parent: Option<ParentWindowHandle>,
        wait_for_parent: bool,
        fallback_scale_factor: Option<f64>,
        gui_context: GuiContext,
        host: Option<HostMethods>,
    ) -> Result<SpawnedEditor<EditorHandle>, Box<dyn Error>> {
        guarded(
            "opening the window",
            || Err("opening the editor window panicked".into()),
            || {
                let spawned = self.0.spawn(
                    parent,
                    wait_for_parent,
                    fallback_scale_factor,
                    gui_context,
                    host,
                )?;
                Ok(SpawnedEditor {
                    handle: EditorHandle(spawned.handle),
                    window: EditorWindow(ManuallyDrop::new(spawned.window)),
                })
            },
        )
    }

    fn size(&self) -> NativeSize<u32> {
        guarded(
            "reporting its size",
            || {
                let size = LogicalSize::new(DEFAULT_WIDTH as f64, DEFAULT_HEIGHT as f64);
                NativeSize::from_size(size.into(), 1.0)
            },
            || self.0.size(),
        )
    }

    fn resize_hint(&self) -> ResizeHint {
        guarded("reporting its resize hint", ResizeHint::default, || {
            self.0.resize_hint()
        })
    }

    fn track_info_updated(&self, info: TrackInfo) {
        guarded(
            "taking track info",
            || (),
            || self.0.track_info_updated(info),
        )
    }

    unsafe fn assume_standalone_in_process(&self) {
        // SAFETY: the caller upholds the inner editor's contract.
        guarded(
            "going standalone",
            || (),
            || unsafe { self.0.assume_standalone_in_process() },
        )
    }
}

impl nice_plug::editor::EditorHandle for EditorHandle {
    type Window = EditorWindow;
    type Error = EditorError;

    fn run_until_closed(mut window: EditorWindow) -> Result<(), EditorError> {
        // SAFETY: `window` is forgotten, so the inner window is taken once.
        let inner = unsafe { ManuallyDrop::take(&mut window.0) };
        std::mem::forget(window);
        window_call("running the window", || {
            EguiEditorHandle::run_until_closed(inner)
        })
    }

    fn set_parent(
        &self,
        parent: ParentWindowHandle,
        window: &EditorWindow,
    ) -> Result<(), EditorError> {
        window_call("setting its parent", || {
            self.0.set_parent(parent, &window.0)
        })
    }

    fn show(&self, window: &EditorWindow) -> Result<(), EditorError> {
        window_call("showing the window", || self.0.show(&window.0))
    }

    fn hide(&self, window: &EditorWindow) -> Result<(), EditorError> {
        window_call("hiding the window", || self.0.hide(&window.0))
    }

    fn set_size(
        &self,
        new_size: NativeSize<u32>,
        window: &EditorWindow,
    ) -> Result<(), EditorError> {
        window_call("resizing the window", || {
            self.0.set_size(new_size, &window.0)
        })
    }

    fn host_main_thread_callback(&self, window: &EditorWindow) {
        guarded(
            "running a main-thread callback",
            || (),
            || self.0.host_main_thread_callback(&window.0),
        )
    }

    fn adjust_size(
        &self,
        new_size: NativeSize<u32>,
        window: &EditorWindow,
    ) -> Option<NativeSize<u32>> {
        guarded(
            "adjusting a size",
            || None,
            || self.0.adjust_size(new_size, &window.0),
        )
    }

    fn set_fallback_scale_factor(
        &self,
        scale_factor: f64,
        window: &EditorWindow,
    ) -> Result<(), EditorError> {
        window_call("setting its scale factor", || {
            self.0.set_fallback_scale_factor(scale_factor, &window.0)
        })
    }

    fn on_virtual_key_from_host(
        &self,
        key_code: VirtualKeyCode,
        is_down: bool,
        modifiers: Modifiers,
    ) -> bool {
        guarded(
            "taking a host key",
            || false,
            || {
                self.0
                    .on_virtual_key_from_host(key_code, is_down, modifiers)
            },
        )
    }

    fn state_changed(&self) {
        guarded("taking a state change", || (), || self.0.state_changed())
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        guarded(
            "taking a parameter change",
            || (),
            || self.0.param_value_changed(id, normalized_value),
        )
    }

    fn param_modulation_changed(&self, id: &str, modulation_offset: f32) {
        guarded(
            "taking a modulation change",
            || (),
            || self.0.param_modulation_changed(id, modulation_offset),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Records `set_editor_open` calls.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<bool>>);

    impl EditorModel for Recorder {
        fn preset_name(&self) -> String {
            String::new()
        }

        fn set_editor_open(&self, open: bool) {
            self.0.lock().unwrap().push(open);
        }
    }

    fn calls(model: &Recorder) -> Vec<bool> {
        model.0.lock().unwrap().clone()
    }

    #[test]
    fn a_failed_window_stops_counting_as_open_once() {
        let model = Arc::new(Recorder::default());
        let window = OpenWindow::new(model.clone());
        let at_close = window.clone();
        window.open();
        // The failure hook, then the host closing the inert window.
        window.close();
        at_close.close();
        assert_eq!(calls(&model), [true, false]);
    }

    #[test]
    fn a_window_counts_from_open_to_its_first_close() {
        let model = Arc::new(Recorder::default());
        let window = OpenWindow::new(model.clone());
        // Closed before it opened (build failed part-way): nothing to give
        // back.
        window.close();
        assert_eq!(calls(&model), Vec::<bool>::new());
        window.open();
        window.open();
        window.close();
        assert_eq!(calls(&model), [true, false]);
    }
}
