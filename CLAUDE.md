# CLAUDE.md

## Project Overview

dash-spv-ui is a cross-platform Dash SPV wallet GUI built with Dioxus 0.7 and Tailwind CSS v4. It serves as both an end-user lightweight wallet and a developer testing tool for dash-spv.

## Architecture

```
┌─────────────────────────────────┐
│  Dioxus Components (RSX+TW)    │  ← thin, no logic, renders state
├─────────────────────────────────┤
│  View Models / State            │  ← pure Rust structs, formatting,
│                                 │     state machines, fully testable
├─────────────────────────────────┤
│  SpvBackend trait               │  ← async trait, no UI dependency
│  + event stream (channels)      │
├────────────┬────────────────────┤
│ NativeBackend │  FfiBackend     │
│ (dash-spv)    │  (dash-spv-ffi) │
└────────────┴────────────────────┘
```

### Layer Boundaries (STRICT)

- Dioxus components (RSX) must ONLY render state — no business logic, no direct backend calls
- Components receive state as props and emit actions/callbacks — never mutate backend state directly
- View models and state types must have ZERO dependency on Dioxus — pure Rust only
- `SpvBackend` trait must have ZERO dependency on Dioxus or any UI framework
- All formatting (balance, addresses, dates) happens in the view model layer, not in components
- `unsafe` code is only permitted in `backend/ffi.rs` for FFI calls

### File Organization

```
src/
├── main.rs              # CLI args, dioxus::launch
├── app.rs               # Root component, routing
├── components/          # Dioxus RSX components (thin, no logic)
├── screens/             # Full-page screen components
├── backend/             # SpvBackend trait + implementations
│   ├── mod.rs
│   ├── trait.rs         # SpvBackend trait definition
│   ├── types.rs         # Shared backend types
│   ├── events.rs        # Event types
│   ├── mock.rs          # Mock implementation for testing
│   ├── native.rs        # NativeBackend (dash-spv direct)
│   └── ffi.rs           # FfiBackend (dash-spv-ffi)
├── state/               # Pure Rust state types + view models
│   ├── mod.rs
│   ├── app_state.rs     # Top-level app state
│   ├── connection.rs    # Connection state machine
│   ├── wallet.rs        # Wallet state
│   ├── network.rs       # Network info
│   ├── view_models.rs   # Formatting functions
│   └── dev_log.rs       # Dev mode event log
└── theme.rs             # Tailwind color palette, Dash branding
```

## Build Commands

```bash
# Development
dx serve                    # Dev server with hot-reload
dx serve --desktop          # Desktop-specific

# Production
dx build --release          # Release build

# Testing
cargo test --lib            # Run tests (skip doc-tests)
cargo test --lib -- --nocapture  # With output

# Linting
cargo fmt --check           # Check formatting
cargo clippy --all-targets -- -D warnings  # Lint
dx fmt --check              # Check RSX formatting
dx check                    # Check rules-of-hooks
```

## Code Style

- Imports always go at the top of the file/module — never inline
- Use the most restrictive visibility possible (default to private)
- Avoid numeric type suffixes when the type is clear from context

## Testing Rules

- Every public function in `state/` and `backend/` must have tests
- View model tests use `#[test]`, not Dioxus rendering
- Backend tests run against the mock implementation
- Use `cargo test --lib` to skip doc-test compilation
- Write tests early, test all critical edge cases
- Before writing a new test function, check if an existing test can be extended

## Anti-Patterns

- DO NOT put business logic in Dioxus components
- DO NOT import dioxus in `state/` or `backend/` modules (except `backend/native.rs` bridge code if needed)
- DO NOT use `unsafe` outside of `backend/ffi.rs`
- DO NOT format values (balance, addresses) in component code — use view model functions
- DO NOT call backend methods directly from components — go through state/actions
