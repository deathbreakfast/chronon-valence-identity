# chronon-valence-identity

[![CI](https://github.com/unified-field-dev/chronon-valence-identity/actions/workflows/ci.yml/badge.svg)](https://github.com/unified-field-dev/chronon-valence-identity/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

[GitHub](https://github.com/unified-field-dev/chronon-valence-identity) · `cargo doc -p chronon-valence-identity --open`

Valence-backed `ContextFactory` for [Chronon](https://github.com/unified-field-dev/chronon) script handlers.

```toml
chronon-valence-identity = { git = "https://github.com/unified-field-dev/chronon-valence-identity" }
```

```rust
use chronon_valence_identity::{valence_from_context, ValenceScriptContextFactory};

async fn my_job(ctx: Box<dyn chronon_core::ScriptContext>) -> chronon_core::Result<()> {
    let valence = valence_from_context(&*ctx)?;
    // …
    Ok(())
}
```

## About

- `ValenceScriptContextFactory` — rebuild product `Valence` from stored `actor_json`
- `valence_from_context` — recover `Valence` from `dyn ScriptContext` inside a job

Wire `ValenceScriptContextFactory::new(valence_factory)` when building the host Chronon runtime.

## Examples

Canonical teaching path and run commands: [examples/README.md](examples/README.md).

## Verify

```bash
export CARGO_BUILD_JOBS=1
cargo test
```

## License

MIT. See [LICENSE](LICENSE), [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).
