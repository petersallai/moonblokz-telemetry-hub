# Moonblokz Telemetry Hub - AI Coding Instructions

## Project Overview

This is a **Spin WASM/WASI** cloud service for collecting telemetry from MoonBlokz Test Stations (probes). It's a single-component HTTP service that handles probe telemetry uploads, command distribution, and log downloads for collectors.

**Key Architecture Points:**
- Single Rust lib (`src/lib.rs`) compiled to `wasm32-wasip2` target
- Three HTTP endpoints: `/update` (probes), `/download` (collectors), `/command` (CLI)
- SQLite for persistent data (`log_messages` and `commands` tables)
- Spin KV store for transient state (`last_cleanup_time`, `max_upload_interval`)
- No networking - pure request/response model

## Critical Build & Run Commands

```bash
# Build (always use --target flag)
cargo build --target wasm32-wasip2 --release

# Or use Spin's build (calls cargo under the hood)
spin build

# Run locally (requires .env with API keys)
spin up

# Test all endpoints
./test_hub.sh
```

**Never** run `cargo build` without `--target wasm32-wasip2` - it will fail in Spin runtime.

## Configuration Architecture

Configuration uses Spin's variable system defined in `spin.toml`:

```toml
# spin.toml defines variables that get passed to the component
[variables]
probe_api_key = { required = true }  # Set via .env or Spin Cloud UI

[component.moonblokz-telemetry-hub.variables]
probe_api_key = "{{ probe_api_key }}"  # Template interpolation
```

Access in Rust code: `variables::get("probe_api_key")?`

**Environment Variables for Local Dev:**
Create `.env` with `SPIN_VARIABLE_<NAME>` format (see `.env.example`):
```bash
SPIN_VARIABLE_PROBE_API_KEY=probe-secret-key-123456789012
SPIN_VARIABLE_CLI_API_KEY=cli-secret-key-12345678901234
```

## Code Patterns & Conventions

### HTTP Handler Structure
All endpoints follow this pattern:
1. API key validation via `X-Api-Key` header (returns 401 if invalid)
2. Parse request (headers for `/update`, query params for `/download`, body for `/command`)
3. Open database: `Connection::open_default()?` and `init_database(&conn)?`
4. Perform data operations
5. Check if cleanup needed via KV store
6. Return JSON response

Example from `/update` handler:
```rust
fn handle_update(req: Request) -> Result<Response> {
    let probe_api_key = variables::get("probe_api_key")?;
    // ... validate, parse, process
    let conn = Connection::open_default()?;
    init_database(&conn)?;
    // ... insert logs, get commands
    if should_cleanup(&store, cleanup_interval)? {
        cleanup_old_data(&conn, delete_timeout)?;
    }
    Ok(Response::builder().status(200).body(json).build())
}
```

### Database Access Patterns
- Always call `init_database()` after opening connection (idempotent CREATE TABLE IF NOT EXISTS)
- Use parameterized queries with `Value::Text()`, `Value::Integer()` enum
- Cleanup operations limit to 10,000 rows at a time to prevent timeouts
- Index exists on `log_messages.timestamp` for efficient filtering

### Key-Value Store Usage
Used for **coordination state only**, not persistent data:
- `last_cleanup_time` - Track when to run next cleanup (prevents every request triggering cleanup)
- `update_interval_config` - JSON with `start_time`, `end_time` (Unix timestamps), `active_period`, `inactive_period`

Pattern: `Store::open_default()?, store.get("key"), store.set("key", bytes)`

### Update Interval Logic
The `set_update_interval` command is handled specially - stored in KV, NOT forwarded to nodes:
```rust
// Stored as UpdateIntervalConfig in KV
let config = UpdateIntervalConfig {
    start_time: start_time.timestamp() as u64,  // Unix timestamp
    end_time: end_time.timestamp() as u64,
    active_period,    // seconds
    inactive_period,  // seconds
};
save_update_interval_config(&store, &config)?;
```

Probes receive `update_interval` in their `/update` response based on whether current time is within the active window.

## Testing Strategy

**Local Testing:** Use `test_hub.sh` which tests all three endpoints:
```bash
# Override defaults with environment variables
BASE_URL=http://localhost:3000 PROBE_KEY=mykey ./test_hub.sh
```

**Manual cURL Examples:**
```bash
# Upload logs (probe)
curl -X POST http://127.0.0.1:3000/update \
  -H "X-Api-Key: probe-key" \
  -H "X-Node-ID: 21" \
  -d '{"logs":[{"timestamp":"2025-01-02T10:00:00Z","message":"[INFO] Test"}]}'

# Submit command (CLI)
curl -X POST http://127.0.0.1:3000/command \
  -H "X-Api-Key: cli-key" \
  -d '{"command":"set_log_level","parameters":{"node_id":21,"log_level":"DEBUG"}}'

# Download logs (collector)
curl http://127.0.0.1:3000/download?last_log_message_id=0 \
  -H "X-Api-Key: collector-key"
```

## Common Pitfalls

1. **Forgetting wasm32-wasip2 target** - Build will succeed but runtime will fail
2. **API key validation** - Three separate keys (probe, collector, CLI) - don't mix them
3. **Node ID extraction** - `/update` uses header (`X-Node-ID`), `/command` uses body parameter
4. **Cleanup timing** - Uses 1.1x safety margin on `max_upload_interval` for log downloads
5. **Command deletion** - Commands are auto-deleted after retrieval by probe (one-time delivery)
6. **Logger initialization** - Happens on every request (stateless WASM environment)

## Project Structure Significance

- `spin.toml` - **Single source of truth** for configuration, routes, and build commands
- `src/lib.rs` - All code in one file (appropriate for single-component service)
- `API.md` - Complete endpoint specifications with examples
- `IMPLEMENTATION.md` - Implementation checklist (useful for understanding feature completeness)
- `moonblokz_test_infrastructure_full_spec.md` - Original requirements document
- `Makefile` - Just runs `spin build` (kept for convenience)

## When Adding Features

1. Update data models (structs with `Serialize/Deserialize`)
2. Add/modify database schema in `init_database()`
3. Create/update handler function following existing pattern
4. Add route match in `handle_request()` main router
5. Update `API.md` with new endpoint/parameters
6. Add test case to `test_hub.sh`
7. Consider impact on cleanup logic (retention policies)
