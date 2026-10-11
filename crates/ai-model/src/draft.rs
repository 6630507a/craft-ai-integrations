//! The form behind "add a connection": editable text in, a checked [`Connection`] out.
//!
//! The launcher's connections screen and the small fallback form inside an app both edit a
//! [`ConnectionDraft`], so the rules live once and the two screens cannot disagree:
//!
//! - **Where it goes decides whether it leaves the computer.** The locality is derived from the
//!   address (loopback means local), never chosen, so a remote service cannot be mislabelled
//!   local to skip the consent prompt.
//! - **A key is checked before it is kept.** One line of visible ASCII, no spaces. A pasted
//!   newline would otherwise reach an HTTP header.
//! - **Editing keeps what was not touched.** Leaving the key box empty keeps the saved key; the
//!   models found on a server are dropped when the address changes, because they belong to the
//!   old one.
//! - **Messages are for people.** Every [`DraftError`] names the field and says what to do, and
//!   never contains the key.
//!
//! Nothing here does I/O, so it compiles for wasm.

use std::fmt;

use crate::connection::{Auth, Billing, Connection, Locality};
use crate::endpoint::Endpoint;
use crate::ids::{ConnectionId, SecretRef, validate_id};
use crate::model::ModelInfo;
use crate::secret::SecretString;

/// Longest label, in characters.
pub const MAX_LABEL_CHARS: usize = 80;
/// Longest address, in bytes.
pub const MAX_ENDPOINT_BYTES: usize = 2048;
/// Longest key, in bytes.
pub const MAX_KEY_BYTES: usize = 4096;

/// A starting point for the form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    /// Stable name, for tests and analytics-free bookkeeping.
    pub key: &'static str,
    /// What the button says.
    pub name: &'static str,
    pub provider: &'static str,
    /// Empty for "type your own".
    pub endpoint: &'static str,
    pub billing: Billing,
    pub needs_key: bool,
    /// One line about it, shown under the name.
    pub note: &'static str,
}

/// The services most people connect, in the order a form lists them.
pub const PRESETS: &[Preset] = &[
    Preset {
        key: "openai",
        name: "OpenAI",
        provider: "openai",
        endpoint: "https://api.openai.com/v1",
        billing: Billing::Metered,
        needs_key: true,
        note: "Your own OpenAI API key. Billed by OpenAI.",
    },
    Preset {
        key: "openrouter",
        name: "OpenRouter",
        provider: "openrouter",
        endpoint: "https://openrouter.ai/api/v1",
        billing: Billing::Metered,
        needs_key: true,
        note: "One key for many hosted models.",
    },
    Preset {
        key: "ollama",
        name: "Ollama",
        provider: "ollama",
        endpoint: "http://127.0.0.1:11434/v1",
        billing: Billing::Free,
        needs_key: false,
        note: "Models running on this computer. Nothing leaves it.",
    },
    Preset {
        key: "lmstudio",
        name: "LM Studio",
        provider: "lmstudio",
        endpoint: "http://127.0.0.1:1234/v1",
        billing: Billing::Free,
        needs_key: false,
        note: "Models running on this computer. Nothing leaves it.",
    },
    Preset {
        key: "llamacpp",
        name: "llama.cpp",
        provider: "llamacpp",
        endpoint: "http://127.0.0.1:8080/v1",
        billing: Billing::Free,
        needs_key: false,
        note: "A llama.cpp server on this computer.",
    },
    Preset {
        key: "custom",
        name: "Other (OpenAI compatible)",
        provider: "openai-compatible",
        endpoint: "",
        billing: Billing::Unknown,
        needs_key: true,
        note: "Any server that speaks the OpenAI API.",
    },
];

/// Which box a problem belongs to, so a form can mark it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Label,
    Provider,
    Address,
    Key,
}

/// Why a draft was refused. The message is safe to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraftError {
    pub field: Field,
    pub message: String,
}

impl DraftError {
    fn new(field: Field, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

impl fmt::Display for DraftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for DraftError {}

/// What to do with the stored key.
#[derive(Clone, Debug)]
pub enum KeyChange {
    /// Leave whatever is saved.
    Keep,
    Set(SecretString),
    /// The connection no longer uses a key; remove the saved one.
    Clear,
}

/// A checked connection and what to do with its key.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub connection: Connection,
    pub key: KeyChange,
}

/// The form's state.
#[derive(Clone)]
pub struct ConnectionDraft {
    /// `Some` when editing an existing connection.
    pub editing: Option<ConnectionId>,
    pub label: String,
    pub provider: String,
    pub endpoint: String,
    pub billing: Billing,
    pub uses_key: bool,
    /// Text typed into the key box. Empty while editing means "keep the saved key". It is a plain
    /// string only because a text box needs one; [`Debug`] never prints it and [`prepare`]
    /// (Self::prepare) turns it into a [`SecretString`].
    pub key: String,
    pub enabled: bool,
    /// Models found by a test, carried onto the saved connection.
    pub models: Vec<ModelInfo>,
    /// Editing: the saved key's locator, and whether a key was saved at all.
    secret: Option<SecretRef>,
    original_endpoint: Option<String>,
}

impl Default for ConnectionDraft {
    fn default() -> Self {
        Self {
            editing: None,
            label: String::new(),
            provider: "openai-compatible".to_owned(),
            endpoint: String::new(),
            billing: Billing::Unknown,
            uses_key: true,
            key: String::new(),
            enabled: true,
            models: Vec::new(),
            secret: None,
            original_endpoint: None,
        }
    }
}

impl fmt::Debug for ConnectionDraft {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectionDraft")
            .field("editing", &self.editing)
            .field("label", &self.label)
            .field("provider", &self.provider)
            .field("endpoint", &self.endpoint)
            .field("uses_key", &self.uses_key)
            .field("key", &format_args!("<{} chars>", self.key.chars().count()))
            .finish_non_exhaustive()
    }
}

impl ConnectionDraft {
    /// A new connection started from a preset.
    pub fn from_preset(preset: &Preset) -> Self {
        Self {
            label: preset.name.to_owned(),
            provider: preset.provider.to_owned(),
            endpoint: preset.endpoint.to_owned(),
            billing: preset.billing,
            uses_key: preset.needs_key,
            ..Self::default()
        }
    }

    /// Edits a saved connection. The key box starts empty.
    pub fn from_connection(c: &Connection) -> Self {
        Self {
            editing: Some(c.id.clone()),
            label: c.label.clone(),
            provider: c.provider.clone(),
            endpoint: c.endpoint.clone().unwrap_or_default(),
            billing: c.billing,
            uses_key: c.auth.secret_ref().is_some(),
            key: String::new(),
            enabled: c.enabled,
            models: c.models.clone(),
            secret: c.auth.secret_ref().cloned(),
            original_endpoint: c.endpoint.clone(),
        }
    }

    /// Where this would send data, as far as the address can tell. `None` while it is not a usable
    /// address, so a form can show the badge live.
    pub fn locality(&self) -> Option<Locality> {
        let endpoint = Endpoint::parse(self.endpoint.trim()).ok()?;
        Some(if endpoint.is_loopback() {
            Locality::Local
        } else {
            Locality::Remote
        })
    }

    /// Checks everything and builds the connection. `existing` is every saved connection, so new
    /// ids stay unique and an edit finds its original.
    pub fn prepare(&self, existing: &[Connection]) -> Result<Prepared, DraftError> {
        let label = self.label.trim();
        if label.is_empty() {
            return Err(DraftError::new(Field::Label, "Give the connection a name."));
        }
        if label.chars().count() > MAX_LABEL_CHARS || label.chars().any(char::is_control) {
            return Err(DraftError::new(
                Field::Label,
                format!("The name must be one line of at most {MAX_LABEL_CHARS} characters."),
            ));
        }
        let provider = self.provider.trim();
        validate_id(provider).map_err(|e| {
            DraftError::new(
                Field::Provider,
                format!("The service type is not valid ({e})."),
            )
        })?;
        let address = self.endpoint.trim();
        if address.is_empty() {
            return Err(DraftError::new(
                Field::Address,
                "Enter the address of the server.",
            ));
        }
        if address.len() > MAX_ENDPOINT_BYTES {
            return Err(DraftError::new(Field::Address, "That address is too long."));
        }
        let parsed = Endpoint::parse(address).map_err(|e| {
            DraftError::new(
                Field::Address,
                format!(
                    "That address cannot be used ({e}). Use https://, or http:// only for this computer."
                ),
            )
        })?;
        let locality = if parsed.is_loopback() {
            Locality::Local
        } else {
            Locality::Remote
        };

        let typed = self.typed_key()?;

        let id = match &self.editing {
            Some(id) => {
                if !existing.iter().any(|c| &c.id == id) {
                    return Err(DraftError::new(
                        Field::Label,
                        "This connection was removed elsewhere. Close the form and start again.",
                    ));
                }
                id.clone()
            }
            None => new_id(label, existing)?,
        };
        let (auth, key) = self.auth_and_key(typed, &id, existing)?;
        let moved = self
            .original_endpoint
            .as_deref()
            .is_some_and(|o| o != address);
        let connection = Connection {
            id,
            label: label.to_owned(),
            provider: provider.to_owned(),
            endpoint: Some(address.to_owned()),
            locality,
            auth,
            billing: self.billing,
            models: if moved {
                Vec::new()
            } else {
                self.models.clone()
            },
            enabled: self.enabled,
        };
        connection
            .validate()
            .map_err(|e| DraftError::new(Field::Address, format!("{e}.")))?;
        Ok(Prepared { connection, key })
    }

    /// The typed key, checked, or `None` if the box is empty.
    fn typed_key(&self) -> Result<Option<SecretString>, DraftError> {
        let text = self.key.trim();
        if text.is_empty() {
            return Ok(None);
        }
        check_key(text).map(|()| Some(SecretString::new(text)))
    }

    fn auth_and_key(
        &self,
        typed: Option<SecretString>,
        id: &ConnectionId,
        existing: &[Connection],
    ) -> Result<(Auth, KeyChange), DraftError> {
        if !self.uses_key {
            let change = if self.secret.is_some() {
                KeyChange::Clear
            } else {
                KeyChange::Keep
            };
            return Ok((Auth::None, change));
        }
        let secret = match &self.secret {
            Some(s) => s.clone(),
            None => new_secret_ref(id, existing)?,
        };
        match typed {
            Some(key) => Ok((Auth::ApiKey { secret }, KeyChange::Set(key))),
            None if self.secret.is_some() => Ok((Auth::ApiKey { secret }, KeyChange::Keep)),
            None => Err(DraftError::new(
                Field::Key,
                "Paste the key. If this server needs none, turn off \"Uses a key\".",
            )),
        }
    }
}

/// One line of visible ASCII with no spaces: what an HTTP bearer token can be.
pub fn check_key(text: &str) -> Result<(), DraftError> {
    if text.len() > MAX_KEY_BYTES {
        return Err(DraftError::new(
            Field::Key,
            "That key is far too long to be a key.",
        ));
    }
    if !text.bytes().all(|b| (0x21..=0x7e).contains(&b)) {
        return Err(DraftError::new(
            Field::Key,
            "A key is one line of letters, digits and symbols with no spaces. Copy it again.",
        ));
    }
    Ok(())
}

/// A lowercase, dash-separated name made from a label.
fn slug(label: &str) -> String {
    let mut out = String::new();
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
        if out.len() >= 40 {
            break;
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "connection".to_owned()
    } else {
        out.to_owned()
    }
}

/// The locator for a new connection's key: its id plus `.key`, so it is recognisable in a keychain
/// listing and two connections can never share a key. (Ids are unique, so this is too, unless an
/// older file used the same text for something else; then it gets a number.)
fn new_secret_ref(id: &ConnectionId, existing: &[Connection]) -> Result<SecretRef, DraftError> {
    let used = |s: &str| {
        existing
            .iter()
            .any(|c| c.auth.secret_ref().is_some_and(|r| r.as_str() == s))
    };
    let mut candidate = format!("{id}.key");
    let mut n = 2u32;
    while used(&candidate) {
        candidate = format!("{id}.key{n}");
        n = n.saturating_add(1);
    }
    SecretRef::new(candidate)
        .map_err(|_| DraftError::new(Field::Label, "That name cannot be used for the saved key."))
}

/// An id derived from the label that no saved connection uses.
fn new_id(label: &str, existing: &[Connection]) -> Result<ConnectionId, DraftError> {
    let base = slug(label);
    let taken = |s: &str| existing.iter().any(|c| c.id.as_str() == s);
    let mut candidate = base.clone();
    let mut n = 2u32;
    while taken(&candidate) {
        candidate = format!("{base}-{n}");
        n = n.saturating_add(1);
    }
    ConnectionId::new(candidate)
        .map_err(|_| DraftError::new(Field::Label, "That name cannot be turned into an id."))
}

#[cfg(test)]
mod tests;
