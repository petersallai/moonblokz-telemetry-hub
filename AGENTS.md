# Repository Guidelines

## Project Structure & Module Organization
This repo contains the Telemetry Hub for the MoonBlokz test infrastructure: a Spin/WASI service that stores probe logs, serves downloads, and queues CLI commands.
- `src/lib.rs` is the single Spin component module with the HTTP handlers, SQLite access, and KV state.
- `spin.toml` defines the Spin component and variables (`probe_api_key`, `log_collector_api_key`, `cli_api_key`, `delete_timeout`, `default_upload_interval`).
- `Cargo.toml`/`Cargo.lock` define the crate and dependencies.
- Docs live in `README.md`, `QUICKSTART.md`, `API.md`, and `moonblokz_test_infrastructure_full_spec.md`.
- Ops helpers: `.env.example` for local config and `test_hub.sh` for endpoint smoke tests.

## Build, Test, and Development Commands
- `cargo build --target wasm32-wasip2 --release` builds the WASI/WASM artifact.
- `spin build` compiles the component; `make build` is a shortcut.
- `spin up` runs the hub.
- `./test_hub.sh` exercises `/update`, `/command`, and `/download` against a running hub.

## Coding Style & Naming Conventions
- Rust 2021 edition; use `rustfmt`-compatible formatting.
- Use snake_case for Rust identifiers and keep JSON fields aligned with `API.md`.
- Keep handler logic small and reuse helpers; the hub is a single-module codebase.

## Testing Guidelines
- No Rust unit/integration tests are present today.
- Prefer `./test_hub.sh` and the curl examples in `QUICKSTART.md` for validation.
- If adding tests, place unit tests in `src/lib.rs` or add a `tests/` directory and document the new command here.

## Commit & Pull Request Guidelines
- Commit messages are concise and imperative (e.g., “Add update_interval to DownloadResponse”).
- Keep commits scoped to one change; include rationale if behavior changes.
- PRs should summarize API impacts, note any Spin variable changes, and include `./test_hub.sh` or curl results.

## Architecture & Related Context
- The hub is one of four system components; the probe, log collector, and CLI are specified in `moonblokz_test_infrastructure_full_spec.md`.
- The MoonBlokz series part VII/5 (“Field Testing Infrastructure”) explains why telemetry runs over a parallel WiFi network (out-of-band control) while LoRa stays dedicated to mesh traffic.
- Test Stations combine an RP2040 LoRa node with a Raspberry Pi Zero (USB-connected). The Probe uploads logs, executes commands, and handles OTA updates; the hub coordinates uploads, downloads, and command queues.
- The hub delays log downloads to preserve timestamp ordering and manages `set_update_interval` rather than forwarding it to probes.

## Security & Configuration Tips
- Use `.env` locally and Spin variables in deployment; do not commit secrets.
- Keep API keys random (32+ bytes) and serve the hub over HTTPS in production.
