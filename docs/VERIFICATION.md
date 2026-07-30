# chronon-valence-identity verification

Re-run after code or doc changes. Chronon `ContextFactory` over Valence —
covered by unit + integration tests below.

## Environment

```bash
export CARGO_BUILD_JOBS=1
export CARGO_TARGET_DIR=target-chronon-valence-identity
```

## Unit + integration (CI)

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### TEST_MAP

| Behavior | Level | Happy | Sad | Notes |
|----------|-------|-------|-----|-------|
| `ValenceScriptContextFactory::new` | unit | wraps `ValenceFactory` | — | construction only |
| `build_valence` | unit + integ | returns usable `Valence` | maps inner `ValenceFactory` failure → `IdentityError` | direct construction |
| `ContextFactory::build` | unit + integ | stages invoke valence; context `label` + `actor_json` | maps failure → `ChrononError::Internal` (via `IdentityError`) | Chronon dispatch path |
| `valence_from_context` / `take_invoke_valence` | unit + integ | recovers staged `Valence` | empty / second take → `ChrononError::Internal` / `"missing invoke valence"` | process `OnceLock` + `Mutex<HashMap>` keyed by invoke id |

## Notes

- Tests may `unwrap`/`expect`; production paths map failures to typed Chronon /
  identity errors (no ordinary-path unwrap).
- Sad-path assertions check typed variants and message content, (stronger than `is_err()` alone).
