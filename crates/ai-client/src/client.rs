//! [`AiClient`]: what an app calls.
//!
//! One flow for every Craft app, matching the adapter checklist:
//!
//! 1. [`status`](AiClient::status): is this feature usable? Gives a plain reason when it is not
//!    (no model chosen, connection switched off, key missing), so the app can grey the feature out
//!    and point at the launcher. Every app works with no connection at all.
//! 2. [`plan`](AiClient::plan): resolve the role, check the request against the chosen model, and
//!    build the [`Disclosure`] of what would leave the machine. Nothing is sent and no key is read.
//! 3. The app shows the disclosure if [`Plan::needs_consent`], and records agreement in its
//!    [`ConsentLedger`].
//! 4. [`start`](AiClient::start): re-checks consent (so it cannot be skipped), reads the key, builds
//!    the provider and submits. The returned [`Job`] is polled and cancelled by the app's own job
//!    system.
//!
//! The key exists only inside `start`, from the moment it is read until the adapter is built. It is
//! never in a [`Plan`], a [`Job`], a status or an error.

use std::sync::Arc;
use std::time::{Duration, Instant};

use craft_ai_model::job::Event;
use craft_ai_model::payload::Limits;
use craft_ai_model::role::eligible;
use craft_ai_model::{
    AiError, AppId, Billing, Binding, Connection, ConnectionId, ConsentLedger, Disclosure,
    ErrorKind, JobId, JobSnapshot, JobState, Locality, ModelId, Provider, Request, Role, RoleId,
    Usage,
};
use craft_ai_native::secrets::{EnvStore, KeychainStore, LayeredStore, SecretStore, secret_for};
use craft_ai_native::shared::SharedStore;

use crate::factory::ProviderFactory;

/// Whether a role can run right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleStatus {
    Ready(RoleReady),
    /// Not usable, with a sentence the app can show.
    Unavailable {
        reason: String,
    },
}

/// What a ready role is bound to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleReady {
    pub connection: ConnectionId,
    pub connection_label: String,
    pub model: ModelId,
    pub model_label: String,
    pub locality: Locality,
    pub billing: Billing,
}

/// One entry in a model picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub connection: ConnectionId,
    pub connection_label: String,
    pub model: ModelId,
    pub model_label: String,
    pub locality: Locality,
}

/// A checked request waiting for consent. Holds no secret.
#[derive(Clone, Debug)]
pub struct Plan {
    connection: Connection,
    model: ModelId,
    request: Request,
    disclosure: Disclosure,
}

impl Plan {
    /// What would be sent where. Show this to the user.
    pub fn disclosure(&self) -> &Disclosure {
        &self.disclosure
    }

    pub fn needs_consent(&self, ledger: &ConsentLedger) -> bool {
        ledger.needs_consent(&self.disclosure)
    }

    pub fn connection_label(&self) -> &str {
        &self.connection.label
    }

    pub fn model(&self) -> &ModelId {
        &self.model
    }
}

/// A started job.
#[derive(Clone)]
pub struct Job {
    provider: Arc<dyn Provider>,
    id: JobId,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Job({})", self.id)
    }
}

/// Everything a finished job produced.
#[derive(Debug)]
pub struct Finished {
    pub state: JobState,
    pub events: Vec<Event>,
    pub usage: Option<Usage>,
}

impl Job {
    pub fn id(&self) -> &JobId {
        &self.id
    }

    pub fn poll(&self) -> Result<JobSnapshot, AiError> {
        self.provider.poll(&self.id)
    }

    pub fn cancel(&self) -> Result<(), AiError> {
        self.provider.cancel(&self.id)
    }

    /// Polls every `interval` until the job ends or `timeout` passes, collecting events. For
    /// scripts and tests; an app with its own job system polls instead.
    pub fn wait(&self, interval: Duration, timeout: Duration) -> Result<Finished, AiError> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        loop {
            let snap = self.poll()?;
            events.extend(snap.events);
            if snap.state.is_terminal() {
                return Ok(Finished {
                    state: snap.state,
                    events,
                    usage: snap.usage,
                });
            }
            if Instant::now() >= deadline {
                let _ = self.cancel();
                return Err(AiError::new(ErrorKind::Network, "the job took too long"));
            }
            std::thread::sleep(interval);
        }
    }
}

/// The app's connection to the AI layer.
pub struct AiClient {
    app: AppId,
    shared: SharedStore,
    secrets: Arc<dyn SecretStore>,
    factory: ProviderFactory,
}

impl AiClient {
    pub fn new(app: AppId, shared: SharedStore, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            app,
            shared,
            secrets,
            factory: ProviderFactory::standard(),
        }
    }

    /// The usual setup: the standard shared store location, and keys from the environment then the
    /// OS keychain. (The passphrase-encrypted file is not included, because only the caller can
    /// supply its passphrase; build a [`LayeredStore`] and use [`new`](Self::new) to add it.)
    pub fn standard(app: AppId) -> Result<Self, AiError> {
        let shared = SharedStore::at_default_location()?;
        let secrets = LayeredStore::new(vec![
            Box::new(EnvStore::new()),
            Box::new(KeychainStore::new()),
        ]);
        Ok(Self::new(app, shared, Arc::new(secrets)))
    }

    /// Replaces the provider factory, to add adapters.
    pub fn with_factory(mut self, factory: ProviderFactory) -> Self {
        self.factory = factory;
        self
    }

    pub fn app(&self) -> &AppId {
        &self.app
    }

    /// Where this app's consent ledger is kept by default.
    pub fn consent_file(&self) -> Option<craft_ai_native::ConsentFile> {
        craft_ai_native::ConsentFile::at_default_location(&self.app)
    }

    /// Can `role` run now? The reason, when it cannot, is safe to show.
    pub fn status(&self, role: &Role) -> RoleStatus {
        let unavailable = |reason: String| RoleStatus::Unavailable { reason };
        let store = match self.shared.load() {
            Ok(s) => s,
            Err(e) => return unavailable(e.to_string()),
        };
        let resolved = match store.resolve(&self.app, role) {
            Ok(r) => r,
            Err(e) => return unavailable(e.to_string()),
        };
        if let Err(e) = secret_for(&*self.secrets, resolved.connection) {
            return unavailable(e.message);
        }
        RoleStatus::Ready(RoleReady {
            connection: resolved.connection.id.clone(),
            connection_label: resolved.connection.label.clone(),
            model: resolved.model.id.clone(),
            model_label: resolved.model.label.clone(),
            locality: resolved.connection.locality,
            billing: resolved.connection.billing,
        })
    }

    /// Every model that could fill `role`, for a picker.
    pub fn choices(&self, role: &Role) -> Result<Vec<Choice>, AiError> {
        let store = self.shared.load()?;
        Ok(eligible(role, &store.connections)
            .into_iter()
            .map(|r| Choice {
                connection: r.connection.id.clone(),
                connection_label: r.connection.label.clone(),
                model: r.model.id.clone(),
                model_label: r.model.label.clone(),
                locality: r.connection.locality,
            })
            .collect())
    }

    /// Sets this app's own choice for `role`, overriding the launcher default. Refused, and nothing
    /// written, if the connection or model cannot fill the role.
    pub fn bind(
        &self,
        role: &Role,
        connection: &ConnectionId,
        model: &ModelId,
    ) -> Result<(), AiError> {
        let binding = Binding {
            role: role.id.clone(),
            connection: connection.clone(),
            model: model.clone(),
        };
        let mut candidate = self.shared.load()?;
        set_override(&mut candidate, &self.app, binding.clone());
        candidate
            .resolve(&self.app, role)
            .map_err(|e| AiError::invalid(e.to_string()))?;
        self.shared
            .update(|store| set_override(store, &self.app, binding))?;
        Ok(())
    }

    /// Removes this app's own choice, so the launcher default applies again.
    pub fn clear_binding(&self, role: &RoleId) -> Result<(), AiError> {
        self.shared.update(|store| {
            if let Some(settings) = store.apps.get_mut(&self.app) {
                settings.overrides.retain(|b| &b.role != role);
            }
        })?;
        Ok(())
    }

    /// Resolves the role and checks the request. Sends nothing and reads no key.
    pub fn plan(&self, role: &Role, request: Request) -> Result<Plan, AiError> {
        if request.capability != role.capability {
            return Err(AiError::invalid(format!(
                "this request is for {}, but \"{}\" needs {}",
                request.capability, role.label, role.capability
            )));
        }
        let store = self.shared.load()?;
        let resolved = store
            .resolve(&self.app, role)
            .map_err(|e| AiError::unsupported(e.to_string()))?;
        request.validate(resolved.spec, &Limits::default())?;
        let disclosure = Disclosure::for_request(&self.app, resolved.connection, &request);
        Ok(Plan {
            connection: resolved.connection.clone(),
            model: resolved.model.id.clone(),
            request,
            disclosure,
        })
    }

    /// Starts the job. Refuses with [`ErrorKind::ConsentRequired`] if `ledger` does not cover the
    /// plan's disclosure.
    pub fn start(&self, plan: &Plan, ledger: &ConsentLedger) -> Result<Job, AiError> {
        if plan.needs_consent(ledger) {
            return Err(AiError::new(
                ErrorKind::ConsentRequired,
                format!(
                    "sending to {} needs your agreement first",
                    plan.disclosure.destination
                ),
            ));
        }
        let secret = secret_for(&*self.secrets, &plan.connection)?;
        let provider = self.factory.build(&plan.connection, secret)?;
        let id = provider.submit(&plan.model, &plan.request)?;
        Ok(Job { provider, id })
    }
}

impl std::fmt::Debug for AiClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AiClient({})", self.app)
    }
}

fn set_override(store: &mut craft_ai_model::StoreFile, app: &AppId, binding: Binding) {
    let settings = store.apps.entry(app.clone()).or_default();
    settings.overrides.retain(|b| b.role != binding.role);
    settings.overrides.push(binding);
}
