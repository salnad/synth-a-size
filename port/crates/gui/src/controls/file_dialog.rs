// SPDX-License-Identifier: GPL-3.0-or-later

//! Native open and save dialogs for the editor, in place of JUCE's
//! `FileChooser::browseForFileToOpen` / `browseForFileToSave`.
//!
//! Upstream's choosers are modal: the plugin's message loop waits inside
//! them. The editor must keep drawing, so a dialog here is a future that
//! each frame polls without waiting: [`open`] starts one under an id, and
//! [`take_result`] returns its answer once the user has chosen. On macOS
//! `rfd` shows it as a sheet on the plugin's window (AppKit, on the main
//! thread the editor already runs on); elsewhere as the platform's dialog,
//! parented to the plugin window where the platform allows it.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use nice_plug_egui::baseview::PlatformHandle;

/// How often the editor redraws to poll a dialog while one is open.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Open a file, save one, or choose a folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    Open,
    Save,
    Folder,
}

/// What a dialog asks for.
#[derive(Clone, Debug, PartialEq)]
pub struct FileRequest {
    pub kind: DialogKind,
    pub title: String,
    /// Named groups of extensions (without dots), offered in order.
    pub filters: Vec<(String, Vec<String>)>,
    /// The folder it starts in.
    pub folder: Option<PathBuf>,
    /// The name a save dialog suggests.
    pub file_name: Option<String>,
}

impl FileRequest {
    /// An open dialog for `filters`.
    pub fn open(title: &str, filters: Vec<(String, Vec<String>)>) -> FileRequest {
        FileRequest {
            kind: DialogKind::Open,
            title: title.to_string(),
            filters,
            folder: None,
            file_name: None,
        }
    }

    /// A save dialog for `filters`, suggesting `file_name`.
    pub fn save(title: &str, filters: Vec<(String, Vec<String>)>, file_name: &str) -> FileRequest {
        FileRequest {
            kind: DialogKind::Save,
            file_name: Some(file_name.to_string()),
            ..FileRequest::open(title, filters)
        }
    }

    /// A dialog choosing a folder (`FileChooser::browseForDirectory`).
    pub fn folder(title: &str) -> FileRequest {
        FileRequest {
            kind: DialogKind::Folder,
            ..FileRequest::open(title, Vec::new())
        }
    }

    pub fn in_folder(mut self, folder: PathBuf) -> FileRequest {
        self.folder = Some(folder);
        self
    }
}

/// JUCE's wildcard lists (`"*.scl;*.tun;*.kbm"`, as
/// `Tuning::allFileExtensions` gives) as extensions without dots.
pub fn extensions(wildcards: &str) -> Vec<String> {
    wildcards
        .split(';')
        .filter_map(|pattern| pattern.trim().strip_prefix("*."))
        .filter(|extension| !extension.is_empty())
        .map(str::to_string)
        .collect()
}

type Answer = Option<PathBuf>;
type Pending = Pin<Box<dyn Future<Output = Answer> + Send>>;

/// The editor's open dialogs and its window, kept in the egui context.
#[derive(Default)]
struct Dialogs {
    /// The editor's window (a weak handle: unavailable once it closes).
    parent: Option<PlatformHandle>,
    pending: Vec<(egui::Id, Pending)>,
    answered: Vec<(egui::Id, Answer)>,
}

#[derive(Clone, Default)]
struct Shared(Arc<Mutex<Dialogs>>);

fn with_dialogs<R>(ctx: &egui::Context, f: impl FnOnce(&mut Dialogs) -> R) -> R {
    let shared = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Shared>(egui::Id::unique("file dialogs"))
            .clone()
    });
    let mut dialogs = shared
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    f(&mut dialogs)
}

/// The window dialogs attach to: the editor's own.
pub fn set_parent(ctx: &egui::Context, parent: Option<PlatformHandle>) {
    with_dialogs(ctx, |dialogs| dialogs.parent = parent);
}

/// Whether a dialog started under `id` is still open.
pub fn is_open(ctx: &egui::Context, id: egui::Id) -> bool {
    with_dialogs(ctx, |dialogs| {
        dialogs.pending.iter().any(|(open, _)| *open == id)
    })
}

/// Opens the dialog `request` describes under `id`, unless one is already
/// open there.
pub fn open(ctx: &egui::Context, id: egui::Id, request: &FileRequest) {
    if is_open(ctx, id) {
        return;
    }
    let parent = with_dialogs(ctx, |dialogs| dialogs.parent.clone());
    let mut dialog = rfd::AsyncFileDialog::new().set_title(&request.title);
    for (name, extensions) in &request.filters {
        dialog = dialog.add_filter(name, extensions);
    }
    if let Some(folder) = &request.folder {
        dialog = dialog.set_directory(folder);
    }
    if let Some(file_name) = &request.file_name {
        dialog = dialog.set_file_name(file_name);
    }
    if let Some(parent) = &parent {
        dialog = dialog.set_parent(parent);
    }
    let future: Pending = match request.kind {
        DialogKind::Open => Box::pin(async move {
            dialog
                .pick_file()
                .await
                .map(|file| file.path().to_path_buf())
        }),
        DialogKind::Folder => Box::pin(async move {
            dialog
                .pick_folder()
                .await
                .map(|folder| folder.path().to_path_buf())
        }),
        DialogKind::Save => Box::pin(async move {
            dialog
                .save_file()
                .await
                .map(|file| file.path().to_path_buf())
        }),
    };
    start(ctx, id, future);
}

/// Waits on `future` as the dialog under `id`.
fn start(ctx: &egui::Context, id: egui::Id, future: Pending) {
    with_dialogs(ctx, |dialogs| dialogs.pending.push((id, future)));
    ctx.request_repaint();
}

/// The answer from the dialog under `id`, once: `Some(Some(path))` when the
/// user chose a file, `Some(None)` when they cancelled, `None` while it is
/// open or when there is none. Polls every open dialog without waiting.
pub fn take_result(ctx: &egui::Context, id: egui::Id) -> Option<Answer> {
    with_dialogs(ctx, |dialogs| {
        let mut poll = Context::from_waker(Waker::noop());
        let mut index = 0;
        while index < dialogs.pending.len() {
            if let Poll::Ready(answer) = dialogs.pending[index].1.as_mut().poll(&mut poll) {
                let (done, _) = dialogs.pending.remove(index);
                dialogs.answered.push((done, answer));
            } else {
                index += 1;
            }
        }
        if !dialogs.pending.is_empty() {
            ctx.request_repaint_after(POLL_INTERVAL);
        }
        let position = dialogs.answered.iter().position(|(done, _)| *done == id)?;
        Some(dialogs.answered.remove(position).1)
    })
}

/// `File::revealToUser`: shows `path` in the file manager (selected, on
/// macOS; its folder opened, elsewhere).
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(not(target_os = "macos"))]
    let result = {
        let folder = if path.is_dir() {
            path
        } else {
            path.parent().unwrap_or(path)
        };
        std::process::Command::new("xdg-open").arg(folder).spawn()
    };
    let _ = result;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_become_extensions() {
        assert_eq!(extensions("*.scl;*.tun;*.kbm"), ["scl", "tun", "kbm"]);
        assert_eq!(extensions("*.wav;*.flac"), ["wav", "flac"]);
        assert!(extensions("").is_empty());
    }

    /// A dialog's answer arrives once, after it resolves, and only under its
    /// own id; polling never waits for it.
    #[test]
    fn answers_arrive_once_under_their_id() {
        let ctx = egui::Context::default();
        let id = egui::Id::unique("tuning");
        let (sender, receiver) = std::sync::mpsc::channel::<Answer>();
        let receiver = Mutex::new(receiver);
        start(
            &ctx,
            id,
            Box::pin(std::future::poll_fn(move |_| {
                match receiver.lock().unwrap().try_recv() {
                    Ok(answer) => Poll::Ready(answer),
                    Err(_) => Poll::Pending,
                }
            })),
        );
        assert!(is_open(&ctx, id));
        assert_eq!(take_result(&ctx, id), None);
        sender.send(Some(PathBuf::from("/tmp/a.scl"))).unwrap();
        assert_eq!(take_result(&ctx, egui::Id::unique("other")), None);
        assert!(!is_open(&ctx, id));
        assert_eq!(
            take_result(&ctx, id),
            Some(Some(PathBuf::from("/tmp/a.scl")))
        );
        assert_eq!(take_result(&ctx, id), None);
    }
}
