# Contributing to dash-spv-ui

## Getting Started

1. Fork the repository
2. Clone your fork
3. Set up pre-commit hooks:
   ```bash
   pip install pre-commit
   pre-commit install
   pre-commit install --hook-type pre-push
   ```
4. Install dependencies:
   ```bash
   npm install
   cargo build
   ```

## Development Workflow

### Branches

- `dev` — active development (PR target)
- `main` — releases only
- Feature branches: `feat/<short-description>`
- Fix branches: `fix/<short-description>`

### Pull Requests

1. Create a feature branch from `dev`
2. Make your changes with tests
3. Ensure CI passes: `cargo test --lib && cargo clippy --all-targets -- -D warnings`
4. Submit a PR targeting `dev`

### PR Title Format

PR titles must follow [Conventional Commits](https://www.conventionalcommits.org/):

- `feat: add wallet creation screen`
- `fix: correct balance display formatting`
- `chore: update dependencies`
- `ci: add coverage reporting`
- `docs: update README`
- `refactor: extract sync state machine`
- `test: add view model edge cases`

### Code Style

- Run `cargo fmt` before committing (automated by pre-commit hook)
- Run `dx fmt` for RSX formatting (automated by pre-commit hook)
- All clippy warnings are errors (`-D warnings`)
- Imports go at the top of the file — never inline

### Testing

- Every public function in `state/` and `backend/` must have tests
- Use `cargo test --lib` to skip doc-test compilation
- View model tests use `#[test]`, not Dioxus rendering
- Backend tests run against the mock implementation

## Architecture Rules

See [CLAUDE.md](CLAUDE.md) for detailed architecture guidelines.
