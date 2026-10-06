# Deprecations

Registry of temporary backward-compatibility shims: code kept only so a recent stable release can
still run after a downgrade. Shim sites the compiler cannot flag on removal are marked with a
`// DOWNGRADE-COMPAT(added X.Y.0, remove X.Y+2.0): why.` comment pointing here.

Policy: a change that stops writing a field which an older daemon hard-requires from persisted state
(config.toml, modes.json) keeps writing that field as a no-op for 2 minor releases. See
`coolercontrold/RUST_STYLE.md` (Downgrade Compatibility) for the convention and `RELEASING.md` for
the removal checklist.

| What                                                                                                                                                                                                   | Where                                           | Added | Remove |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------- | ----- | ------ |
| `lcd.colors` no-op field: 4.3.x requires it in config.toml and modes.json                                                                                                                              | `daemon/src/setting.rs`, `daemon/src/config.rs` | 5.0.0 | 5.2.0  |
| Alert `channel_source` single-source field: 4.3.x requires it in alerts.json; written as `channel_sources[0]`                                                                                          | `daemon/src/alerts.rs`                          | 5.0.0 | 5.2.0  |
| Per-stream SSE routes `/sse/{logs,status,modes,alerts,notifications}`: superseded by `GET /sse?events=`                                                                                                | `daemon/src/api/sse.rs`                         | 5.0.0 | 5.2.0  |
| `AlertLog.resolved`: superseded by `AlertLog.kind`; still written for alert-logs.json readers                                                                                                          | `daemon/src/alerts.rs`                          | 5.0.0 | 5.2.0  |
| Access-token argon2 `hash` field: 4.3.x validates tokens against it; superseded by the SHA-256 `digest`                                                                                                | `daemon/src/token.rs`                           | 5.0.0 | 5.2.0  |
| Custom sensor `offset` as a whole number: 5.0.x requires one; written where 5.0.x computes the same value. Not shimmed: 5.0.x refuses a config holding a `Sum` mix or a `channel_source` sensor source | `daemon/src/config.rs`                          | 5.1.0 | 5.3.0  |
