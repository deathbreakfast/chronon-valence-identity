# Security Policy

## Supported versions

Security fixes are accepted against the latest `main` branch and tagged releases (`0.1.x`) of this repository's crates (`chronon-valence-identity`).

## Reporting a vulnerability

Please **do not** open a public GitHub issue for security-sensitive reports.

Prefer one of the following:

1. **GitHub Security Advisories** — use [Report a vulnerability](https://github.com/unified-field-dev/chronon-valence-identity/security/advisories/new) on this repository when available.
2. Contact the maintainers privately via the repository owner listed at https://github.com/unified-field-dev/chronon-valence-identity.

Include:

- a description of the issue and its impact
- steps to reproduce or a proof of concept when possible
- affected crate names and versions

We will acknowledge receipt as soon as practical and coordinate a fix and disclosure timeline with you.

## Scope

In scope: vulnerabilities in this repository's published crates and documentation that could cause unsafe production defaults, plus CI/supply-chain issues in this repository.

Out of scope: vulnerabilities solely in third-party dependencies unless this project mishandles them in a security-relevant way.

## Host checklist: Valence factory trust

1. **External script/API paths** — build the inner [`ValenceFactory`] with
   [`router_config_reject_external_system`](src/lib.rs) so client JSON cannot mint
   `Actor::System`.
2. **Internal schedulers** — set `config.actor_trust = ActorTrust::Internal` when System
   actors are required for platform jobs.
3. **Invoke recovery** — use [`valence_from_context`](src/lib.rs) with the dispatch
   `ScriptContext`; staged Valence is keyed by invoke id (not thread-local).
