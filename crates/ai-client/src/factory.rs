//! Turning a saved [`Connection`] into a working [`Provider`].
//!
//! The launcher stores a connection with a `provider` name (`openai`, `ollama`, ...). The factory
//! maps that name to code. The built-in names all use the OpenAI-compatible adapter; an app or a
//! test can [`register`](ProviderFactory::register) more, which is how new adapters (fal,
//! ComfyUI, the ArtCraft API) plug in without the client changing.
//!
//! The key arrives here already fetched and is handed straight to the adapter. It is not stored in
//! the factory.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use craft_ai_model::{AiError, Connection, Provider, SecretString};
use craft_ai_openai::{OpenAiConfig, OpenAiProvider};

/// Builds a provider for one connection.
pub type BuildFn =
    dyn Fn(&Connection, Option<SecretString>) -> Result<Arc<dyn Provider>, AiError> + Send + Sync;

/// Provider names that speak the OpenAI REST shape.
const OPENAI_FAMILY: &[&str] = &[
    "openai",
    "openai-compatible",
    "ollama",
    "lmstudio",
    "llamacpp",
    "vllm",
    "openrouter",
];

/// Maps provider names to adapters.
#[derive(Clone)]
pub struct ProviderFactory {
    builders: HashMap<String, Arc<BuildFn>>,
}

impl ProviderFactory {
    /// Only what is built in: the OpenAI-compatible family.
    pub fn standard() -> Self {
        Self::openai_family(None)
    }

    /// Like [`standard`](Self::standard) but with short timeouts (5 s to connect, 20 s in all), for
    /// the "Test" button: a server that does not answer must not leave the screen waiting for the
    /// ten minutes a real image job is allowed.
    pub fn probing() -> Self {
        Self::openai_family(Some((Duration::from_secs(5), Duration::from_secs(20))))
    }

    fn openai_family(timeouts: Option<(Duration, Duration)>) -> Self {
        let mut f = Self {
            builders: HashMap::new(),
        };
        for name in OPENAI_FAMILY {
            f.register(name, move |c, s| build_openai(c, s, timeouts));
        }
        f
    }

    /// Adds or replaces the adapter for a provider name.
    pub fn register(
        &mut self,
        name: &str,
        build: impl Fn(&Connection, Option<SecretString>) -> Result<Arc<dyn Provider>, AiError>
        + Send
        + Sync
        + 'static,
    ) {
        self.builders
            .insert(name.to_ascii_lowercase(), Arc::new(build));
    }

    pub fn knows(&self, name: &str) -> bool {
        self.builders.contains_key(&name.to_ascii_lowercase())
    }

    pub fn build(
        &self,
        connection: &Connection,
        secret: Option<SecretString>,
    ) -> Result<Arc<dyn Provider>, AiError> {
        let build = self
            .builders
            .get(&connection.provider.to_ascii_lowercase())
            .ok_or_else(|| {
                AiError::unsupported(format!(
                    "this app has no adapter for the \"{}\" provider of connection \"{}\"",
                    connection.provider, connection.label
                ))
            })?;
        build(connection, secret)
    }
}

impl Default for ProviderFactory {
    fn default() -> Self {
        Self::standard()
    }
}

impl std::fmt::Debug for ProviderFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<_> = self.builders.keys().collect();
        names.sort();
        write!(f, "ProviderFactory({names:?})")
    }
}

fn build_openai(
    connection: &Connection,
    secret: Option<SecretString>,
    timeouts: Option<(Duration, Duration)>,
) -> Result<Arc<dyn Provider>, AiError> {
    let endpoint = connection.endpoint.as_deref().ok_or_else(|| {
        AiError::invalid(format!(
            "connection \"{}\" has no address to connect to",
            connection.label
        ))
    })?;
    let mut config = OpenAiConfig::new(endpoint);
    config.api_key = secret;
    if let Some((connect, request)) = timeouts {
        config.connect_timeout = connect;
        config.request_timeout = request;
    }
    // The models the user saw in the launcher are the ones requests are checked against.
    if !connection.models.is_empty() {
        config = config.with_models(connection.models.clone());
    }
    Ok(Arc::new(OpenAiProvider::new(config)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(provider: &str, endpoint: Option<&str>) -> Connection {
        let mut v = serde_json::json!({
            "id": "c", "label": "Test", "provider": provider,
            "locality": "local", "auth": {"kind": "none"}, "billing": "free", "models": []
        });
        if let Some(e) = endpoint {
            v["endpoint"] = serde_json::json!(e);
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn standard_knows_the_openai_family_only() {
        let f = ProviderFactory::standard();
        for n in ["openai", "OLLAMA", "lmstudio", "openrouter"] {
            assert!(f.knows(n), "{n}");
        }
        assert!(!f.knows("fal"));
    }

    #[test]
    fn unknown_provider_names_the_provider_and_connection() {
        let e = ProviderFactory::standard()
            .build(&conn("fal", Some("http://127.0.0.1:1/v1")), None)
            .err()
            .unwrap();
        assert!(e.message.contains("fal") && e.message.contains("Test"));
    }

    #[test]
    fn openai_family_needs_an_address() {
        assert!(
            ProviderFactory::standard()
                .build(&conn("openai", None), None)
                .is_err()
        );
        assert!(
            ProviderFactory::standard()
                .build(&conn("ollama", Some("http://127.0.0.1:11434/v1")), None)
                .is_ok()
        );
    }

    #[test]
    fn register_replaces_and_debug_lists_names_only() {
        let mut f = ProviderFactory::standard();
        f.register("fal", |_, _| Err(AiError::unsupported("stub")));
        assert!(f.knows("fal"));
        assert!(format!("{f:?}").contains("fal"));
    }
}
