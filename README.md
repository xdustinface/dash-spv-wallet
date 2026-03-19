# dash-spv-ui

Cross-platform Dash SPV wallet GUI built with [Dioxus](https://dioxuslabs.com/) and Tailwind CSS.

## Features

- Lightweight SPV wallet (no full node required)
- Standard HD wallet with mnemonic backup
- Real-time sync progress and peer monitoring
- Transaction history with InstantSend/ChainLock indicators
- Developer mode for testing and debugging
- Supports mainnet, testnet, and regtest

## Architecture

```
┌─────────────────────────────────┐
│  Dioxus Components (RSX+TW)    │  ← thin, no logic
├─────────────────────────────────┤
│  View Models / State            │  ← pure Rust, fully testable
├─────────────────────────────────┤
│  SpvBackend trait               │  ← async, framework-agnostic
├────────────┬────────────────────┤
│ NativeBackend │  FfiBackend     │
│ (dash-spv)    │  (dash-spv-ffi) │
└────────────┴────────────────────┘
```

## Prerequisites

- [Rust](https://rustup.rs/) (see `rust-toolchain.toml` for version)
- [Node.js](https://nodejs.org/) 22+ (for Tailwind CSS)
- [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started/): `cargo install dioxus-cli`

### Platform-specific

**Linux:**
```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libudev-dev
```

## Getting Started

```bash
# Install dependencies
npm install

# Run in development mode
dx serve

# Build for release
dx build --release

# Run tests
cargo test --lib
```

## Development

### Pre-commit hooks

```bash
pip install pre-commit
pre-commit install
pre-commit install --hook-type pre-push
```

### Branch conventions

- `dev` — active development
- `main` — releases
- Feature branches: `feat/<description>`
- Fix branches: `fix/<description>`

## License

MIT
