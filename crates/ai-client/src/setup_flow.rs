//! Reusable setup state for any host UI. Approval and worker answers belong to one exact plan.
use crate::{CheckedNative, NativeSetupPlan};
use craft_ai_core::{AiError, ErrorKind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// What a setup request does on its worker: hash files already on disk, or download then hash.
pub enum SetupTask {
    CheckExisting,
    Download,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Names one request, so an answer from an older or cancelled worker can be told apart from the
/// current one.
pub struct SetupTicket(u64);

#[derive(Clone, Debug)]
/// One job for a worker thread: an exact plan, what to do with it, whether the person approved
/// the licences, and a flag that cancels it. It holds no token.
pub struct SetupRequest {
    ticket: SetupTicket,
    plan: NativeSetupPlan,
    task: SetupTask,
    approved: bool,
    cancelled: Arc<AtomicBool>,
}
impl SetupRequest {
    /// The ticket the worker's answer must carry back.
    pub fn ticket(&self) -> SetupTicket {
        self.ticket
    }
    /// The exact plan this request was made from.
    pub fn plan(&self) -> &NativeSetupPlan {
        &self.plan
    }
    /// Whether this request checks existing files or downloads.
    pub fn task(&self) -> SetupTask {
        self.task
    }
    /// Also available to a host shutting down a worker without a mounted setup window.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
    /// Whether the person approved the licences when the request started.
    pub fn approved(&self) -> bool {
        self.approved
    }
    /// Carry this out on a worker. Tokens are borrowed for this call and never stored in a request.
    pub fn run(
        &self,
        downloader: &craft_ai_comfy_env::Downloader,
        token: Option<&craft_ai_core::SecretString>,
        progress: &mut dyn FnMut(usize, craft_ai_comfy_env::Progress),
    ) -> Result<CheckedNative, AiError> {
        let cancel = self.cancelled.as_ref();
        match self.task {
            SetupTask::CheckExisting => self.plan.check_existing(cancel, progress),
            SetupTask::Download => self.plan.download(crate::NativeInstall {
                downloader,
                token,
                licences_accepted: self.approved,
                cancel,
                progress,
            }),
        }
    }
}

/// Setup state for one reviewed plan, kept by the host: the approval, at most one running request,
/// the last checked result and any failure. All disk and network work happens on a worker, never
/// here.
pub struct SetupSession {
    plan: NativeSetupPlan,
    accepted: bool,
    next: u64,
    active: Option<SetupRequest>,
    checked: Option<CheckedNative>,
    problem: Option<ErrorKind>,
    progress: Option<(usize, crate::SetupProgress)>,
}
impl SetupSession {
    /// A session for `plan` with nothing approved and nothing running.
    pub fn new(plan: NativeSetupPlan) -> Self {
        Self {
            plan,
            accepted: false,
            next: 0,
            active: None,
            checked: None,
            problem: None,
            progress: None,
        }
    }
    /// The plan under review.
    pub fn plan(&self) -> &NativeSetupPlan {
        &self.plan
    }
    /// Whether the person has ticked the licences for the next download.
    pub fn accepted(&self) -> bool {
        self.accepted
    }
    /// Records the person's licence approval for the next download.
    pub fn accept_licences(&mut self, accepted: bool) {
        self.accepted = accepted;
    }
    /// Whether a request is running.
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    /// The files that passed every check, if a request finished cleanly. This is not a binding.
    pub fn checked(&self) -> Option<&CheckedNative> {
        self.checked.as_ref()
    }
    /// The latest progress of the running request: the file index and its state.
    pub fn progress(&self) -> Option<(usize, crate::SetupProgress)> {
        self.progress
    }
    /// Records progress from the worker. Refused, returning false, when the ticket is not the running
    /// request's or the file index is not a file of the plan.
    pub fn report_progress(
        &mut self,
        ticket: SetupTicket,
        file: usize,
        progress: crate::SetupProgress,
    ) -> bool {
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.ticket != ticket)
            || file >= self.plan.route().models.len()
        {
            return false;
        }
        self.progress = Some((file, progress));
        true
    }
    /// The kind of the last failure, never its raw text.
    pub fn problem(&self) -> Option<ErrorKind> {
        self.problem
    }
    /// Shows `plan`. A plan for the same target (see [`NativeSetupPlan::same_target`]) only
    /// refreshes what is shown: the approval, the running request and the checked result stay, so
    /// a download's own progress cannot cancel it. A different target discards them and returns
    /// the old ticket so the host can cancel its worker. Stale answers cannot update this session.
    pub fn review(&mut self, plan: NativeSetupPlan) -> Option<SetupTicket> {
        if self.plan.same_target(&plan) {
            self.plan = plan;
            // A checked result belongs to the directory it was checked in. Keep it only while
            // that is still where the plan's files are (or would go).
            if self
                .checked
                .as_ref()
                .is_some_and(|checked| !self.is_current(checked.weights().parent()))
            {
                self.checked = None;
            }
            return None;
        }
        let previous = self.cancel();
        self.plan = plan;
        self.accepted = false;
        self.checked = None;
        self.problem = None;
        self.progress = None;
        previous
    }
    /// Whether `directory` is one the plan stands for: where it found complete files, or where a
    /// download would go. A checked result is kept only for such a directory.
    fn is_current(&self, directory: Option<&std::path::Path>) -> bool {
        directory.is_some()
            && (directory == self.plan.existing_directory() || directory == self.plan.destination())
    }
    /// Consumes approval and returns a worker request, never performs I/O on the drawing thread.
    pub fn begin(&mut self, task: SetupTask) -> Option<SetupRequest> {
        if self.busy() || !self.plan.available() {
            return None;
        }
        match task {
            SetupTask::Download if !self.accepted || self.plan.destination().is_none() => {
                return None;
            }
            SetupTask::CheckExisting if self.plan.existing_directory().is_none() => return None,
            _ => {}
        }
        self.next = self.next.checked_add(1)?;
        let request = SetupRequest {
            ticket: SetupTicket(self.next),
            plan: self.plan.clone(),
            task,
            approved: self.accepted,
            cancelled: Arc::new(AtomicBool::new(false)),
        };
        self.accepted = false;
        self.checked = None;
        self.problem = None;
        self.progress = None;
        self.active = Some(request.clone());
        Some(request)
    }
    /// Accept a worker answer without trusting its raw error text.
    pub fn complete(
        &mut self,
        ticket: SetupTicket,
        result: Result<CheckedNative, AiError>,
    ) -> bool {
        if self
            .active
            .as_ref()
            .is_none_or(|active| active.ticket != ticket)
        {
            return false;
        }
        let Some(active) = self.active.take() else {
            return false;
        };
        self.progress = None;
        let directory = match active.task {
            SetupTask::CheckExisting => active.plan.existing_directory(),
            SetupTask::Download => active.plan.destination(),
        };
        let expected = directory
            .zip(active.plan.route().weights())
            .map(|(dir, file)| dir.join(&file.filename));
        match result {
            Ok(checked)
                if checked.feature() == active.plan.feature()
                    && checked.provider() == active.plan.route().provider
                    && expected.as_deref() == Some(checked.weights()) =>
            {
                // A late answer for a directory the review has moved away from is dropped
                // quietly: it is not an error, and the old directory is not offered.
                let current = self.is_current(checked.weights().parent());
                self.checked = current.then_some(checked);
            }
            Ok(_) => {
                self.checked = None;
                self.problem = Some(ErrorKind::InvalidRequest);
            }
            Err(error) => {
                self.checked = None;
                self.problem = Some(error.kind);
            }
        }
        true
    }
    /// Cancels the running request, if any, and returns its ticket. The worker sees the cancel flag;
    /// its late answer is refused by `complete`.
    pub fn cancel(&mut self) -> Option<SetupTicket> {
        self.progress = None;
        self.active.take().map(|request| {
            request.cancel();
            request.ticket
        })
    }
}
