// Copyright 2013-2019 Matt Tytel (upstream); synth-a-size contributors (port).
// SPDX-License-Identifier: GPL-3.0-or-later

//! The plugin editor window: [`Interface`] in a nice-plug egui editor drawn
//! with wgpu (Metal on macOS).
//!
//! Window behaviour follows upstream's plugin editor
//! (`vital/src/plugin/synth_editor.cpp`, `common/border_bounds_constrainer.cpp`):
//! each window opens at 1400 × 820 scaled by the size last saved in the
//! config (`LoadSave::loadWindowSize`), and the host may resize it down to
//! 350 × 205, keeping the 1400:820 aspect ratio. Once a new size has held,
//! the editor saves it (`BorderBoundsConstrainer::resizeEnd`), for the next
//! window to open at. Upstream also keeps the first size within the screen,
//! which baseview can't report (D30).
//!
//! The chat's drawer (D13) widens the window beside that layout while it is
//! open ([`crate::drawer`]): the editor asks the host for the wider window,
//! and the sizes it accepts keep room for the drawer.

use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::mem::ManuallyDrop;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;
use std::sync::Arc;

use nice_plug::context::gui::GuiContext;
use nice_plug::context::PluginApi;
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

use crate::chat::{ChatHost, ChatPanel};
use crate::drawer::{self, DrawerState};
use crate::fonts;
use crate::host::{Host, OpenGestures};
use crate::interface::{EditorModel, Interface, PRODUCT_NAME};
use crate::layout::{DEFAULT_HEIGHT, DEFAULT_WIDTH};

/// `vital::kMinWindowWidth` / `kMinWindowHeight`.
pub const MIN_WIDTH: f32 = 350.0;
pub const MIN_HEIGHT: f32 = 205.0;

/// What a plugin instance keeps of its editor while the window is closed:
/// the window's size and whether the chat's drawer is open.
#[derive(Clone, Debug)]
pub struct EditorState {
    pub window: Arc<EguiEditorState>,
    pub drawer: DrawerState,
}

/// The editor's state at the default size, with the drawer closed. Keep one
/// per plugin instance so they survive closing and reopening the window.
pub fn default_state() -> EditorState {
    state_for_window_size(1.0)
}

/// The editor's state at upstream's size scaled by `window_size`, with the
/// drawer closed.
pub fn state_for_window_size(window_size: f32) -> EditorState {
    let size = window_logical_size(window_size, false);
    EditorState {
        window: EguiEditorState::from_size(size, 1.0),
        drawer: DrawerState::default(),
    }
}

/// `SynthEditor`'s constructor: `kDefaultWindowWidth` and
/// `kDefaultWindowHeight` times `window_size`, rounded, no smaller than the
/// minimum window, widened by the drawer when it is `drawer_open`.
pub fn window_logical_size(window_size: f32, drawer_open: bool) -> LogicalSize<f64> {
    let width = (window_size * DEFAULT_WIDTH as f32).round().max(MIN_WIDTH);
    let height = (window_size * DEFAULT_HEIGHT as f32)
        .round()
        .max(MIN_HEIGHT);
    let drawer = if drawer_open {
        drawer::width_pixels(height, 1.0)
    } else {
        0.0
    };
    LogicalSize::new((width + drawer) as f64, height as f64)
}

/// How long a new editor size must hold before it is saved: a drag's
/// sizes in between aren't (upstream saves at `resizeEnd`).
pub const WINDOW_SIZE_SETTLE: f64 = 0.5;

/// Follows the editor's size and says when to save it: a size other than
/// the one last seen, once it has held for [`WINDOW_SIZE_SETTLE`] seconds.
/// The first size seen is the one the window opened at, not saved.
#[derive(Clone, Debug, Default)]
pub struct WindowSizeSaver {
    last: Option<f32>,
    changed: Option<(f32, f64)>,
}

impl WindowSizeSaver {
    /// The editor at `window_size` (its width over 1400) at time `now`:
    /// the size to save, if one is due.
    pub fn observe(&mut self, window_size: f32, now: f64) -> Option<f32> {
        let same = |a: f32, b: f32| (a - b).abs() < 1e-4;
        let Some(last) = self.last else {
            self.last = Some(window_size);
            return None;
        };
        if same(window_size, last) {
            self.changed = None;
            return None;
        }
        match self.changed {
            Some((size, since)) if same(size, window_size) => {
                if now - since < WINDOW_SIZE_SETTLE {
                    return None;
                }
                self.last = Some(window_size);
                self.changed = None;
                Some(window_size)
            }
            _ => {
                self.changed = Some((window_size, now));
                None
            }
        }
    }

    /// Whether a size is waiting to hold.
    pub fn is_waiting(&self) -> bool {
        self.changed.is_some()
    }
}

/// The editor for one plugin instance.
pub fn create(state: EditorState, model: Arc<dyn EditorModel>) -> Option<Editor> {
    let resize_hint = nice_plug::prelude::ResizeHint {
        preserve_aspect_ratio: true,
        ..nice_plug::prelude::ResizeHint::resizable()
            .with_min_logical_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
            .with_aspect_ratio(DEFAULT_WIDTH as u32, DEFAULT_HEIGHT as u32)
    };
    let settings = EguiNiceSettings::default()
        .with_tile(PRODUCT_NAME)
        .with_resize_hint(resize_hint);
    let app = App::new(model.clone(), state.drawer.clone());
    create_egui_editor(state.window.clone(), Default::default(), settings, app).map(|editor| {
        Editor {
            inner: editor,
            drawer: state.drawer,
            window: state.window,
            model,
        }
    })
}

/// The egui app behind [`Editor`].
pub struct App {
    model: Arc<dyn EditorModel>,
    interface: Interface,
    /// The host connection, from the window's `build` until it closes.
    gui_context: Option<GuiContext>,
    open_gestures: Arc<OpenGestures>,
    chat: ChatPanel,
    /// The drawer state the window's width last followed (`None` before
    /// the first frame).
    drawer_followed: Option<bool>,
    drawer: DrawerState,
    /// A size asked of the host for the drawer, until it answers.
    resize_pending: Option<drawer::PendingResize>,
    window_size: WindowSizeSaver,
    #[cfg(feature = "focus-spike")]
    focus_spike: crate::focus_spike::FocusSpike,
}

impl App {
    fn new(model: Arc<dyn EditorModel>, drawer: DrawerState) -> App {
        let mut interface = Interface::default();
        interface.set_drawer_state(drawer.clone());
        App {
            model,
            interface,
            gui_context: None,
            open_gestures: Arc::default(),
            chat: ChatPanel::default(),
            drawer_followed: None,
            drawer,
            resize_pending: None,
            window_size: WindowSizeSaver::default(),
            #[cfg(feature = "focus-spike")]
            focus_spike: Default::default(),
        }
    }
}

impl App {
    /// Asks the host to widen the window by the drawer's width when the
    /// drawer opens, and to give the width back when it closes. A host that
    /// refuses leaves the window as it was, and the chat overlays the
    /// editor instead (`drawer::split`).
    fn follow_drawer(&mut self, ctx: &egui::Context, window: egui::Rect) {
        let now = ctx.input(|input| input.time);
        if let Some(pending) = self.resize_pending {
            match pending.answer(self.drawer.host_size(), now) {
                drawer::HostAnswer::Waiting => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
                drawer::HostAnswer::Answered => self.resize_pending = None,
                drawer::HostAnswer::Refused => {
                    self.resize_pending = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(pending.previous));
                }
            }
        }
        let open = self.interface.drawer_open();
        if let Some(size) =
            drawer_resize(self.drawer_followed, open, window, ctx.pixels_per_point())
        {
            let api = self.gui_context.as_ref().map(GuiContext::plugin_api);
            if waits_for_host_answer(api) {
                self.drawer.forget_host_size();
                self.resize_pending = Some(drawer::PendingResize {
                    previous: window.size(),
                    asked_at: now,
                });
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        self.drawer_followed = Some(open);
    }
}

impl App {
    /// Saves the editor's share of the window, without the drawer, once a
    /// new size has held; not while the drawer's resize awaits the host.
    fn save_window_size(&mut self, ctx: &egui::Context, window: egui::Rect) {
        if self.resize_pending.is_some() {
            return;
        }
        let editor =
            drawer::split(window, ctx.pixels_per_point(), self.interface.drawer_open()).editor;
        let now = ctx.input(|input| input.time);
        if let Some(size) = self
            .window_size
            .observe(editor.width() / DEFAULT_WIDTH as f32, now)
        {
            self.model.save_window_size(size);
        }
        if self.window_size.is_waiting() {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(WINDOW_SIZE_SETTLE));
        }
    }
}

/// Whether a size the editor asks for awaits the host's answer. Only VST3
/// answers with a call (`onSize`, through `set_size`) and drops a refusal
/// on the way; a CLAP host that accepts `gui.request_resize` needn't call
/// `set_size`, and one that refuses answers at once with an error, on which
/// baseview puts the view back itself. The standalone wrapper never calls
/// `set_size` either (the plugin exports only CLAP and VST3).
pub fn waits_for_host_answer(api: Option<PluginApi>) -> bool {
    api == Some(PluginApi::Vst3)
}

/// The resize hint while the drawer is `open`: upstream's width plus the
/// drawer's to the height, 1760:820, which holds wherever the drawer
/// follows the editor's scale (0.5 to 2). `adjust_size` keeps upstream's
/// ratio for the editor's part at every size.
pub fn drawer_resize_hint(hint: ResizeHint, open: bool) -> ResizeHint {
    if !open || !hint.preserve_aspect_ratio {
        return hint;
    }
    ResizeHint {
        aspect_ratio_width: DEFAULT_WIDTH as u32 + crate::chat::PANEL_WIDTH as u32,
        aspect_ratio_height: DEFAULT_HEIGHT as u32,
        ..hint
    }
}

/// One frame of the chat in a window at `window` (points), after the
/// interface's: the panel's toggle opens and closes the drawer, whose state
/// the interface keeps.
pub fn show_chat(
    interface: &mut Interface,
    panel: &mut ChatPanel,
    ctx: &egui::Context,
    window: egui::Rect,
    host: &dyn ChatHost,
    request_focus: &mut dyn FnMut(),
) {
    let placement = interface.chat_placement(window, ctx.pixels_per_point());
    panel.set_open(interface.drawer_open());
    panel.show(ctx, &placement, host, request_focus);
    interface.set_drawer_open(panel.is_open());
}

/// The window size (points) to ask for when the drawer's state goes from
/// `followed` to `open` in a window at `window`: wider by the drawer when it
/// opens without room, narrower by it when it closes docked. Nothing on the
/// first frame, which keeps the size the instance saved.
pub fn drawer_resize(
    followed: Option<bool>,
    open: bool,
    window: egui::Rect,
    pixels_per_point: f32,
) -> Option<egui::Vec2> {
    if followed.is_none_or(|followed| followed == open) {
        return None;
    }
    let docked = drawer::split(window, pixels_per_point, true)
        .drawer
        .is_some();
    let width = drawer::width_pixels(window.height() * pixels_per_point, pixels_per_point)
        / pixels_per_point;
    let new_width = match (open, docked) {
        (true, false) => window.width() + width,
        (false, true) => window.width() - width,
        _ => return None,
    };
    Some(egui::vec2(new_width, window.height()))
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
        crate::controls::file_dialog::show_alerts(ui.ctx());
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
            show_chat(
                &mut self.interface,
                &mut self.chat,
                ui.ctx(),
                area,
                chat,
                &mut || {
                    let _ = frame.baseview_window().focus();
                },
            );
        }
        self.follow_drawer(ui.ctx(), area);
        self.save_window_size(ui.ctx(), area);
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
pub struct Editor {
    inner: EguiEditor<App>,
    drawer: DrawerState,
    window: Arc<EguiEditorState>,
    model: Arc<dyn EditorModel>,
}

impl Editor {
    /// `SynthEditor`'s constructor, run each time upstream's window opens:
    /// while no window is open, the next one is to open at the size saved
    /// in the config, wider by the drawer when it is open. Nothing changes
    /// without a saved size (a model with no config), or while a window is
    /// open.
    pub fn reload_window_size(&self) {
        if self.window.is_open() {
            return;
        }
        if let Some(window_size) = self.model.window_size() {
            let size = window_logical_size(window_size, self.drawer.is_open());
            self.window.set_size(size);
        }
    }
}

/// [`Editor`]'s handle, its calls guarded the same way.
pub struct EditorHandle {
    inner: EguiEditorHandle,
    drawer: DrawerState,
}

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
                self.reload_window_size();
                let spawned = self.inner.spawn(
                    parent,
                    wait_for_parent,
                    fallback_scale_factor,
                    gui_context,
                    host,
                )?;
                Ok(SpawnedEditor {
                    handle: EditorHandle {
                        inner: spawned.handle,
                        drawer: self.drawer.clone(),
                    },
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
            || {
                // The host asks before it opens a window, for the frame to
                // give it.
                self.reload_window_size();
                self.inner.size()
            },
        )
    }

    fn resize_hint(&self) -> ResizeHint {
        guarded("reporting its resize hint", ResizeHint::default, || {
            drawer_resize_hint(self.inner.resize_hint(), self.drawer.is_open())
        })
    }

    fn track_info_updated(&self, info: TrackInfo) {
        guarded(
            "taking track info",
            || (),
            || self.inner.track_info_updated(info),
        )
    }

    unsafe fn assume_standalone_in_process(&self) {
        // SAFETY: the caller upholds the inner editor's contract.
        guarded(
            "going standalone",
            || (),
            || unsafe { self.inner.assume_standalone_in_process() },
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
            self.inner.set_parent(parent, &window.0)
        })
    }

    fn show(&self, window: &EditorWindow) -> Result<(), EditorError> {
        window_call("showing the window", || self.inner.show(&window.0))
    }

    fn hide(&self, window: &EditorWindow) -> Result<(), EditorError> {
        window_call("hiding the window", || self.inner.hide(&window.0))
    }

    fn set_size(
        &self,
        new_size: NativeSize<u32>,
        window: &EditorWindow,
    ) -> Result<(), EditorError> {
        // The host sets the size (VST3's `onSize`), answering any the
        // editor asked for.
        self.drawer.host_resized(new_size.width, new_size.height);
        window_call("resizing the window", || {
            self.inner.set_size(new_size, &window.0)
        })
    }

    fn host_main_thread_callback(&self, window: &EditorWindow) {
        guarded(
            "running a main-thread callback",
            || (),
            || self.inner.host_main_thread_callback(&window.0),
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
            || {
                // An open drawer keeps its width beside upstream's sizes.
                if !self.drawer.is_open() {
                    return self.inner.adjust_size(new_size, &window.0);
                }
                let scale_factor = window.0.size().scale_factor as f32;
                let mut refused = false;
                let (width, height) = drawer::adjust_size(
                    (new_size.width, new_size.height),
                    scale_factor,
                    |(width, height)| {
                        let size = NativeSize { width, height };
                        match self.inner.adjust_size(size, &window.0) {
                            Some(size) => (size.width, size.height),
                            None => {
                                refused = true;
                                (width, height)
                            }
                        }
                    },
                );
                (!refused).then_some(NativeSize { width, height })
            },
        )
    }

    fn set_fallback_scale_factor(
        &self,
        scale_factor: f64,
        window: &EditorWindow,
    ) -> Result<(), EditorError> {
        window_call("setting its scale factor", || {
            self.inner
                .set_fallback_scale_factor(scale_factor, &window.0)
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
                self.inner
                    .on_virtual_key_from_host(key_code, is_down, modifiers)
            },
        )
    }

    fn state_changed(&self) {
        guarded(
            "taking a state change",
            || (),
            || self.inner.state_changed(),
        )
    }

    fn param_value_changed(&self, id: &str, normalized_value: f32) {
        guarded(
            "taking a parameter change",
            || (),
            || self.inner.param_value_changed(id, normalized_value),
        )
    }

    fn param_modulation_changed(&self, id: &str, modulation_offset: f32) {
        guarded(
            "taking a modulation change",
            || (),
            || self.inner.param_modulation_changed(id, modulation_offset),
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

    /// Records `save_window_size` calls.
    #[derive(Default)]
    struct SizeRecorder(Mutex<Vec<f32>>);

    impl EditorModel for SizeRecorder {
        fn preset_name(&self) -> String {
            String::new()
        }

        fn save_window_size(&self, window_size: f32) {
            self.0.lock().unwrap().push(window_size);
        }
    }

    /// `SynthEditor`'s constructor: upstream's size scaled and rounded, no
    /// smaller than the minimum window.
    #[test]
    fn the_state_opens_at_the_saved_size() {
        let size = |window_size| {
            let state = state_for_window_size(window_size);
            let size = state.window.size().to_logical::<f64>(1.0);
            (size.width, size.height)
        };
        assert_eq!(size(1.0), (1400.0, 820.0));
        assert_eq!(size(1.5), (2100.0, 1230.0));
        assert_eq!(size(0.6005), (841.0, 492.0));
        assert_eq!(size(0.1), (350.0, 205.0));
    }

    /// A model with a saved window size.
    struct Saved(Option<f32>);

    impl EditorModel for Saved {
        fn preset_name(&self) -> String {
            String::new()
        }

        fn window_size(&self) -> Option<f32> {
            self.0
        }
    }

    /// Each window opens at the saved size (`SynthEditor`'s constructor runs
    /// at every opening), wider by the drawer when it is open; a model with
    /// no config leaves the size alone.
    #[test]
    fn each_window_opens_at_the_saved_size() {
        let logical = |state: &EditorState| {
            let size = state.window.size().to_logical::<f64>(1.0);
            (size.width, size.height)
        };
        let state = default_state();
        let editor = create(state.clone(), Arc::new(Saved(Some(1.5)))).unwrap();
        editor.reload_window_size();
        assert_eq!(logical(&state), (2100.0, 1230.0));
        state.drawer.set_open(true);
        editor.reload_window_size();
        let drawer = drawer::width_pixels(1230.0, 1.0) as f64;
        assert_eq!(logical(&state), (2100.0 + drawer, 1230.0));

        let state = default_state();
        let editor = create(state.clone(), Arc::new(Saved(None))).unwrap();
        editor.reload_window_size();
        assert_eq!(logical(&state), (1400.0, 820.0));
    }

    /// A new size is saved once it has held for half a second: not the size
    /// the window opened at, and not the sizes a drag passes through.
    #[test]
    fn a_size_is_saved_once_it_holds() {
        let mut saver = WindowSizeSaver::default();
        assert_eq!(saver.observe(1.0, 0.0), None);
        assert_eq!(saver.observe(1.1, 1.0), None);
        assert_eq!(saver.observe(1.2, 1.2), None);
        assert_eq!(saver.observe(1.2, 1.6), None);
        assert!(saver.is_waiting());
        assert_eq!(saver.observe(1.2, 1.71), Some(1.2));
        assert!(!saver.is_waiting());
        assert_eq!(saver.observe(1.2, 5.0), None);
        // Back to where it was before it settles: nothing to save.
        assert_eq!(saver.observe(0.8, 6.0), None);
        assert_eq!(saver.observe(1.2, 6.2), None);
        assert!(!saver.is_waiting());
    }

    /// The editor saves its own share of the window: opening the drawer
    /// widens the window and saves nothing; a corner drag saves the
    /// editor's width over 1400 once it holds.
    #[test]
    fn the_editor_saves_its_width_without_the_drawer() {
        let model = Arc::new(SizeRecorder::default());
        let mut app = App::new(model.clone(), DrawerState::default());
        let ctx = egui::Context::default();
        let frame = |app: &mut App, time: f64, width: f32, height: f32| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
                app.save_window_size(ui.ctx(), window);
            });
            output.textures_delta.clear();
        };
        frame(&mut app, 0.0, 1400.0, 820.0);
        app.interface.set_drawer_open(true);
        frame(&mut app, 0.1, 1760.0, 820.0);
        frame(&mut app, 1.0, 1760.0, 820.0);
        assert_eq!(*model.0.lock().unwrap(), Vec::<f32>::new());
        frame(&mut app, 2.0, 2640.0, 1230.0);
        frame(&mut app, 2.6, 2640.0, 1230.0);
        assert_eq!(*model.0.lock().unwrap(), [1.5]);
    }

    /// Only VST3's answer is awaited: CLAP accepts without a call, and
    /// refuses at once.
    #[test]
    fn only_vst3_resizes_wait_for_the_host() {
        assert!(waits_for_host_answer(Some(PluginApi::Vst3)));
        assert!(!waits_for_host_answer(Some(PluginApi::Clap)));
        assert!(!waits_for_host_answer(Some(PluginApi::Standalone)));
        assert!(!waits_for_host_answer(None));
    }

    /// The open drawer's hint is upstream's ratio widened by the drawer,
    /// which is what `adjust_size` makes of a size within the drawer's
    /// scale range; the rest of the hint stays.
    #[test]
    fn the_open_drawers_hint_is_widened_by_it() {
        let hint = ResizeHint {
            preserve_aspect_ratio: true,
            ..ResizeHint::resizable()
                .with_min_logical_size(LogicalSize::new(MIN_WIDTH, MIN_HEIGHT))
                .with_aspect_ratio(DEFAULT_WIDTH as u32, DEFAULT_HEIGHT as u32)
        };
        let open = drawer_resize_hint(hint, true);
        assert!(open.preserve_aspect_ratio);
        assert_eq!(
            (open.aspect_ratio_width, open.aspect_ratio_height),
            (1760, 820)
        );
        let aspect = |(width, _): (u32, u32)| (width, width * 820 / 1400);
        for height in [410, 820, 1640] {
            let width = height * 1760 / 820;
            assert_eq!(
                drawer::adjust_size((width, height), 1.0, aspect),
                (width, height)
            );
        }
        assert!(open.can_resize);
        assert_eq!(open.size_constraints, hint.size_constraints);
        assert_eq!(drawer_resize_hint(hint, false), hint);
    }
}
