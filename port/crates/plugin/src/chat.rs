// SPDX-License-Identifier: GPL-3.0-or-later

//! The chat behind the editor's panel (L5 of the LLM plan; D13 in
//! `migration/DIVERGENCES.md`).
//!
//! [`PluginChat`] is the editor's `ChatHost`. It lives as long as the plugin
//! instance, so the conversation survives closing the window (it is never
//! saved). While the chat is off (`ChatSettings::enabled`, false by default)
//! it starts nothing and sends nothing. Once on, the first message starts a
//! worker thread that runs the model and the tool loop (`sas_llm_agent`);
//! the tools act on the [`LiveSession`] on the GUI thread through the
//! [`ChatBridge`], which the editor runs every frame.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use nice_plug::context::gui::GuiContext;
use sas_gui::chat::{ChatLine, CompareSide, LineKind};
use sas_gui::{ChatHost, ChatSettingsView, ChatView, FocusMode};
use sas_llm_agent::settings::{default_dir, SETTINGS_FILE};
use sas_llm_agent::{
    Agent, AgentConfig, AgentEvent, ChatSettings, KeyboardFocus, ProviderConfig, Transport,
};
use sas_llm_control::{Controller, Policy, PresetLibrary, Side};

use crate::chat_bridge::{ChatBridge, RemoteSession};
use crate::keys::{KeyStore, NoKeys, SystemKeys};
use crate::live::LiveSession;
use crate::params::SynthParams;

/// How long a tool call waits for the editor to run it.
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);

/// How long a closing plugin waits for the chat thread to finish. Closing
/// the bridge ends tool calls at once, so only a model request in flight
/// can take longer: it cannot be interrupted, and runs until it answers or
/// the transport's timeout (120 s). Past this wait the thread is left to end
/// on its own, and must not outlive the plugin's library.
const WORKER_EXIT_WAIT: Duration = Duration::from_secs(2);

/// Builds the transport for a provider: real HTTPS in the macOS plugin.
pub type TransportFactory =
    Box<dyn Fn(&ProviderConfig) -> Result<Box<dyn Transport>, String> + Send + Sync>;

/// The shipped transport: HTTPS through the system's TLS on macOS; other
/// builds have no network code.
pub fn system_transport() -> TransportFactory {
    Box::new(|_config| {
        #[cfg(target_os = "macos")]
        {
            sas_llm_agent::HttpTransport::new()
                .map(|transport| Box::new(transport) as Box<dyn Transport>)
                .map_err(|error| error.to_string())
        }
        #[cfg(not(target_os = "macos"))]
        {
            Err("this build has no network support; the chat runs in the macOS plugin".to_string())
        }
    })
}

enum Command {
    /// A message, with the cancel flag made for it.
    Send(String, Arc<AtomicBool>),
    Undo,
    Compare,
    Confirm(bool),
}

struct Worker {
    commands: Option<mpsc::Sender<Command>>,
    /// This worker's own bridge to the GUI thread, closed when it stops.
    bridge: Arc<ChatBridge>,
    /// Ends the worker's loop; separate from the messages' cancel flags.
    shutdown: Arc<AtomicBool>,
    /// The cancel flag of the last message sent.
    cancel: Mutex<Option<Arc<AtomicBool>>>,
    /// Disconnects when the thread ends; `None` once retired.
    exited: Option<mpsc::Receiver<()>>,
}

impl Worker {
    /// Stops the message being answered after its model call.
    fn stop(&self) {
        if let Some(cancel) = &*lock(&self.cancel) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Ends the thread: no further command or tool call runs.
    fn shut_down(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        self.stop();
        self.bridge.close();
        self.commands = None;
    }

    /// Stops the worker without waiting for it (the plugin stays loaded).
    fn retire(mut self) {
        self.shut_down();
        self.exited = None;
    }
}

impl Drop for Worker {
    /// The plugin is going away (and its library may be unloaded after):
    /// stop the thread and wait for it, up to [`WORKER_EXIT_WAIT`].
    fn drop(&mut self) {
        self.shut_down();
        if let Some(exited) = &self.exited {
            let _ = exited.recv_timeout(WORKER_EXIT_WAIT);
        }
    }
}

/// What the worker and the editor share.
#[derive(Default)]
struct Shared {
    view: Mutex<ChatView>,
    /// Counts started workers; only the newest one updates the view's state.
    worker: AtomicU64,
    wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Shared {
    fn view(&self) -> MutexGuard<'_, ChatView> {
        self.view.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn push(&self, kind: LineKind, text: impl Into<String>) {
        self.view().lines.push(ChatLine {
            kind,
            text: text.into(),
        });
        self.wake();
    }

    /// [`push`](Self::push) from worker `generation`; dropped if a newer
    /// worker has started (a retired worker's lines never reach the new
    /// conversation).
    fn push_from(&self, generation: u64, kind: LineKind, text: impl Into<String>) {
        let mut view = self.view();
        if self.worker.load(Ordering::Acquire) != generation {
            return;
        }
        view.lines.push(ChatLine {
            kind,
            text: text.into(),
        });
        drop(view);
        self.wake();
    }

    fn wake(&self) {
        if let Some(wake) = &*self.wake.lock().unwrap_or_else(|p| p.into_inner()) {
            wake();
        }
    }
}

/// The chat of one plugin instance.
pub struct PluginChat {
    params: Arc<SynthParams>,
    shared: Arc<Shared>,
    /// The tools' session on the GUI thread, made when the editor opens.
    live: Mutex<Option<LiveSession<GuiContext>>>,
    worker: Mutex<Option<Worker>>,
    settings: Mutex<ChatSettings>,
    settings_path: Option<PathBuf>,
    keys: Box<dyn KeyStore>,
    /// The provider's key as last read, so the Keychain is read once per
    /// provider, not every frame.
    key: Mutex<Option<(String, Option<String>)>>,
    transports: TransportFactory,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

impl PluginChat {
    /// The chat with the user's settings file, the system key store and the
    /// system transport.
    pub fn new(params: Arc<SynthParams>) -> PluginChat {
        PluginChat::with(
            params,
            default_dir().map(|dir| dir.join(SETTINGS_FILE)),
            Box::new(SystemKeys),
            system_transport(),
        )
    }

    /// A chat with no settings file, no keys and no network: it stays off.
    /// Tests and in-memory plugins use this, so they never read the user's
    /// settings or the Keychain.
    pub fn in_memory(params: Arc<SynthParams>) -> PluginChat {
        PluginChat::with(
            params,
            None,
            Box::new(NoKeys),
            Box::new(|_config| Err("this plugin has no network access".to_string())),
        )
    }

    pub fn with(
        params: Arc<SynthParams>,
        settings_path: Option<PathBuf>,
        keys: Box<dyn KeyStore>,
        transports: TransportFactory,
    ) -> PluginChat {
        let settings = settings_path
            .as_deref()
            .map(|path| ChatSettings::load(path).0)
            .unwrap_or_default();
        PluginChat {
            params,
            shared: Arc::new(Shared::default()),
            live: Mutex::new(None),
            worker: Mutex::new(None),
            settings: Mutex::new(settings),
            settings_path,
            keys,
            key: Mutex::new(None),
            transports,
        }
    }

    /// The editor opened: the session for its `GuiContext`, and how to wake
    /// it.
    pub fn attach(&self, gui: GuiContext, wake: Arc<dyn Fn() + Send + Sync>) {
        *lock(&self.live) = Some(LiveSession::new(self.params.clone(), gui));
        *lock(&self.shared.wake) = Some(wake);
    }

    fn key(&self, provider: &str) -> Option<String> {
        let mut cached = lock(&self.key);
        match &*cached {
            Some((cached_provider, key)) if cached_provider == provider => key.clone(),
            _ => {
                let key = self.keys.get(provider);
                *cached = Some((provider.to_string(), key.clone()));
                key
            }
        }
    }

    /// The provider to use. The key store is not read while the chat is off.
    fn provider(&self) -> Result<ProviderConfig, String> {
        let settings = lock(&self.settings).clone();
        let key = if settings.enabled {
            self.key(&settings.provider)
        } else {
            None
        };
        settings.provider_config(key)
    }

    /// The worker, started if the chat is on and can run.
    fn worker(&self) -> Result<MutexGuard<'_, Option<Worker>>, String> {
        let mut worker = lock(&self.worker);
        if worker.is_none() {
            let provider = self.provider()?;
            let transport = (self.transports)(&provider)?;
            let library = lock(&self.settings).preset_folder().map(PresetLibrary::new);
            *worker = Some(spawn_worker(
                provider,
                transport,
                library,
                self.shared.clone(),
            ));
        }
        Ok(worker)
    }

    /// Hands `command` to the worker, starting it if needed. The chat is
    /// busy until the worker has run it.
    fn command(&self, command: Command) {
        match self.worker() {
            Ok(worker) => {
                let Some(worker) = worker.as_ref() else {
                    return;
                };
                if let Command::Send(_, cancel) = &command {
                    *lock(&worker.cancel) = Some(cancel.clone());
                }
                if let Some(commands) = &worker.commands {
                    self.shared.view().busy = true;
                    if commands.send(command).is_err() {
                        self.shared.view().busy = false;
                    }
                }
            }
            Err(error) => {
                self.shared.view().busy = false;
                self.shared.push(LineKind::Error, error);
            }
        }
    }
}

fn spawn_worker(
    provider: ProviderConfig,
    transport: Box<dyn Transport>,
    library: Option<PresetLibrary>,
    shared: Arc<Shared>,
) -> Worker {
    let (commands, receiver) = mpsc::channel::<Command>();
    let (exited_sender, exited) = mpsc::channel::<()>();
    let bridge = ChatBridge::new();
    let waker = shared.clone();
    bridge.set_waker(move || waker.wake());
    let mut controller = Controller::new(bridge.remote(TOOL_TIMEOUT), Policy::default());
    if let Some(library) = library {
        controller = controller.with_library(library);
    }
    let mut agent = Agent::new(
        controller,
        provider.build(),
        transport,
        AgentConfig::default(),
    );
    let shutdown = Arc::new(AtomicBool::new(false));
    let stopping = shutdown.clone();
    let generation = shared.worker.fetch_add(1, Ordering::AcqRel) + 1;
    std::thread::Builder::new()
        .name("synth-a-size chat".to_string())
        .spawn(move || {
            let _exited = exited_sender;
            for command in receiver {
                if stopping.load(Ordering::Relaxed) {
                    break;
                }
                run_command(&mut agent, command, &shared, generation);
                let controller = agent.controller();
                let mut view = shared.view();
                if shared.worker.load(Ordering::Acquire) != generation {
                    continue;
                }
                view.busy = false;
                view.can_undo = controller.undo_depth() > 0;
                view.comparing = controller.comparing().map(|side| match side {
                    Side::Before => CompareSide::Before,
                    Side::After => CompareSide::After,
                });
                view.can_compare = view.can_undo || view.comparing.is_some();
                view.pending = controller.pending().map(|pending| pending.reason.clone());
                drop(view);
                shared.wake();
            }
        })
        .expect("spawn the chat thread");
    Worker {
        commands: Some(commands),
        bridge,
        shutdown,
        cancel: Mutex::new(None),
        exited: Some(exited),
    }
}

fn run_command(
    agent: &mut Agent<RemoteSession>,
    command: Command,
    shared: &Shared,
    generation: u64,
) {
    match command {
        Command::Send(text, cancel) => {
            agent.set_cancel_flag(cancel);
            let result = agent.send(&text, &mut |event| match event {
                AgentEvent::Text(text) => shared.push_from(generation, LineKind::Assistant, text),
                AgentEvent::ToolCall { name, input } => {
                    shared.push_from(
                        generation,
                        LineKind::Tool,
                        format!("{name} {}", summary(&input.to_string())),
                    );
                }
                AgentEvent::ToolResult { output, .. } if output.is_error => {
                    shared.push_from(
                        generation,
                        LineKind::Tool,
                        format!("error: {}", output.content),
                    );
                }
                AgentEvent::ToolResult { .. } => {}
            });
            match result {
                Ok(report) => {
                    if let Some(stopped) = report.stopped {
                        shared.push_from(generation, LineKind::Note, format!("({stopped})"));
                    }
                }
                Err(error) => shared.push_from(generation, LineKind::Error, error.to_string()),
            }
        }
        Command::Undo => match agent.controller_mut().undo(1, None) {
            Ok(0) => shared.push_from(generation, LineKind::Note, "Nothing to undo."),
            Ok(steps) => {
                agent.note_user_undo(steps);
                shared.push_from(generation, LineKind::Note, "Undid the last change.");
            }
            Err(error) => shared.push_from(generation, LineKind::Error, error.to_string()),
        },
        Command::Compare => match agent.controller_mut().compare_toggle() {
            Ok(Some(Side::Before)) => {
                shared.push_from(generation, LineKind::Note, "Playing the version before.")
            }
            Ok(Some(Side::After)) => {
                shared.push_from(generation, LineKind::Note, "Playing the version after.")
            }
            Ok(None) => shared.push_from(generation, LineKind::Note, "Nothing to compare yet."),
            Err(error) => shared.push_from(generation, LineKind::Error, error.to_string()),
        },
        Command::Confirm(accept) => match agent.resolve_pending(accept) {
            Ok(true) if accept => shared.push_from(generation, LineKind::Note, "Applied."),
            Ok(true) => shared.push_from(generation, LineKind::Note, "Declined; nothing changed."),
            Ok(false) => {}
            Err(error) => shared.push_from(generation, LineKind::Error, error.to_string()),
        },
    }
}

/// `text` cut to about 100 characters for the conversation.
pub fn summary(text: &str) -> String {
    if text.chars().count() <= 100 {
        return text.to_string();
    }
    let mut short: String = text.chars().take(97).collect();
    short.push_str("...");
    short
}

impl ChatHost for PluginChat {
    fn run_frame(&self) {
        let bridge = lock(&self.worker)
            .as_ref()
            .map(|worker| worker.bridge.clone());
        if let (Some(bridge), Some(session)) = (bridge, &mut *lock(&self.live)) {
            bridge.run_pending(session);
        }
    }

    fn view(&self) -> ChatView {
        let mut view = self.shared.view().clone();
        view.enabled = lock(&self.settings).enabled;
        view.problem = if view.enabled {
            self.provider().err()
        } else {
            None
        };
        view
    }

    fn send(&self, text: String) {
        self.shared.push(LineKind::User, text.clone());
        self.command(Command::Send(text, Arc::new(AtomicBool::new(false))));
    }

    fn stop(&self) {
        if let Some(worker) = &*lock(&self.worker) {
            worker.stop();
        }
    }

    fn undo(&self) {
        self.command(Command::Undo);
    }

    fn compare(&self) {
        self.command(Command::Compare);
    }

    fn confirm(&self, accept: bool) {
        self.command(Command::Confirm(accept));
    }

    fn settings(&self) -> ChatSettingsView {
        let settings = lock(&self.settings).clone();
        ChatSettingsView {
            enabled: settings.enabled,
            has_key: settings.enabled && self.key(&settings.provider).is_some(),
            model: settings.model.clone().unwrap_or_default(),
            base_url: settings.base_url.clone().unwrap_or_default(),
            focus: match settings.keyboard_focus {
                KeyboardFocus::Grab => FocusMode::GrabOnClick,
                KeyboardFocus::Plain => FocusMode::Plain,
            },
            provider: settings.provider,
        }
    }

    fn save_settings(&self, view: ChatSettingsView, api_key: Option<String>) -> Result<(), String> {
        if let Some(key) = &api_key {
            self.keys.set(&view.provider, key)?;
        }
        *lock(&self.key) = None;
        let mut settings = lock(&self.settings).clone();
        settings.enabled = view.enabled;
        settings.provider = view.provider;
        let text = |text: String| (!text.trim().is_empty()).then(|| text.trim().to_string());
        settings.model = text(view.model);
        settings.base_url = text(view.base_url);
        settings.keyboard_focus = match view.focus {
            FocusMode::GrabOnClick => KeyboardFocus::Grab,
            FocusMode::Plain => KeyboardFocus::Plain,
        };
        if let Some(path) = &self.settings_path {
            settings.save(path)?;
        }
        *lock(&self.settings) = settings;
        // The next message starts a worker with the new provider and key (and
        // a new conversation); the old one stops after its model call.
        if let Some(worker) = lock(&self.worker).take() {
            let mut view = self.shared.view();
            // Retired: its lines and state no longer reach the view.
            self.shared.worker.fetch_add(1, Ordering::AcqRel);
            drop(view);
            worker.retire();
            view = self.shared.view();
            view.busy = false;
            view.can_undo = false;
            view.can_compare = false;
            view.comparing = None;
            view.pending = None;
            drop(view);
            self.shared.push(
                LineKind::Note,
                "(Settings changed; a new conversation starts.)",
            );
        }
        Ok(())
    }
}
