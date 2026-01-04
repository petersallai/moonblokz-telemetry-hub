# Moonblokz Telemetry Hub - API Documentation

## Overview

The Telemetry Hub provides three REST API endpoints for the MoonBlokz Test Infrastructure:

1. **POST /update** - Probe telemetry upload and command retrieval
2. **GET /download** - Log collector data download  
3. **POST /command** - CLI command submission

All endpoints require HTTPS in production and use API key authentication.

---

## Authentication

All requests must include an `X-Api-Key` header with the appropriate API key:

- Probes use `probe_api_key`
- Log collectors use `log_collector_api_key`  
- CLI clients use `cli_api_key`

**Unauthorized Request (401):**
```json
Response: "Unauthorized"
Status: 401 Unauthorized
```

---

## Endpoint: POST /update

Probes use this endpoint to upload log batches and retrieve pending commands.

### Request

**URL:** `/update`  
**Method:** `POST`  
**Content-Type:** `application/json`

**Headers:**
| Header | Type | Required | Description |
|--------|------|----------|-------------|
| X-Api-Key | string | Yes | Probe API key |
| X-Node-ID | integer | Yes | Unique node identifier |

**Request Body:**
```json
{
  "logs": [
    {
      "timestamp": "2025-10-24T12:00:00Z",
      "message": "[INFO] Node initialised"
    },
    {
      "timestamp": "2025-10-24T12:00:05Z",
      "message": "[DEBUG] Packet received"
    }
  ]
}
```

**Fields:**
- `logs` (array, required): Array of log entries
  - `timestamp` (string, required): ISO 8601 UTC timestamp
  - `message` (string, required): Log line including level prefix

### Response

**Success (200 OK):**
```json
{
  "commands": [
    {
      "command": "set_log_level",
      "parameters": {
        "log_level": "DEBUG"
      }
    },
    {
      "command": "update_node"
    }
  ],
  "update_interval": 60
}
```

Returns pending commands for this node and the current upload interval in seconds. Commands are deleted after retrieval. The `update_interval` is determined by the global `set_update_interval` configuration - if current time is within the active period, `active_period` is returned; otherwise `inactive_period` is used.

**Error Responses:**
- `400 Bad Request` - Missing headers or malformed body
- `401 Unauthorized` - Invalid API key
- `500 Internal Server Error` - Database or server error

### Example

```bash
curl -X POST https://hub.example.com/update \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: your-probe-key" \
  -H "X-Node-ID: 21" \
  -d '{
    "logs": [
      {
        "timestamp": "2025-10-24T12:00:00Z",
        "message": "[INFO] System started"
      }
    ]
  }'
```

---

## Endpoint: GET /download

Log collectors use this endpoint to download accumulated logs.

### Request

**URL:** `/download?last_log_message_id={id}`  
**Method:** `GET`

**Headers:**
| Header | Type | Required | Description |
|--------|------|----------|-------------|
| X-Api-Key | string | Yes | Log collector API key |

**Query Parameters:**
| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| last_log_message_id | integer | Yes | ID of last processed log (0 for first request) |

### Response

**Success (200 OK):**
```json
{
  "logs": [
    {
      "item_id": 42,
      "timestamp": "2025-10-24T12:00:00Z",
      "node_id": 21,
      "message": "[INFO] System started"
    },
    {
      "item_id": 43,
      "timestamp": "2025-10-24T12:00:05Z",
      "node_id": 21,
      "message": "[DEBUG] Processing data"
    }
  ]
}
```

**Fields:**
- `logs` (array): Array of log entries (may be empty)
  - `item_id` (integer): Database ID (use for next request)
  - `timestamp` (string): ISO 8601 UTC timestamp
  - `node_id` (integer): Node that generated the log
  - `message` (string): Log message text

**Notes:**
- Uses the current upload interval (active or inactive period based on configured time range) with a 1.1x safety margin to filter logs, ensuring all probes have uploaded
- Limited to 10,000 entries per request
- Empty array if no new logs available

**Error Responses:**
- `400 Bad Request` - Missing or invalid `last_log_message_id`
- `401 Unauthorized` - Invalid API key
- `500 Internal Server Error` - Database or server error

### Example

```bash
curl -X GET "https://hub.example.com/download?last_log_message_id=0" \
  -H "X-Api-Key: your-collector-key"
```

---

## Endpoint: POST /command

CLI clients use this endpoint to submit commands for probes.

### Request

**URL:** `/command`  
**Method:** `POST`  
**Content-Type:** `application/json`

**Headers:**
| Header | Type | Required | Description |
|--------|------|----------|-------------|
| X-Api-Key | string | Yes | CLI API key |

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

**Fields:**
- `command` (string, required): Command name (see below)
- `parameters` (object, optional): Command parameters
  - `node_id` (integer, optional): Target node (omit for all nodes)

### Response

**Success (200 OK):**
```
OK
```

**Error Responses:**
- `400 Bad Request` - Invalid command or parameters
- `401 Unauthorized` - Invalid API key
- `500 Internal Server Error` - Database or server error

### Supported Commands

#### set_update_interval

Configure global upload schedule based on active/inactive time periods. This command is stored centrally and NOT forwarded to nodes. Instead, probes receive the current `update_interval` value in their `/update` response.

**Note:** This command does NOT accept a `node_id` parameter - it configures a global schedule.

```json
{
  "command": "set_update_interval",
  "parameters": {
    "start_time": "2025-10-24T08:00:00Z",
    "end_time": "2025-10-24T18:00:00Z",
    "active_period": 60,
    "inactive_period": 300
  }
}
```

**Parameters:**
- `start_time` (string, required): ISO 8601 UTC timestamp for active period start
- `end_time` (string, required): ISO 8601 UTC timestamp for active period end
- `active_period` (integer, required): Upload interval in seconds during active period
- `inactive_period` (integer, required): Upload interval in seconds outside active period

**Behavior:**
- When current time is between `start_time` and `end_time`, probes receive `active_period` as their `update_interval`
- Outside this window, probes receive `inactive_period`
- The `/download` endpoint uses the same logic to filter logs appropriately
```

#### set_log_level

Change node log verbosity.

```json
{
  "command": "set_log_level",
  "parameters": {
    "node_id": 21,
    "log_level": "DEBUG"
  }
}
```

Valid levels: `TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`

#### set_filter

Update probe log filter.

```json
{
  "command": "set_filter",
  "parameters": {
    "node_id": 21,
    "value": "ERROR"
  }
}
```

#### run_command

Execute arbitrary node command.

```json
{
  "command": "run_command",
  "parameters": {
    "node_id": 21,
    "value": "/STATUS"
  }
}
```

#### update_node

Trigger node firmware update.

```json
{
  "command": "update_node",
  "parameters": {
    "node_id": 21
  }
}
```

#### update_probe

Trigger probe self-update.

```json
{
  "command": "update_probe",
  "parameters": {
    "node_id": 21
  }
}
```

#### reboot_probe

Reboot the probe's Raspberry Pi.

```json
{
  "command": "reboot_probe",
  "parameters": {
    "node_id": 21
  }
}
```

### Broadcasting Commands

To send a command to all nodes, omit the `node_id` parameter:

```json
{
  "command": "set_log_level",
  "parameters": {
    "log_level": "INFO"
  }
}
```

The hub will insert a command for each node that has uploaded logs.

### Example

```bash
curl -X POST https://hub.example.com/command \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: your-cli-key" \
  -d '{
    "command": "set_log_level",
    "parameters": {
      "node_id": 21,
      "log_level": "DEBUG"
    }
  }'
```

---

## Error Handling

### Common Error Codes

| Status Code | Description | Action |
|-------------|-------------|--------|
| 400 | Bad Request | Check request format and required fields |
| 401 | Unauthorized | Verify API key is correct |
| 404 | Not Found | Check endpoint URL |
| 500 | Internal Server Error | Retry request; contact support if persists |

### Error Response Format

Most errors return plain text error messages:

```
Status: 400 Bad Request
Body: "Missing X-Node-ID header"
```

---

## Rate Limiting

No rate limiting is currently enforced. Clients should implement their own rate limiting:

- Probes: Upload at configured interval (default 300s)
- Collectors: Poll at configured interval (default 60s)
- CLI: No automated requests expected

---

## Data Retention

- Logs and commands older than `delete_timeout` minutes are automatically deleted
- Default retention: 30 minutes
- Cleanup runs during probe upload requests
- Set `delete_timeout` variable to adjust retention period

---

## Timestamps

All timestamps must be in ISO 8601 format with UTC timezone:

**Valid formats:**
- `2025-10-24T12:00:00Z`
- `2025-10-24T12:00:00.000Z`

**Invalid formats:**
- `2025-10-24 12:00:00` (missing timezone)
- `2025-10-24T12:00:00+01:00` (local timezone)

---

## Security Best Practices

1. **Use HTTPS only** in production
2. **Keep API keys secret** - never commit to version control
3. **Use separate API keys** for each client type
4. **Rotate keys regularly** - update in Spin configuration
5. **Monitor logs** for unauthorized access attempts
6. **Use strong API keys** - minimum 32 random bytes

---

## Testing

Use the provided `test_hub.sh` script to test all endpoints:

```bash
./test_hub.sh
```

Set environment variables to customize:

```bash
export BASE_URL=https://your-hub.example.com
export PROBE_KEY=your-probe-key
export COLLECTOR_KEY=your-collector-key
export CLI_KEY=your-cli-key
./test_hub.sh
```
