//! Minimal in-memory wiring for Valence script-context factory.
//!
//! Uses [`chronon_valence_identity::router_config_reject_external_system`] so System-shaped
//! actor JSON is rejected.

#![allow(clippy::print_stderr)]

use std::sync::Arc;

use chronon_core::ContextFactory;
use chronon_valence_identity::{
    router_config_reject_external_system, valence_from_context, ValenceScriptContextFactory,
};
use valence::{
    DatabaseRouter, InMemoryBackend, RouterValenceFactory, ValenceFactory,
    DEFAULT_IN_MEMORY_ROUTER_KEY,
};

fn mem_factory_external() -> Arc<dyn ValenceFactory> {
    let mut router = DatabaseRouter::new();
    router.register(
        DEFAULT_IN_MEMORY_ROUTER_KEY.to_string(),
        Arc::new(InMemoryBackend::new()),
    );
    RouterValenceFactory::arc(
        Arc::new(router),
        router_config_reject_external_system(DEFAULT_IN_MEMORY_ROUTER_KEY),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let factory = ValenceScriptContextFactory::new(mem_factory_external());
    let actor = serde_json::json!({"User": {"user_id": "wire-factory-user"}});

    let direct = factory.build_valence(&actor)?;
    let _ = direct.database_router();

    let context = factory.build(&actor)?;
    let recovered = valence_from_context(context.as_ref())?;
    let _ = recovered.database_router();

    let system = serde_json::json!({"System": {"operation": "wire_factory"}});
    match factory.build_valence(&system) {
        Ok(_) => return Err("external factory must reject System actor JSON".into()),
        Err(e) => eprintln!("wire_factory: System rejected as expected — {e}"),
    }

    eprintln!("wire_factory: OK — built and recovered Valence with external System reject");
    Ok(())
}
