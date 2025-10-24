#!/bin/bash

# Test script for Moonblokz Telemetry Hub
# This script tests all three main endpoints

set -e

BASE_URL="${BASE_URL:-http://127.0.0.1:3000}"
PROBE_KEY="${PROBE_KEY:-probe-secret-key-123456789012}"
COLLECTOR_KEY="${COLLECTOR_KEY:-collector-secret-key-123456789}"
CLI_KEY="${CLI_KEY:-cli-secret-key-12345678901234}"
NODE_ID=21

echo "=== Testing Moonblokz Telemetry Hub ==="
echo "Base URL: $BASE_URL"
echo ""

# Test 1: Upload logs from probe
echo "Test 1: Upload logs via /update endpoint"
UPLOAD_RESPONSE=$(curl -s -X POST "$BASE_URL/update" \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: $PROBE_KEY" \
  -H "X-Node-ID: $NODE_ID" \
  -d '{
    "logs": [
      {
        "timestamp": "2025-10-24T12:00:00Z",
        "message": "[INFO] Test log message 1"
      },
      {
        "timestamp": "2025-10-24T12:00:05Z",
        "message": "[DEBUG] Test log message 2"
      }
    ]
  }')

echo "Response: $UPLOAD_RESPONSE"
echo ""

# Test 2: Submit a command via CLI
echo "Test 2: Submit command via /command endpoint"
COMMAND_RESPONSE=$(curl -s -X POST "$BASE_URL/command" \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: $CLI_KEY" \
  -d '{
    "command": "set_log_level",
    "parameters": {
      "node_id": '$NODE_ID',
      "log_level": "DEBUG"
    }
  }')

echo "Response: $COMMAND_RESPONSE"
echo ""

# Test 3: Upload again to retrieve the command
echo "Test 3: Upload empty batch to retrieve pending commands"
UPLOAD_RESPONSE2=$(curl -s -X POST "$BASE_URL/update" \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: $PROBE_KEY" \
  -H "X-Node-ID: $NODE_ID" \
  -d '{
    "logs": []
  }')

echo "Response: $UPLOAD_RESPONSE2"
echo ""

# Test 4: Download logs
echo "Test 4: Download logs via /download endpoint"
# Note: This may return empty logs if they're too recent (within max_upload_interval)
DOWNLOAD_RESPONSE=$(curl -s -X GET "$BASE_URL/download?last_log_message_id=0" \
  -H "X-Api-Key: $COLLECTOR_KEY")

echo "Response: $DOWNLOAD_RESPONSE"
echo ""

# Test 5: Test authentication failure
echo "Test 5: Test authentication failure (should return 401)"
AUTH_FAILURE=$(curl -s -w "\nHTTP Status: %{http_code}" -X POST "$BASE_URL/update" \
  -H "Content-Type: application/json" \
  -H "X-Api-Key: wrong-key" \
  -H "X-Node-ID: $NODE_ID" \
  -d '{
    "logs": []
  }')

echo "$AUTH_FAILURE"
echo ""

echo "=== Tests completed ==="
