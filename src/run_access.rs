//! Re-check a person's `Run` access when their run starts.
//!
//! A person who presses Run now is checked as themselves when the request comes in. Access can
//! be removed before the run starts, so the run is checked again, as the same person, at start.
//! [`check_run_start`] does that second check:
//!
//! - A System actor passes without a check. Scheduled runs of code-declared jobs start as System
//!   through the host's context factory, which the no-elevation rule allows.
//! - Any other actor is rebuilt as itself through [`ValenceScriptContextFactory::build_valence`]
//!   and checked through the host's [`RunAccessCheck`]. Nothing elevates.
//! - A failed check returns [`RunStartError::Refused`]. Its text, `refused: missing Run on
//!   chronon_job.<id>`, is what the run records as its error, and [`parse_refused`] reads it
//!   back so an app can show the run as Refused with the missing action. A refused run must not
//!   be retried.
//!
//! The check itself lives with the host (usually Gauge's `actor_can` on
//! `chronon_job.<id>.Run`), so this crate takes no permission dependency.
//!
//! ```rust,ignore
//! use chronon_valence_identity::{check_run_start, RunAccessCheck, RunStartError};
//!
//! match check_run_start(&factory, &run.actor_json, "billing-rollup", &gauge_check).await {
//!     Ok(()) => { /* start the run */ }
//!     Err(RunStartError::Refused(refused)) => { /* end the run with refused.to_string() */ }
//!     Err(other) => { /* fail the run */ }
//! }
//! ```

use std::fmt;

use async_trait::async_trait;
use serde_json::Value;

use crate::ValenceScriptContextFactory;

/// Gauge resource kind prefix for Chronon jobs.
pub const CHRONON_JOB_KIND: &str = "chronon_job";

/// The action a run needs on its job.
pub const RUN_ACTION: &str = "Run";

/// Start of the error text a refused run records.
pub const REFUSED_PREFIX: &str = "refused: ";

/// Host check for one action on one Chronon job, made as the actor the `valence` belongs to.
///
/// Implementations must check as that actor (for example Gauge `actor_can` on
/// `chronon_job.<resource_id>.Run`) and never switch to System.
#[async_trait]
pub trait RunAccessCheck: Send + Sync {
    /// Whether the actor holds `action` on the job `resource_id`.
    ///
    /// `Err` means the check couldn't be made (a storage error); the run fails rather than
    /// starting.
    async fn can(
        &self,
        valence: &valence::Valence,
        resource_id: &str,
        action: &str,
    ) -> Result<bool, String>;
}

/// A run that started without the access it needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRefused {
    /// The missing action, `Run`.
    pub action: String,
    /// The job's resource id (script name, or the id of a job a person created).
    pub resource: String,
}

impl fmt::Display for RunRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{REFUSED_PREFIX}missing {} on {CHRONON_JOB_KIND}.{}",
            self.action, self.resource
        )
    }
}

impl std::error::Error for RunRefused {}

/// Why [`check_run_start`] didn't let a run start.
#[derive(Debug)]
pub enum RunStartError {
    /// The actor lacks the action. End the run as Refused and don't retry it.
    Refused(RunRefused),
    /// The actor JSON couldn't be turned into a Valence handle.
    Identity(chronon_core::IdentityError),
    /// The host check failed to run.
    Check(String),
}

impl fmt::Display for RunStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refused) => refused.fmt(f),
            Self::Identity(e) => write!(f, "identity: {}", e.0),
            Self::Check(e) => write!(f, "run access check failed: {e}"),
        }
    }
}

impl std::error::Error for RunStartError {}

/// Whether `actor_json` is a System actor (`{"System": {...}}`).
fn is_system_actor(actor_json: &Value) -> bool {
    actor_json.get("System").is_some()
}

/// Re-check `Run` on the job `resource_id` as the actor stored on the run.
///
/// System actors pass unchecked; see the module docs.
pub async fn check_run_start(
    factory: &ValenceScriptContextFactory,
    actor_json: &Value,
    resource_id: &str,
    check: &dyn RunAccessCheck,
) -> Result<(), RunStartError> {
    if is_system_actor(actor_json) {
        return Ok(());
    }
    let valence = factory
        .build_valence(actor_json)
        .map_err(RunStartError::Identity)?;
    match check.can(&valence, resource_id, RUN_ACTION).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(RunStartError::Refused(RunRefused {
            action: RUN_ACTION.to_owned(),
            resource: resource_id.to_owned(),
        })),
        Err(e) => Err(RunStartError::Check(e)),
    }
}

/// Read a refusal back from a run's error text, if the run was refused.
pub fn parse_refused(error: &str) -> Option<RunRefused> {
    let rest = error
        .strip_prefix(REFUSED_PREFIX)?
        .strip_prefix("missing ")?;
    let (action, target) = rest.split_once(" on ")?;
    let resource = target.strip_prefix(CHRONON_JOB_KIND)?.strip_prefix('.')?;
    if action.is_empty() || resource.is_empty() {
        return None;
    }
    Some(RunRefused {
        action: action.to_owned(),
        resource: resource.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router_config_reject_external_system;
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use valence::{
        ActorTrust, InMemoryBackend, RouterValenceFactory, ValenceFactory,
        DEFAULT_IN_MEMORY_ROUTER_KEY,
    };

    fn factory() -> ValenceScriptContextFactory {
        let mut router = valence::DatabaseRouter::new();
        router.register(
            DEFAULT_IN_MEMORY_ROUTER_KEY.to_string(),
            Arc::new(InMemoryBackend::new()),
        );
        let mut config = router_config_reject_external_system(DEFAULT_IN_MEMORY_ROUTER_KEY);
        config.actor_trust = ActorTrust::Internal;
        ValenceScriptContextFactory::new(RouterValenceFactory::arc(Arc::new(router), config))
    }

    struct Fixed {
        answer: Result<bool, String>,
        calls: AtomicUsize,
    }

    impl Fixed {
        fn new(answer: Result<bool, String>) -> Self {
            Self {
                answer,
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl RunAccessCheck for Fixed {
        async fn can(
            &self,
            _valence: &valence::Valence,
            _resource_id: &str,
            action: &str,
        ) -> Result<bool, String> {
            assert_eq!(action, RUN_ACTION);
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.answer.clone()
        }
    }

    struct FailValenceFactory;
    impl ValenceFactory for FailValenceFactory {
        fn build(&self, _actor_json: &Value) -> valence::Result<valence::Valence> {
            Err(valence::Error::Identity("factory build failed".into()))
        }
    }

    fn user() -> Value {
        json!({"User": {"user_id": "ravi"}})
    }

    #[tokio::test]
    async fn system_actor_skips_the_check() {
        let check = Fixed::new(Ok(false));
        let actor = json!({"System": {"operation": "schedule"}});
        check_run_start(&factory(), &actor, "billing-rollup", &check)
            .await
            .unwrap_or_else(|e| panic!("system should pass: {e}"));
        assert_eq!(check.calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn user_with_run_starts() {
        let check = Fixed::new(Ok(true));
        check_run_start(&factory(), &user(), "billing-rollup", &check)
            .await
            .unwrap_or_else(|e| panic!("allowed user should pass: {e}"));
        assert_eq!(check.calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn user_without_run_is_refused() {
        let check = Fixed::new(Ok(false));
        match check_run_start(&factory(), &user(), "billing-rollup", &check).await {
            Err(RunStartError::Refused(refused)) => {
                assert_eq!(
                    refused.to_string(),
                    "refused: missing Run on chronon_job.billing-rollup"
                );
            }
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn check_failure_is_not_a_refusal() {
        let check = Fixed::new(Err("store down".into()));
        match check_run_start(&factory(), &user(), "billing-rollup", &check).await {
            Err(RunStartError::Check(msg)) => assert_eq!(msg, "store down"),
            other => panic!("expected Check, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn factory_error_maps_to_identity() {
        let factory = ValenceScriptContextFactory::new(Arc::new(FailValenceFactory));
        let check = Fixed::new(Ok(true));
        match check_run_start(&factory, &user(), "billing-rollup", &check).await {
            Err(RunStartError::Identity(e)) => assert!(e.0.contains("factory build failed")),
            other => panic!("expected Identity, got {other:?}"),
        }
        assert_eq!(check.calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn parse_refused_round_trips() {
        let refused = RunRefused {
            action: "Run".into(),
            resource: "e2e_echo".into(),
        };
        assert_eq!(parse_refused(&refused.to_string()), Some(refused));
    }

    #[test]
    fn parse_refused_ignores_other_errors() {
        assert_eq!(parse_refused("Valence refused the connection"), None);
        assert_eq!(
            parse_refused("refused: missing Run on photon_topic.x"),
            None
        );
        assert_eq!(parse_refused("refused: missing Run on chronon_job."), None);
    }
}
