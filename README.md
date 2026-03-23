<p align="center">
  <a href="https://www.dash.org">
    <img alt="Dash" src="https://media.dash.org/wp-content/uploads/dash_digital-cash_logo_2018_rgb_for_screens.png" width="400">
  </a>
</p>

<p align="center">
  Lightweight SPV wallet for the Dash network
</p>

<p align="center">
  <a href="https://github.com/xdustinface/dash-spv-wallet/actions/workflows/ci.yml">
    <img alt="CI" src="https://github.com/xdustinface/dash-spv-wallet/actions/workflows/ci.yml/badge.svg?branch=dev">
  </a>
  <a href="https://codecov.io/gh/xdustinface/dash-spv-wallet">
    <img alt="Coverage" src="https://codecov.io/gh/xdustinface/dash-spv-wallet/branch/dev/graph/badge.svg">
  </a>
</p>

## Overview

Dash SPV Wallet is a cross-platform desktop wallet built with [Dioxus](https://dioxuslabs.com/) and Tailwind CSS. It connects directly to the Dash P2P network via SPV — no full node required.

## Features

- HD wallet with mnemonic backup (BIP39/BIP44)
- Real-time sync progress with peer monitoring
- Transaction history with InstantSend/ChainLock indicators
- Network selection: Mainnet, Testnet, Regtest
- Developer mode with event log viewer
- Native and FFI backend support
- Expandable transaction details
- Persistent wallet and settings

## Architecture

```mermaid
block-beta
  columns 1
  block:ui["Dioxus Components (RSX + Tailwind CSS)"]
  end
  block:vm["View Models / State"]
  end
  block:trait["SpvBackend trait"]
  end
  block:backends
    columns 2
    native["NativeBackend\n(dash-spv)"]
    ffi["FfiBackend\n(dash-spv-ffi)"]
  end
```

## Prerequisites

- [Rust](https://rustup.rs/) (see `rust-toolchain.toml` for version)
- [Node.js](https://nodejs.org/) 22+ (for Tailwind CSS)
- [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started/): `cargo install dioxus-cli`

**Linux:**
```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libudev-dev libxdo-dev
```

## Getting Started

```bash
npm install
cargo run
```

Dev mode:
```bash
cargo run -- --dev
```

FFI backend:
```bash
cargo run --features ffi -- --dev --backend ffi
```

Build for release:
```bash
dx build --release
```

## Development

### Pre-commit hooks

```bash
pip install pre-commit
pre-commit install
pre-commit install --hook-type pre-push
```

### Running tests

```bash
cargo test --all-features --lib
```

Integration tests (requires a running `dashd` regtest node):
```bash
cargo test --all-features
```

### Branch conventions

- `dev` — active development
- `main` — releases
- Feature branches: `feat/<description>`
- Fix branches: `fix/<description>`

## License

MIT
