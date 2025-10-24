# Quick Start Guide - Moonblokz Telemetry Hub

## Prerequisites

- Rust toolchain (latest stable)
- Spin CLI installed (`spin --version` should work)
- wasm32-wasip2 target: `rustup target add wasm32-wasip2`

## Installation

1. Clone the repository:
   ```bash
   cd /path/to/moonblokz-telemetry-hub
   ```

2. Build the project:
   ```bash
   cargo build --target wasm32-wasip2 --release
   ```

## Local Development

1. Create a `.env` file based on `.env.example`:
   ```bash
   cp .env.example .env
   ```

2. Edit `.env` and set your API keys (minimum 32 bytes each):
   ```
   SPIN_VARIABLE_PROBE_API_KEY=probe-secret-key-123456789012
   SPIN_VARIABLE_LOG_COLLECTOR_API_KEY=collector-secret-key-123456789
   SPIN_VARIABLE_CLI_API_KEY=cli-secret-key-12345678901234
   ```

3. Start the development server:
   ```bash
   spin build
   spin up
   ```

   The hub will be available at `http://127.0.0.1:3000`

## Testing the Endpoints

### Test the /update endpoint (probe upload)

```bash
curl -X POST http://127.0.0.1:3000/update \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: probe-secret-key-123456789012" \
  -H "X-Node-ID: 21" \
  -d '{
    "logs": [
      {
        "timestamp": "2025-10-24T12:00:00Z",
        "message": "[INFO] Test log message"
      }
    ]
  }'
```

Expected response: `[]` (empty array of commands)

### Test the /command endpoint (CLI command submission)

```bash
curl -X POST http://127.0.0.1:3000/command \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: cli-secret-key-12345678901234" \
  -d '{
    "command": "set_log_level",
    "parameters": {
      "node_id": 21,
      "log_level": "DEBUG"
    }
  }'
```

Expected response: `OK`

### Test the /download endpoint (log collector download)

```bash
curl -X GET "http://127.0.0.1:3000/download?last_log_message_id=0" \
  -H "X-Api-Key: collector-secret-key-123456789"
```

Expected response: JSON object with logs array

## Deployment

### Deploy to Fermyon Cloud

1. Login to Fermyon Cloud:
   ```bash
   spin login
   ```

2. Deploy the application:
   ```bash
   spin deploy
   ```

3. Set the required variables:
   ```bash
   spin cloud variables set probe_api_key="your-secure-probe-key" \
                          log_collector_api_key="your-secure-collector-key" \
                          cli_api_key="your-secure-cli-key"
   ```

### Deploy to Other Spin Platforms

Follow your platform's deployment instructions for Spin applications. Ensure you set the required environment variables.

## Troubleshooting

### Build fails with "target not found"

Install the wasm32-wasip2 target:
```bash
rustup target add wasm32-wasip2
```

### "Unauthorized" responses

- Check that your API keys match between `.env` and your requests
- Ensure you're using the correct header names (`X-Api-Key`, `X-Node-ID`)

### Database errors

The SQLite database is automatically created on first use. If you encounter persistent errors, try deleting the `.spin` directory and restarting.

## Next Steps

- Review the [specification](moonblokz_test_infrastructure_full_spec.md) for complete system details
- Set up the MoonBlokz Probe to send telemetry
- Configure the Log Collector to retrieve logs
- Use the Telemetry CLI to send commands to probes
