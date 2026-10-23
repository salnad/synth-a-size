//! The tool loop's state around a session: undo history, edits waiting for
//! the user's confirmation, and the policy that decides which edits need it.

use sas_preset::display::display_number;
use sas_preset::PresetState;

use crate::catalog::control;
use crate::library::PresetLibrary;
use crate::listen::{audition_spec, Auditioner};
use crate::session::{Applied, Edit, SynthSession};
use crate::tools::{self, ToolOutput};
use crate::tools::{specs, ToolSpec};
use crate::Error;

/// Tunables of the tool layer. Every field is a working default the owner
/// may change (`docs/project-context.md`, "working defaults").
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    /// Edits that raise one of these controls above its threshold (in
    /// display units) wait for the user to confirm them.
    pub confirm_above: Vec<(&'static str, f32)>,
    /// Undo snapshots kept; the oldest is dropped beyond this.
    pub max_undo: usize,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            // Master volume above unity gain (its range is -80..+6 dB).
            confirm_above: vec![("volume", 0.0)],
            max_undo: 50,
        }
    }
}

/// A batch of edits held until the user confirms or rejects it.
#[derive(Clone, Debug)]
pub struct Pending {
    pub edits: Vec<Edit>,
    /// Why it needs confirmation, for the user.
    pub reason: String,
}

/// What [`Controller::apply`] did with a batch.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Outcome {
    /// Applied, with one result per edit.
    Applied(Vec<Applied>),
    /// Held for the user's confirmation, for this reason.
    Pending(String),
}

#[derive(Clone, Debug)]
struct Snapshot {
    label: Option<String>,
    state: PresetState,
}

/// Runs tool calls against a session.
pub struct Controller<S: SynthSession> {
    session: S,
    policy: Policy,
    history: Vec<Snapshot>,
    /// Whether the next edit starts a new undo step.
    needs_snapshot: bool,
    pending: Option<Pending>,
    library: Option<PresetLibrary>,
    compare: Option<Compare>,
    auditioner: Option<Box<dyn Auditioner>>,
}

/// Which version an A/B comparison is playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The patch before the last undo step (before the model's last turn).
    Before,
    /// The patch with that step.
    After,
}

#[derive(Clone, Debug)]
struct Compare {
    before: PresetState,
    after: PresetState,
    playing: Side,
}

impl<S: SynthSession> Controller<S> {
    pub fn new(session: S, policy: Policy) -> Controller<S> {
        Controller {
            session,
            policy,
            history: Vec::new(),
            needs_snapshot: true,
            pending: None,
            library: None,
            compare: None,
            auditioner: None,
        }
    }

    /// Gives the preset tools a folder to read and write.
    pub fn with_library(mut self, library: PresetLibrary) -> Controller<S> {
        self.library = Some(library);
        self
    }

    pub fn library(&self) -> Option<&PresetLibrary> {
        self.library.as_ref()
    }

    /// Lets the model listen to the patch (the `audition` tool).
    pub fn with_auditioner(mut self, auditioner: Box<dyn Auditioner>) -> Controller<S> {
        self.auditioner = Some(auditioner);
        self
    }

    pub(crate) fn auditioner(&mut self) -> Option<&mut (dyn Auditioner + 'static)> {
        self.auditioner.as_deref_mut()
    }

    /// The tools this controller offers: [`specs`], and `audition` when it
    /// has an auditioner.
    pub fn specs(&self) -> Vec<ToolSpec> {
        let mut specs = specs();
        if self.auditioner.is_some() {
            specs.push(audition_spec());
        }
        specs
    }

    pub fn session(&self) -> &S {
        &self.session
    }

    pub fn into_session(self) -> S {
        self.session
    }

    pub fn policy(&self) -> &Policy {
        &self.policy
    }

    /// Starts a model turn: its edits form one undo step. An A/B comparison
    /// in progress keeps the version playing. If the state was replaced
    /// outside the chat, the undo history and any held edits are dropped.
    pub fn begin_turn(&mut self) {
        self.needs_snapshot = true;
        self.compare = None;
        self.check_replaced();
    }

    /// Drops the undo history, held edits and comparison if the session's
    /// state was replaced outside the chat. Returns whether it was.
    pub fn check_replaced(&mut self) -> bool {
        if !self.session.take_replaced() {
            return false;
        }
        self.history.clear();
        self.pending = None;
        self.compare = None;
        self.needs_snapshot = true;
        true
    }

    /// Mutable access to the session, for checks such as
    /// [`SynthSession::take_outside_changes`].
    pub fn session_mut(&mut self) -> &mut S {
        &mut self.session
    }

    /// Switches between the patch before and after the last undo step (A/B
    /// compare). The first call plays the version before; each later call
    /// swaps. Returns the side now playing, or `None` with nothing to
    /// compare. Any edit or new turn ends the comparison on the side
    /// playing.
    pub fn compare_toggle(&mut self) -> Result<Option<Side>, Error> {
        if self.check_replaced() {
            return Ok(None);
        }
        let compare = match self.compare.take() {
            Some(mut compare) => {
                compare.playing = match compare.playing {
                    Side::Before => Side::After,
                    Side::After => Side::Before,
                };
                compare
            }
            None => {
                let Some(before) = self.history.last() else {
                    return Ok(None);
                };
                Compare {
                    before: before.state.clone(),
                    after: self.session.snapshot()?,
                    playing: Side::Before,
                }
            }
        };
        let target = match compare.playing {
            Side::Before => &compare.before,
            Side::After => &compare.after,
        };
        self.session.restore(target)?;
        let playing = compare.playing;
        self.compare = Some(compare);
        Ok(Some(playing))
    }

    /// The side an A/B comparison is playing, if one is in progress.
    pub fn comparing(&self) -> Option<Side> {
        self.compare.as_ref().map(|compare| compare.playing)
    }

    /// Runs tool `name` with JSON arguments `args`.
    pub fn call(&mut self, name: &str, args: &serde_json::Value) -> ToolOutput {
        tools::call(self, name, args)
    }

    /// Undo steps available.
    pub fn undo_depth(&self) -> usize {
        self.history.len()
    }

    /// The batch waiting for the user, if any.
    pub fn pending(&self) -> Option<&Pending> {
        self.pending.as_ref()
    }

    /// The user confirmed the pending batch: applies it.
    pub fn confirm_pending(&mut self) -> Result<bool, Error> {
        match self.pending.take() {
            Some(pending) => {
                self.apply_now(&pending.edits)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// The user rejected the pending batch.
    pub fn reject_pending(&mut self) -> bool {
        self.pending.take().is_some()
    }

    /// Applies `edits` as one batch, or holds them for confirmation if the
    /// policy says so.
    pub(crate) fn apply(&mut self, edits: Vec<Edit>) -> Result<Outcome, Error> {
        if let Some(reason) = self.needs_confirmation(&edits) {
            self.pending = Some(Pending {
                edits,
                reason: reason.clone(),
            });
            return Ok(Outcome::Pending(reason));
        }
        self.apply_now(&edits).map(Outcome::Applied)
    }

    /// Applies `edits` in order. If one fails, the batch's earlier edits are
    /// undone, so a batch applies whole or not at all.
    fn apply_now(&mut self, edits: &[Edit]) -> Result<Vec<Applied>, Error> {
        self.compare = None;
        if self.needs_snapshot {
            self.push_snapshot(None)?;
        }
        let before = match edits.len() > 1 {
            true => Some(self.session.snapshot()?),
            false => None,
        };
        let mut applied = Vec::with_capacity(edits.len());
        for edit in edits {
            match self.session.apply(edit) {
                Ok(result) => applied.push(result),
                Err(error) => {
                    if let Some(state) = &before {
                        self.session.restore(state)?;
                    }
                    return Err(error);
                }
            }
        }
        Ok(applied)
    }

    /// Why `edits` need the user's confirmation, if they do: one raises a
    /// guarded control past its threshold, directly or by loading a preset.
    fn needs_confirmation(&self, edits: &[Edit]) -> Option<String> {
        for edit in edits {
            let changes: Vec<(&str, f32)> = match edit {
                Edit::SetControl { name, value } => vec![(name, *value)],
                Edit::LoadPreset(state) => self
                    .policy
                    .confirm_above
                    .iter()
                    .filter_map(|(name, _)| Some((*name, state.value(name)?)))
                    .collect(),
                _ => Vec::new(),
            };
            for (name, value) in changes {
                let Some((_, threshold)) =
                    self.policy.confirm_above.iter().find(|(n, _)| *n == name)
                else {
                    continue;
                };
                let info = control(name)?;
                let current = self.session.control(name).unwrap_or(info.default_value);
                let new_display = display_number(info, value);
                if value > current && new_display > *threshold {
                    return Some(format!(
                        "{name} would rise to {new_display:.2}{}, above {threshold}{}",
                        info.display_units, info.display_units
                    ));
                }
            }
        }
        None
    }

    pub(crate) fn push_snapshot(&mut self, label: Option<String>) -> Result<(), Error> {
        self.history.push(Snapshot {
            label,
            state: self.session.snapshot()?,
        });
        if self.history.len() > self.policy.max_undo {
            self.history.remove(0);
        }
        self.needs_snapshot = false;
        Ok(())
    }

    /// Undoes `steps` steps, or back to the checkpoint `label`. Returns the
    /// steps undone.
    pub fn undo(&mut self, steps: usize, label: Option<&str>) -> Result<usize, Error> {
        if self.check_replaced() {
            return Ok(0);
        }
        if let Some(compare) = self.compare.take() {
            // Undo from the version being compared: back to "after" first,
            // so the undo history matches the state.
            if compare.playing == Side::Before {
                self.session.restore(&compare.after)?;
            }
        }
        let count = match label {
            Some(label) => {
                let position = self
                    .history
                    .iter()
                    .rposition(|snapshot| snapshot.label.as_deref() == Some(label))
                    .ok_or_else(|| Error::Invalid(format!("no checkpoint named {label:?}")))?;
                self.history.len() - position
            }
            None => steps.min(self.history.len()),
        };
        if count == 0 {
            return Ok(0);
        }
        let target = self.history.len() - count;
        let snapshot = self.history[target].clone();
        self.session.restore(&snapshot.state)?;
        self.history.truncate(target);
        self.needs_snapshot = true;
        Ok(count)
    }
}
