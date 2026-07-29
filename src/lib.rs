//! Valence-backed `ContextFactory` for product `#[chronon_coordinator_macros::script]` handlers.
//!
//! ## Features
//!
//! - **Context factory** — [`ValenceScriptContextFactory`] wraps a `valence::ValenceFactory` as a
//!   [`chronon_core::ContextFactory`]; construct via [`ValenceScriptContextFactory::new`] and
//!   register with the host Chronon runtime.
//! - **Invoke-scoped recovery** — [`valence_from_context`] hands a running script handler the
//!   [`valence::Valence`] staged for that dispatch (process map keyed by invoke id — async-safe).
//! - **Direct construction** — [`ValenceScriptContextFactory::build_valence`] builds a
//!   [`valence::Valence`] straight from actor JSON, for callers that don't go through Chronon dispatch.
//!
//! ## Security
//!
//! Hosts that accept client-supplied actor JSON **must** install
//! [`RejectExternalSystemActor`] via [`router_config_reject_external_system`]. Internal
//! schedulers that mint System actors should set [`valence::ActorTrust::Internal`].
//!
//! # Concern → API
//!
//! | Concern | API |
//! |---------|-----|
//! | Register with the host Chronon runtime | [`ValenceScriptContextFactory::new`] |
//! | Recover `Valence` inside a script handler | [`valence_from_context`] |
//! | Build `Valence` directly from actor JSON | [`ValenceScriptContextFactory::build_valence`] |
//! | Default external-safe router config | [`router_config_reject_external_system`] |
//!
//! Runnable deep dive: `cargo run -p chronon-valence-identity --example wire_factory`
//!
//! # Highlights
//!
//! Config → factory → direct build / dispatch recover:
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use chronon_core::ContextFactory;
//! use chronon_valence_identity::{
//!     router_config_reject_external_system, valence_from_context, ValenceScriptContextFactory,
//! };
//! use valence::{
//!     DatabaseRouter, InMemoryBackend, RouterValenceFactory, DEFAULT_IN_MEMORY_ROUTER_KEY,
//! };
//!
//! let mut router = DatabaseRouter::new();
//! router.register(
//!     DEFAULT_IN_MEMORY_ROUTER_KEY.to_string(),
//!     Arc::new(InMemoryBackend::new()),
//! );
//! let valence_factory = RouterValenceFactory::arc(
//!     Arc::new(router),
//!     router_config_reject_external_system(DEFAULT_IN_MEMORY_ROUTER_KEY),
//! );
//! let factory = ValenceScriptContextFactory::new(valence_factory);
//! let actor = serde_json::json!({"User": {"user_id": "u1"}});
//! let _direct = factory.build_valence(&actor)?;
//! let ctx = factory.build(&actor)?;
//! let _recovered = valence_from_context(&*ctx)?;
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;
use valence::{RejectExternalSystemActor, RouterValenceFactoryConfig};

static NEXT_INVOKE_ID: AtomicU64 = AtomicU64::new(1);
static STAGED_VALENCE: OnceLock<Mutex<HashMap<u64, valence::Valence>>> = OnceLock::new();

fn staged_map() -> &'static Mutex<HashMap<u64, valence::Valence>> {
    STAGED_VALENCE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Router config that rejects System-shaped actors on the default external trust path.
///
/// # Examples
///
/// ```rust,ignore
/// use valence::{RouterValenceFactory, DEFAULT_IN_MEMORY_ROUTER_KEY};
/// use chronon_valence_identity::router_config_reject_external_system;
///
/// let config = router_config_reject_external_system(DEFAULT_IN_MEMORY_ROUTER_KEY);
/// let _factory = RouterValenceFactory::arc(router, config);
/// ```
#[must_use]
pub fn router_config_reject_external_system(
    default_backend_key: impl Into<String>,
) -> RouterValenceFactoryConfig {
    RouterValenceFactoryConfig::new(default_backend_key)
        .actor_json_policy(RejectExternalSystemActor)
}

/// Recover the staged [`valence::Valence`] for the current script dispatch.
///
/// Valid only inside a running script handler when [`ValenceScriptContextFactory`] built the
/// context. Until then, returns an internal error.
///
/// # Errors
///
/// Returns [`chronon_core::ChrononError::Internal`] when the context label is not an invoke id
/// from this factory, or when the staged [`valence::Valence`] was already taken.
///
/// # Examples
///
/// ```rust,ignore
/// use chronon_core::ContextFactory;
/// use chronon_valence_identity::{valence_from_context, ValenceScriptContextFactory};
///
/// let factory = ValenceScriptContextFactory::new(valence_factory);
/// let ctx = factory.build(&actor_json)?;
/// let valence = valence_from_context(&*ctx)?;
/// let _ = valence.database_router();
/// ```
pub fn valence_from_context(
    ctx: &dyn chronon_core::ScriptContext,
) -> chronon_core::Result<valence::Valence> {
    let invoke_id = parse_invoke_id(ctx.label()).ok_or_else(|| {
        chronon_core::ChrononError::Internal(
            "missing invoke valence (invalid script context label)".into(),
        )
    })?;
    take_staged(invoke_id)
}

fn parse_invoke_id(label: &str) -> Option<u64> {
    let rest = label.strip_prefix("invoke:")?;
    let id_str = rest.split_once('|').map_or(rest, |(id, _)| id);
    id_str.parse().ok()
}

fn take_staged(invoke_id: u64) -> chronon_core::Result<valence::Valence> {
    staged_map()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .remove(&invoke_id)
        .ok_or_else(|| chronon_core::ChrononError::Internal("missing invoke valence".into()))
}

/// Wraps a [`valence::ValenceFactory`] as a [`chronon_core::ContextFactory`].
#[derive(Clone)]
pub struct ValenceScriptContextFactory {
    inner: Arc<dyn valence::ValenceFactory>,
}

impl ValenceScriptContextFactory {
    /// Create from a host `valence::ValenceFactory`.
    ///
    /// Prefer a factory built with [`router_config_reject_external_system`] unless this worker
    /// intentionally accepts System actors (`ActorTrust::Internal`).
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use chronon_valence_identity::ValenceScriptContextFactory;
    ///
    /// let factory = ValenceScriptContextFactory::new(valence_factory);
    /// ```
    pub fn new(inner: Arc<dyn valence::ValenceFactory>) -> Self {
        Self { inner }
    }

    /// Build a [`valence::Valence`] for script dispatch from serialized actor JSON.
    ///
    /// # Errors
    ///
    /// Returns [`chronon_core::IdentityError`] when the inner factory rejects the actor JSON.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// let actor = serde_json::json!({"User": {"user_id": "u1"}});
    /// let valence = factory.build_valence(&actor)?;
    /// let _ = valence.database_router();
    /// ```
    pub fn build_valence(
        &self,
        actor_json: &Value,
    ) -> std::result::Result<valence::Valence, chronon_core::IdentityError> {
        self.inner
            .build(actor_json)
            .map_err(|e| chronon_core::IdentityError(e.to_string()))
    }

    /// Always errors; use [`valence_from_context`] with the dispatch [`chronon_core::ScriptContext`].
    ///
    /// # Errors
    ///
    /// Always returns [`chronon_core::ChrononError::Internal`] directing callers to
    /// [`valence_from_context`].
    pub fn take_invoke_valence() -> chronon_core::Result<valence::Valence> {
        Err(chronon_core::ChrononError::Internal(
            "missing invoke valence (use valence_from_context with the dispatch ScriptContext)"
                .into(),
        ))
    }
}

struct ValenceScriptContext {
    label: String,
    actor_json: Value,
}

impl chronon_core::ScriptContext for ValenceScriptContext {
    fn label(&self) -> &str {
        &self.label
    }

    fn actor_json(&self) -> &Value {
        &self.actor_json
    }
}

impl chronon_core::ContextFactory for ValenceScriptContextFactory {
    fn build(
        &self,
        actor_json: &Value,
    ) -> chronon_core::Result<Box<dyn chronon_core::ScriptContext>> {
        let valence = self.build_valence(actor_json)?;
        let invoke_id = NEXT_INVOKE_ID.fetch_add(1, Ordering::Relaxed);
        staged_map()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(invoke_id, valence);
        let actor_label = serde_json::to_string(actor_json).unwrap_or_else(|_| "actor".into());
        let label = format!("invoke:{invoke_id}|{actor_label}");
        Ok(Box::new(ValenceScriptContext {
            label,
            actor_json: actor_json.clone(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chronon_core::{ChrononError, ContextFactory, IdentityError};
    use std::sync::Arc;
    use valence::{
        ActorTrust, InMemoryBackend, RouterValenceFactory, ValenceFactory,
        DEFAULT_IN_MEMORY_ROUTER_KEY,
    };

    fn mem_factory_external() -> Arc<dyn ValenceFactory> {
        let mut router = valence::DatabaseRouter::new();
        router.register(
            DEFAULT_IN_MEMORY_ROUTER_KEY.to_string(),
            Arc::new(InMemoryBackend::new()),
        );
        RouterValenceFactory::arc(
            Arc::new(router),
            router_config_reject_external_system(DEFAULT_IN_MEMORY_ROUTER_KEY),
        )
    }

    fn mem_factory_internal() -> Arc<dyn ValenceFactory> {
        let mut router = valence::DatabaseRouter::new();
        router.register(
            DEFAULT_IN_MEMORY_ROUTER_KEY.to_string(),
            Arc::new(InMemoryBackend::new()),
        );
        let mut config = router_config_reject_external_system(DEFAULT_IN_MEMORY_ROUTER_KEY);
        config.actor_trust = ActorTrust::Internal;
        RouterValenceFactory::arc(Arc::new(router), config)
    }

    struct FailValenceFactory;

    impl ValenceFactory for FailValenceFactory {
        fn build(&self, _actor_json: &Value) -> valence::Result<valence::Valence> {
            Err(valence::Error::Identity("factory build failed".into()))
        }
    }

    #[test]
    fn external_factory_rejects_system_actor_json() {
        let factory = ValenceScriptContextFactory::new(mem_factory_external());
        let actor = serde_json::json!({"System": {"operation": "test"}});
        match factory.build(&actor) {
            Ok(_) => panic!("System must be rejected on external trust"),
            Err(ChrononError::Internal(msg)) => assert!(msg.contains("System")),
            Err(other) => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn factory_stages_valence_for_invoke() {
        let factory = ValenceScriptContextFactory::new(mem_factory_internal());
        let actor = serde_json::json!({"System": {"operation": "test"}});
        let ctx = factory.build(&actor).expect("ok");
        assert_eq!(ctx.actor_json(), &actor);
        assert!(ctx.label().starts_with("invoke:"));
        let v = valence_from_context(ctx.as_ref()).expect("staged");
        let _ = v.database_router();
    }

    #[test]
    fn build_valence_roundtrip() {
        let factory = ValenceScriptContextFactory::new(mem_factory_internal());
        let v = factory
            .build_valence(&serde_json::json!({"System": {"operation": "t"}}))
            .expect("valence");
        let _ = v.database_router();
    }

    #[test]
    fn take_invoke_valence_without_context_errors() {
        match ValenceScriptContextFactory::take_invoke_valence() {
            Ok(_) => panic!("expected missing invoke valence"),
            Err(ChrononError::Internal(msg)) => assert!(msg.contains("missing invoke valence")),
            Err(other) => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn valence_from_context_errors_when_already_taken() {
        let factory = ValenceScriptContextFactory::new(mem_factory_internal());
        let actor = serde_json::json!({"System": {"operation": "test"}});
        let ctx = factory.build(&actor).expect("ok");
        let _ = valence_from_context(ctx.as_ref()).expect("first");
        match valence_from_context(ctx.as_ref()) {
            Ok(_) => panic!("expected unstaged recovery to fail"),
            Err(ChrononError::Internal(msg)) => assert!(msg.contains("missing invoke valence")),
            Err(other) => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn invoke_valence_is_one_shot() {
        let factory = ValenceScriptContextFactory::new(mem_factory_internal());
        let actor = serde_json::json!({"System": {"operation": "once"}});
        let ctx = factory.build(&actor).expect("ok");
        let _ = valence_from_context(ctx.as_ref()).expect("first take");
        match valence_from_context(ctx.as_ref()) {
            Ok(_) => panic!("second take should fail"),
            Err(ChrononError::Internal(msg)) => assert!(msg.contains("missing invoke valence")),
            Err(other) => panic!("expected Internal, got {other:?}"),
        }
    }

    #[test]
    fn build_valence_maps_factory_failure() {
        let factory = ValenceScriptContextFactory::new(Arc::new(FailValenceFactory));
        match factory.build_valence(&serde_json::json!({"System": {"operation": "x"}})) {
            Ok(_) => panic!("build should fail"),
            Err(IdentityError(msg)) => assert!(msg.contains("factory build failed")),
        }
    }

    #[test]
    fn context_factory_build_maps_identity_error() {
        let factory = ValenceScriptContextFactory::new(Arc::new(FailValenceFactory));
        match factory.build(&serde_json::json!({"System": {"operation": "x"}})) {
            Ok(_) => panic!("build should fail"),
            Err(ChrononError::Internal(msg)) => assert!(msg.contains("factory build failed")),
            Err(other) => panic!("expected Internal, got {other:?}"),
        }
    }
}
