# 3D Model Generator - Project Documentation

## Technology Stack

- **Language**: Rust (edition 2024)

## Rust Best Practices

### Module Structure — Feature-Sliced & Granular

**Prefer many small concern-focused files over a few large canonical ones.** A `damage.rs` file holding the `DamageMultiplier` type + `apply_damage` function + damage constants together is preferred over the same code split across `types.rs`/`functions.rs`/`constants.rs` mixed with 30 unrelated entries.

**Hard rules:**
- `mod.rs` does `mod` declarations + `pub use` re-exports ONLY. No logic, no constants, no types.
- Files exceeding ~300 lines must be split unless every line is genuinely cohesive (e.g., a single large match-on-enum or a single asset registry).
- `styles.rs` is forbidden. Constants live with their feature, or in a `constants.rs` for cross-cutting values only.

**Feature-slicing rule:** When splitting a module with multiple concerns, group by concern, not by file type. Reserve canonical names (`types.rs`, `constants.rs`) for genuinely cross-cutting / shared content.

### Module Visibility
- Use `pub(super)` for items only needed within a module
- Use `pub(crate)` for crate-internal APIs
- Only use `pub` for true public API

### Function Arguments
- **Helper functions** should keep argument counts reasonable. When multiple helpers share the same parameter group, extract a params struct.
- **Constructors** with many fields: prefer `#[allow(clippy::too_many_arguments)]` on `new()` since the arguments map 1:1 to struct fields.

### Constants Organization
- Crate-wide constants live in a top-level `constants.rs`. Split by concern when that file exceeds ~200 lines.
- Module-specific constants either live in feature files alongside the code that uses them, or in a `constants.rs` if they are shared across feature files.
- Constants used by exactly one feature file should be inlined there.
- **`styles.rs` is forbidden.** Colors, dimensions, and styling go in feature files or `constants.rs`.
- Use `pub(super)` for module-internal constants.

### Code Sharing
- Extract common logic into shared functions rather than duplicating code.
- When adding new functionality, check existing implementations for shared patterns.
- Feature-specific behavior should be minimal overrides on top of shared helpers.

### Error Handling
- Use `Result<T, E>` for fallible operations
- Use `thiserror` for library error types and `anyhow` for application error context
- Never use `.unwrap()` in production code
- Use `.expect()` only for invariants with descriptive messages

### Logging
- Use the `tracing` (or `log`) macros: `info!`, `warn!`, `error!`, `debug!`
- Avoid excessive logging in production code
- Remove debug logging after debugging is complete

### Code Simplification
- Always `/simplify` before releasing to optimize the codebase from a duplication standpoint.

## Build & Testing

### Iterative compile checks (USE THIS during work)
While iterating — refactoring, splitting files, fixing import errors — use `cargo check` instead of a full build. It runs the full compiler frontend (so it catches every type error, missing import, visibility issue, etc.) but skips codegen, making it much faster.

```bash
cargo check
```

You can also use `cargo fix --allow-dirty` to auto-remove unused imports surfaced by `cargo check`.

### Testing
```bash
cargo test
cargo clippy -- -D warnings
cargo fmt --check
```

## Git Workflow
- Never commit unless explicitly instructed
