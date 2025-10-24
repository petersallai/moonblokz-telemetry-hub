# Moonblokz Telemetry Hub

A central cloud service for collecting telemetry data from MoonBlokz Test Stations (probes), managing commands, and serving log data to collectors.

## Overview

The Telemetry Hub is a WASI/WASM service built with the Spin framework that acts as the central component of the MoonBlokz Test Infrastructure. It:

- Receives log batches from multiple probes via HTTPS POST requests
- Stores logs in a SQLite database with automatic cleanup of old data
- Manages and delivers commands to probes (log level changes, firmware updates, reboots, etc.)
- Serves log downloads to log collector applications
- Uses a key-value store for maintaining state information

## Architecture

The hub provides three main HTTP endpoints:

1. **POST /update** - Probes upload log batches and retrieve pending commands
2. **GET /download** - Log collectors download accumulated logs
3. **POST /command** - CLI clients submit commands to be executed by probes

## Configuration

The application is configured via `spin.toml` variables:

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `probe_api_key` | Yes | - | API key for probe authentication |
| `log_collector_api_key` | Yes | - | API key for log collector authentication |
| `cli_api_key` | Yes | - | API key for CLI authentication |
| `delete_timeout` | No | 30 | Minutes before old logs/commands are deleted |
| `default_upload_interval` | No | 300 | Default telemetry upload interval in seconds |

## Data Storage

The hub uses two storage mechanisms:

### SQLite Database

Two tables store the core data:

```sql
-- Stores all log messages from probes
CREATE TABLE log_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    node_id INTEGER NOT NULL,
    message TEXT NOT NULL
);

-- Stores pending commands for probes
CREATE TABLE commands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    node_id INTEGER NOT NULL,
    command TEXT NOT NULL
);
```

### Key-Value Store

- `last_delete_time` - Timestamp of the last cleanup operation
- `max_upload_interval` - Maximum upload interval across all probes

## API Endpoints

### POST /update

Probes upload log batches and retrieve pending commands.

**Headers:**
- `X-Api-Key`: Must match `probe_api_key`
- `X-Node-ID`: Unique identifier for the probe (integer)

**Request Body:**
```json
{
  "logs": [
    {
      "timestamp": "2025-10-23T18:00:00Z",
      "message": "[INFO] Node initialised"
    }
  ]
}
```

**Response:** JSON array of commands for the probe to execute

```json
[
  {
    "command": "set_log_level",
    "parameters": {
      "log_level": "DEBUG"
    }
  }
]
```

### GET /download

Log collectors download accumulated logs.

**Headers:**
- `X-Api-Key`: Must match `log_collector_api_key`

**Query Parameters:**
- `last_log_message_id`: Last processed log ID (use 0 for first request)

**Response:**
```json
{
  "logs": [
    {
      "item_id": 42,
      "timestamp": "2025-10-23T18:00:00Z",
      "node_id": 21,
      "message": "[INFO] Node initialised"
    }
  ]
}
```

### POST /command

CLI clients submit commands to be executed by probes.

**Headers:**
- `X-Api-Key`: Must match `cli_api_key`

**Request Body:**
```json
{
  "command": "set_log_level",
  "parameters": {
    "node_id": 21,
    "log_level": "DEBUG"
  }
}
```

**Note:** Omit `node_id` from parameters to broadcast the command to all nodes.

## Supported Commands

- `set_update_interval` - Modify probe upload schedule
- `set_log_level` - Change node log verbosity (TRACE, DEBUG, INFO, WARN, ERROR)
- `set_filter` - Update log substring filter
- `run_command` - Execute arbitrary USB command on node
- `update_node` - Trigger node firmware update
- `update_probe` - Trigger probe self-update
- `reboot_probe` - Reboot the Raspberry Pi

## Data Retention

The hub automatically cleans up old data:

- Cleanup runs during `/update` requests if `delete_timeout` minutes have elapsed
- Deletes log messages and commands older than the timeout
- Ensures the database doesn't grow unbounded

## Security

- All endpoints require API key authentication via `X-Api-Key` header
- Three separate API keys for different client types (probe, collector, CLI)
- Must be served over HTTPS (handled by Spin framework)
- API keys should be random strings with sufficient entropy (32+ bytes)

## Building

```bash
cargo build --target wasm32-wasip2 --release
```

## Running Locally

```bash
spin build
spin up
```

## Deployment

Deploy to Fermyon Cloud or any Spin-compatible platform:

```bash
spin deploy
```

Set the required variables during deployment or in your Spin configuration.

## Development

The application is written in Rust using:
- Spin SDK 3.0.1 for WASI/WASM runtime
- serde/serde_json for JSON serialization
- chrono for timestamp handling
- SQLite for persistent storage
- Key-value store for state management

## License

See LICENSE file for details.