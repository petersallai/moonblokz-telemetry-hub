# Changelog

All notable changes to the Moonblokz Telemetry Hub will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2025-10-24

### Added
- Complete implementation of Telemetry Hub based on specification
- Three main HTTP endpoints: /update, /download, /command
- SQLite database for persistent storage of logs and commands
- Key-value store for state management (cleanup timing, max upload interval)
- Automatic data cleanup based on configurable timeout
- Support for all specified commands (set_log_level, update_node, etc.)
- API key authentication for probes, collectors, and CLI
- Command broadcasting to all nodes when node_id is omitted
- Comprehensive documentation (README, QUICKSTART, API)
- Test script for endpoint validation
- Example environment configuration

### Technical Details
- Built with Spin SDK 3.0.1 for WASI/WASM runtime
- Uses serde/serde_json for JSON serialization
- Uses chrono for timestamp handling
- Implements idiomatic Rust patterns
- Stateless design suitable for cloud deployment

## [0.1.0] - Initial Release

### Added
- Basic project structure
- Spin configuration file
- Build system setup
