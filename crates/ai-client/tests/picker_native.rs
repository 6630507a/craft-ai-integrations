//! A built-in model must not advertise an actionable setup route to an app without its engine.
use craft_ai_client::{AiClient, GroupState, ProviderFactory, SetupLink, ToolDesk};
use craft_ai_core::{AiError, AppId, ModelId};
use craft_ai_native::secrets::MemoryStore;
use craft_ai_native::shared::SharedStore;
use std::sync::Arc;

/// The built-in model offers setup only when this app has registered the engine.
#[test]
fn the_builtin_model_only_offers_setup_when_this_app_has_the_engine() {
    let dir = tempfile::tempdir().unwrap();
    let client = AiClient::new(
        AppId::new("test-app").unwrap(),
        SharedStore::new(dir.path().join("connections.json")),
        Arc::new(MemoryStore::new()),
    );
    let features = craft_ai_features::builtin();
    let role = features
        .iter()
        .find(|feature| feature.id == "select-subject")
        .unwrap()
        .role()
        .unwrap();
    let view = client
        .picker(&role, &craft_ai_features::setup_options(&features))
        .unwrap();
    let native = view
        .groups
        .iter()
        .find(|g| g.title.contains("built in"))
        .unwrap();
    if cfg!(feature = "candle") {
        assert_eq!(native.state, GroupState::NotSetUp);
        assert!(matches!(&native.setup, Some(SetupLink::New { option })
            if option == "native:select-subject:candle"));
    } else {
        assert_eq!(native.state, GroupState::NeedsAdapter);
        assert!(native.setup.is_none());
    }
}

/// A saved connection with this id but an older model on it does not stand for the current model:
/// the file is there, yet the option for the model the recipe now names keeps its Set up link.
#[test]
fn an_older_model_on_the_same_connection_does_not_hide_the_current_one() {
    let dir = tempfile::tempdir().unwrap();
    let shared = SharedStore::new(dir.path().join("connections.json"));
    let mut factory = ProviderFactory::standard();
    factory.register("candle", |_, _| Err(AiError::unsupported("not run here")));
    let client = AiClient::new(
        AppId::new("test-app").unwrap(),
        shared.clone(),
        Arc::new(MemoryStore::new()),
    )
    .with_factory(factory);
    let features = craft_ai_features::builtin();
    let feature = features
        .iter()
        .find(|feature| feature.id == "select-subject")
        .unwrap();
    let role = feature.role().unwrap();
    let weights = dir.path().join("model.safetensors");
    std::fs::write(&weights, b"weights").unwrap();
    let launcher = AiClient::new(
        AppId::new("launcher").unwrap(),
        shared.clone(),
        Arc::new(MemoryStore::new()),
    );
    ToolDesk::new(launcher, shared.clone(), None)
        .enable_native(feature, &weights)
        .unwrap();
    let mut store = shared.load().unwrap();
    store.connections[0].models[0].id = ModelId::new("sam3-older").unwrap();
    shared.replace(&store).unwrap();
    let options = craft_ai_features::setup_options(&features);
    let view = client.picker(&role, &options).unwrap();
    assert!(
        view.groups.iter().any(|g| g.title.contains("built in")
            && g.state == GroupState::NotSetUp
            && g.setup.is_some()),
        "the current model still has a way in"
    );
}
