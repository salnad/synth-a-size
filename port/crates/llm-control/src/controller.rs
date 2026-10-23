//! The tool loop's state around a session: undo history, edits waiting for
//! the user's confirmation, and the policy that decides which edits need it.

use sas_preset::display::display_number;
use sas_preset::PresetState;

use crate::catalog::control;
use crate::session::{Edit, SynthSession};
use crate::tools::{self, ToolOutput};
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
#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    pub edits: Vec<Edit>,
    /// Why it needs confirmation, for the user.
    pub reason: String,
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
}

impl<S: SynthSession> Controller<S> {
    pub fn new(session: S, policy: Policy) -> Controller<S> {
        Controller {
            session,
            policy,
            history: Vec::new(),
            needs_snapshot: true,
            pending: None,
        }
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

    /// Starts a model turn: its edits form one undo step.
    pub fn begin_turn(&mut self) {
        self.needs_snapshot = true;
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
    /// policy says so. Returns the reason when they are held.
    pub(crate) fn apply(&mut self, edits: Vec<Edit>) -> Result<Option<String>, Error> {
        if let Some(reason) = self.needs_confirmation(&edits) {
            self.pending = Some(Pending {
                edits,
                reason: reason.clone(),
            });
            return Ok(Some(reason));
        }
        self.apply_now(&edits)?;
        Ok(None)
    }

    fn apply_now(&mut self, edits: &[Edit]) -> Result<(), Error> {
        if self.needs_snapshot {
            self.push_snapshot(None);
        }
        for edit in edits {
            self.session.apply(edit)?;
        }
        Ok(())
    }

    fn needs_confirmation(&self, edits: &[Edit]) -> Option<String> {
        for edit in edits {
            let Edit::SetControl { name, value } = edit;
            let Some((_, threshold)) = self.policy.confirm_above.iter().find(|(n, _)| n == name)
            else {
                continue;
            };
            let info = control(name)?;
            let current = self.session.control(name).unwrap_or(info.default_value);
            let new_display = display_number(info, *value);
            if *value > current && new_display > *threshold {
                return Some(format!(
                    "{name} would rise to {new_display:.2}{}, above {threshold}{}",
                    info.display_units, info.display_units
                ));
            }
        }
        None
    }

    pub(crate) fn push_snapshot(&mut self, label: Option<String>) {
        self.history.push(Snapshot {
            label,
            state: self.session.snapshot(),
        });
        if self.history.len() > self.policy.max_undo {
            self.history.remove(0);
        }
        self.needs_snapshot = false;
    }

    /// Undoes `steps` steps, or back to the checkpoint `label`. Returns the
    /// steps undone.
    pub fn undo(&mut self, steps: usize, label: Option<&str>) -> Result<usize, Error> {
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
