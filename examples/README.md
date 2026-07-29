# chronon-valence-identity examples

Canonical teaching path for Valence-backed Chronon script context — in-memory router;
examples build/recover Valence for script handlers.

## `wire_factory` — build, dispatch, recover Valence

Run when you want to confirm `ValenceScriptContextFactory` builds User Valence, wraps it in
`ScriptContext`, and `valence_from_context` recovers the same session inside a job handler.

```bash
cargo run -p chronon-valence-identity --example wire_factory
```

Success: stderr prints `wire_factory: System rejected as expected — …` (expected) and
`wire_factory: OK — built and recovered Valence with external System reject`.

## `persist_actor_recover` — file-persisted actor JSON → executor Valence

```bash
CARGO_BUILD_JOBS=1 cargo run --example persist_actor_recover
```

Success: stderr prints `persist_actor_recover: OK — actor persisted + executor Valence recovered`.

Walkthrough: the example registers an in-memory backend with
`router_config_reject_external_system`, builds Valence directly and via `factory.build`,
recovers through `valence_from_context`, then verifies external System JSON fails closed.

See `examples/wire_factory.rs`, then register
`ValenceScriptContextFactory::new(valence_factory)` on the host runtime.
