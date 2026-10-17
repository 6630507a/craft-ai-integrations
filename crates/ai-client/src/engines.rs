//! Turning a registry of built-in model runners into adapters the client can build.

use std::sync::Arc;

use craft_ai_engine::{EngineProvider, EngineRegistry};

use crate::factory::ProviderFactory;

impl ProviderFactory {
    /// Makes every engine in `registry` available: a saved connection whose provider is one of
    /// the registry's engines (`candle`, `burn`) is built as an [`EngineProvider`] that runs its
    /// model from the weights file recorded on the connection. The accelerators each engine
    /// declares are recorded too, so engine advice needs nothing stated by the app.
    ///
    /// This is the whole of enabling a built-in engine in an app.
    pub fn register_engines(&mut self, registry: EngineRegistry) {
        let registry = Arc::new(registry);
        for engine in registry.engines() {
            let mut vendors = Vec::new();
            for runner in registry.runners_of(&engine) {
                for vendor in runner.accelerators() {
                    if !vendors.contains(vendor) {
                        vendors.push(*vendor);
                    }
                }
            }
            let registry = registry.clone();
            self.register(&engine, move |connection, _| {
                Ok(Arc::new(EngineProvider::from_connection(
                    &registry, connection,
                )?))
            });
            // After registering: registering a name forgets any earlier declaration.
            self.declare_accelerators(&engine, &vendors);
        }
    }
}

/// The engines compiled into this build, by cargo feature. Empty when none is turned on.
pub(crate) fn built_in() -> EngineRegistry {
    #[allow(unused_mut)]
    let mut registry = EngineRegistry::new();
    #[cfg(feature = "candle")]
    registry.extend(craft_ai_engine_candle::runners());
    registry
}
