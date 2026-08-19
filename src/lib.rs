use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use spin_sdk::{
    http::{IntoResponse, Request, Response},
    http_component,
    key_value::Store,
    sqlite::{Connection, Value},
    variables,
};

const DEFAULT_CLEANUP_INTERVAL_MINUTES: i64 = 5;
const DEFAULT_DELETE_TIMEOUT_MINUTES: i64 = 30;
const DEFAULT_UPLOAD_INTERVAL_SECONDS: i64 = 300;
const MAX_LOG_ITEMS_PER_DOWNLOAD: i64 = 10000;

// ============================================================================
// Data Models
// ============================================================================

#[derive(Debug, Deserialize, Serialize)]
struct LogEntry {
    timestamp: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ProbeUploadRequest {
    logs: Vec<LogEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Command {
    command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct DownloadLogEntry {
    item_id: i64,
    timestamp: String,
    node_id: i64,
    message: String,
}

#[derive(Debug, Serialize)]
struct DownloadResponse {
    logs: Vec<DownloadLogEntry>,
    update_interval: i64,
}

#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct UpdateResponse {
    commands: Vec<Command>,
    update_interval: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdateIntervalConfig {
    start_time: u64,
    end_time: u64,
    active_period: i64,
    inactive_period: i64,
}

// ============================================================================
// Database Operations
// ============================================================================

fn init_database(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS log_messages (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            node_id INTEGER NOT NULL,
            message TEXT NOT NULL
        )",
        &[],
    )?;

    // Create index on timestamp for efficient sorting and filtering
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_log_messages_timestamp ON log_messages(timestamp)",
        &[],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS commands (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            node_id INTEGER NOT NULL,
            command TEXT NOT NULL
        )",
        &[],
    )?;

    Ok(())
}

fn insert_log_messages(conn: &Connection, node_id: u32, logs: &[LogEntry]) -> Result<()> {
    for log in logs {
        log::trace!(
            "Inserting log message for node_id {}: {}",
            node_id,
            log.message
        );
        _ = conn.execute(
            "INSERT INTO log_messages (timestamp, node_id, message) VALUES (?, ?, ?)",
            &[
                Value::Text(log.timestamp.clone()),
                Value::Integer(node_id as i64),
                Value::Text(log.message.clone()),
            ],
        )?;
    }
    Ok(())
}

fn get_and_delete_commands(conn: &Connection, node_id: u32) -> Result<Vec<Command>> {
    let result = conn.execute(
        "SELECT id, command FROM commands WHERE node_id = ? ORDER BY id",
        &[Value::Integer(node_id as i64)],
    )?;
    let mut commands = Vec::new();
    for row in result.rows() {
        if let Some(command_json) = row.get::<&str>("command") {
            if let Ok(cmd) = serde_json::from_str::<Command>(command_json) {
                commands.push(cmd);
            }
        }
    }

    // Delete the commands
    conn.execute(
        "DELETE FROM commands WHERE node_id = ?",
        &[Value::Integer(node_id as i64)],
    )?;

    Ok(commands)
}

fn cleanup_old_data(conn: &Connection, delete_timeout_minutes: i64) -> Result<()> {
    log::debug!(
        "Cleaning up old data older than {} minutes.",
        delete_timeout_minutes
    );
    let cutoff_time = Utc::now() - chrono::Duration::minutes(delete_timeout_minutes);
    let cutoff_str = cutoff_time.to_rfc3339();

    conn.execute(
        "DELETE FROM log_messages WHERE id IN (SELECT id FROM log_messages WHERE timestamp < ? LIMIT 10000)",
        &[Value::Text(cutoff_str.clone())],
    )?;

    // Count remaining log messages
    let log_count_result = conn.execute("SELECT COUNT(*) as count FROM log_messages", &[])?;
    if let Some(row) = log_count_result.rows().next() {
        if let Some(count) = row.get::<i64>("count") {
            if count > 0 {
                log::debug!("Remaining log messages after cleanup: {}", count);
            }
        }
    }

    conn.execute(
        "DELETE FROM commands WHERE id IN (SELECT id FROM commands WHERE timestamp < ? LIMIT 10000)",
        &[Value::Text(cutoff_str)],
    )?;

    // Count remaining commands
    let cmd_count_result = conn.execute("SELECT COUNT(*) as count FROM commands", &[])?;
    if let Some(row) = cmd_count_result.rows().next() {
        if let Some(count) = row.get::<i64>("count") {
            if count > 0 {
                log::debug!("Remaining commands after cleanup: {}", count);
            }
        }
    }

    Ok(())
}

fn get_logs_for_download(
    conn: &Connection,
    last_timestamp: DateTime<Utc>,
    max_upload_interval: i64,
) -> Result<Vec<DownloadLogEntry>> {
    let cutoff_time =
        Utc::now() - chrono::Duration::seconds((max_upload_interval as f64 * 1.1) as i64);
    let cutoff_str = cutoff_time.to_rfc3339();
    let last_timestamp_str = last_timestamp.to_rfc3339();

    log::debug!(
        "Fetching logs for download: last_timestamp={}, cutoff_time={}, current_time={}",
        last_timestamp_str,
        cutoff_str,
        Utc::now().to_rfc3339()
    );

    let result = conn.execute(
        "SELECT id, timestamp, node_id, message FROM log_messages 
         WHERE timestamp > ? AND timestamp < ?
         ORDER BY timestamp ASC, id ASC LIMIT ?",
        &[
            Value::Text(last_timestamp_str),
            Value::Text(cutoff_str),
            Value::Integer(MAX_LOG_ITEMS_PER_DOWNLOAD),
        ],
    )?;

    log::debug!("Fetched {} logs for download.", result.rows().count());

    let mut logs = Vec::new();
    for row in result.rows() {
        let id = row.get::<i64>("id");
        let timestamp = row.get::<&str>("timestamp");
        let node_id = row.get::<i64>("node_id");
        let message = row.get::<&str>("message");

        if let (Some(id), Some(timestamp), Some(node_id), Some(message)) =
            (id, timestamp, node_id, message)
        {
            logs.push(DownloadLogEntry {
                item_id: id,
                timestamp: timestamp.to_string(),
                node_id,
                message: message.to_string(),
            });
        }
    }

    Ok(logs)
}

fn insert_command(conn: &Connection, node_id: i64, command_json: &str) -> Result<()> {
    let timestamp = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO commands (timestamp, node_id, command) VALUES (?, ?, ?)",
        &[
            Value::Text(timestamp),
            Value::Integer(node_id),
            Value::Text(command_json.to_string()),
        ],
    )?;
    Ok(())
}

fn get_all_node_ids(conn: &Connection) -> Result<Vec<i64>> {
    let result = conn.execute(
        "SELECT DISTINCT node_id FROM log_messages ORDER BY node_id",
        &[],
    )?;

    let mut node_ids = Vec::new();
    for row in result.rows() {
        if let Some(node_id) = row.get::<i64>("node_id") {
            node_ids.push(node_id);
        }
    }

    Ok(node_ids)
}

// ============================================================================
// Key-Value Store Operations
// ============================================================================

fn should_cleanup(store: &Store, cleanup_interval_minutes: i64) -> Result<bool> {
    match store.get("last_cleanup_time") {
        Ok(Some(bytes)) => {
            let last_cleanup_str = String::from_utf8(bytes)?;
            let last_cleanup: DateTime<Utc> = last_cleanup_str.parse()?;
            let now = Utc::now();
            Ok((now - last_cleanup).num_minutes() >= cleanup_interval_minutes)
        }
        _ => Ok(true),
    }
}

fn update_last_cleanup_time(store: &Store) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    store.set("last_cleanup_time", now.as_bytes())?;
    Ok(())
}

fn save_update_interval_config(store: &Store, config: &UpdateIntervalConfig) -> Result<()> {
    let json = serde_json::to_string(config)?;
    store.set("update_interval", json.as_bytes())?;
    Ok(())
}

fn get_update_interval_config(store: &Store) -> Option<UpdateIntervalConfig> {
    store
        .get("update_interval")
        .ok()
        .and_then(|opt| opt)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
}

fn get_current_update_interval(store: &Store, default_interval: i64) -> i64 {
    match get_update_interval_config(store) {
        Some(config) => {
            let now = Utc::now().timestamp() as u64;
            if now >= config.start_time && now <= config.end_time {
                log::debug!(
                    "In active period, using active_period: {}",
                    config.active_period
                );
                config.active_period
            } else {
                log::debug!(
                    "In inactive period, using inactive_period: {}",
                    config.inactive_period
                );
                config.inactive_period
            }
        }
        None => {
            log::debug!(
                "No update interval config found, using default: {}",
                default_interval
            );
            default_interval
        }
    }
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err(anyhow!("Invalid percent-encoding in query parameter"));
                }
                let hex = &value[i + 1..i + 3];
                let decoded = u8::from_str_radix(hex, 16)
                    .map_err(|_| anyhow!("Invalid percent-encoding in query parameter"))?;
                out.push(decoded as char);
                i += 3;
            }
            b'+' => {
                out.push(' ');
                i += 1;
            }
            _ => {
                out.push(bytes[i] as char);
                i += 1;
            }
        }
    }
    Ok(out)
}

fn get_query_param(uri: &str, key: &str) -> Result<Option<String>> {
    let query = uri.splitn(2, '?').nth(1).unwrap_or("");
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let mut parts = pair.splitn(2, '=');
        let k = parts.next().unwrap_or("");
        let v = parts.next().unwrap_or("");
        if k == key {
            return Ok(Some(percent_decode(v)?));
        }
    }
    Ok(None)
}

// ============================================================================
// HTTP Handlers
// ============================================================================

fn handle_update(req: Request) -> Result<Response> {
    // Validate probe API key
    let probe_api_key = variables::get("probe_api_key")?;
    let api_key_header = req
        .header("x-api-key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing X-Api-Key header"))?;

    if api_key_header != probe_api_key {
        return Ok(Response::builder().status(401).body("Unauthorized").build());
    }

    // Get node ID
    let node_id_str = req
        .header("x-node-id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing X-Node-ID header"))?;
    let node_id: u32 = node_id_str
        .parse()
        .map_err(|_| anyhow!("Invalid node ID"))?;

    // Parse request body
    let body = req.body();
    let upload_req: ProbeUploadRequest = serde_json::from_slice(body)?;

    log::debug!(
        "Received upload request. Node_id: {}, uploaded logline count: {}",
        node_id,
        upload_req.logs.len()
    );

    // Open database and initialize
    let conn = Connection::open_default()?;
    init_database(&conn)?;

    // Insert log messages
    insert_log_messages(&conn, node_id, &upload_req.logs)?;

    // Check if cleanup is needed
    let store = Store::open_default()?;
    let cleanup_interval = variables::get("cleanup_interval")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_CLEANUP_INTERVAL_MINUTES);
    let delete_timeout = variables::get("delete_timeout")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_DELETE_TIMEOUT_MINUTES);

    if should_cleanup(&store, cleanup_interval)? {
        cleanup_old_data(&conn, delete_timeout)?;
        update_last_cleanup_time(&store)?;
    }

    // Get and delete commands for this node
    let commands = get_and_delete_commands(&conn, node_id)?;

    // Get current update interval based on active/inactive period
    let default_interval = variables::get("default_upload_interval")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_UPLOAD_INTERVAL_SECONDS);
    let update_interval = get_current_update_interval(&store, default_interval);

    // Return commands and update_interval as JSON
    let response = UpdateResponse {
        commands,
        update_interval,
    };
    let response_body = serde_json::to_string(&response)?;
    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(response_body)
        .build())
}

fn handle_download(req: Request) -> Result<Response> {
    // Validate log collector API key
    let log_collector_api_key = variables::get("log_collector_api_key")?;
    let api_key_header = req
        .header("x-api-key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing X-Api-Key header"))?;

    if api_key_header != log_collector_api_key {
        return Ok(Response::builder().status(401).body("Unauthorized").build());
    }

    // Parse query parameter
    let uri = req.uri().to_string();
    let last_timestamp_str = get_query_param(&uri, "last_log_timestamp")?
        .ok_or_else(|| anyhow!("Missing last_log_timestamp parameter"))?;
    let last_timestamp: DateTime<Utc> = last_timestamp_str
        .parse()
        .map_err(|_| anyhow!("Invalid last_log_timestamp: expected ISO 8601"))?;

    // Open database
    let conn = Connection::open_default()?;
    init_database(&conn)?;

    // Get current upload interval based on active/inactive period (same logic as /update)
    let store = Store::open_default()?;
    let default_interval = variables::get("default_upload_interval")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_UPLOAD_INTERVAL_SECONDS);
    let current_upload_interval = get_current_update_interval(&store, default_interval);

    // Get logs using the current interval for filtering
    let logs = get_logs_for_download(&conn, last_timestamp, current_upload_interval)?;

    // Check if cleanup is needed
    let store = Store::open_default()?;
    let cleanup_interval = variables::get("cleanup_interval")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_CLEANUP_INTERVAL_MINUTES);
    let delete_timeout = variables::get("delete_timeout")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_DELETE_TIMEOUT_MINUTES);

    if should_cleanup(&store, cleanup_interval)? {
        cleanup_old_data(&conn, delete_timeout)?;
        update_last_cleanup_time(&store)?;
    }

    // Return logs and update_interval as JSON
    let response = DownloadResponse {
        logs,
        update_interval: current_upload_interval,
    };
    let response_body = serde_json::to_string(&response)?;
    Ok(Response::builder()
        .status(200)
        .header("content-type", "application/json")
        .body(response_body)
        .build())
}

fn handle_command(req: Request) -> Result<Response> {
    // Validate CLI API key
    let cli_api_key = variables::get("cli_api_key")?;
    let api_key_header = req
        .header("x-api-key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing X-Api-Key header"))?;

    if api_key_header != cli_api_key {
        return Ok(Response::builder().status(401).body("Unauthorized").build());
    }

    // Parse request body
    let body = req.body();
    let cmd_req: CommandRequest = serde_json::from_slice(body)?;

    // Handle set_update_interval specially - store in KV, don't forward to nodes
    if cmd_req.command == "set_update_interval" {
        if let Some(params) = &cmd_req.parameters {
            // Parse start_time and end_time as ISO 8601 strings and convert to Unix timestamps
            let start_time_str = params
                .get("start_time")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing start_time parameter"))?;
            let end_time_str = params
                .get("end_time")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("Missing end_time parameter"))?;
            let active_period = params
                .get("active_period")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow!("Missing active_period parameter"))?;
            let inactive_period = params
                .get("inactive_period")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| anyhow!("Missing inactive_period parameter"))?;

            // Parse ISO 8601 timestamps to Unix timestamps
            let start_time: DateTime<Utc> = start_time_str
                .parse()
                .map_err(|_| anyhow!("Invalid start_time format, expected ISO 8601"))?;
            let end_time: DateTime<Utc> = end_time_str
                .parse()
                .map_err(|_| anyhow!("Invalid end_time format, expected ISO 8601"))?;

            let config = UpdateIntervalConfig {
                start_time: start_time.timestamp() as u64,
                end_time: end_time.timestamp() as u64,
                active_period,
                inactive_period,
            };

            log::info!(
                "Storing update interval config: start={}, end={}, active={}, inactive={}",
                config.start_time,
                config.end_time,
                config.active_period,
                config.inactive_period
            );

            let store = Store::open_default()?;
            save_update_interval_config(&store, &config)?;

            return Ok(Response::builder().status(200).body("OK").build());
        } else {
            return Ok(Response::builder()
                .status(400)
                .body("Missing parameters for set_update_interval")
                .build());
        }
    }

    // Open database for other commands
    let conn = Connection::open_default()?;
    init_database(&conn)?;

    // Prepare command JSON
    let command = Command {
        command: cmd_req.command.clone(),
        parameters: cmd_req.parameters.clone(),
    };
    let command_json = serde_json::to_string(&command)?;
    log::info!("Inserting command: {}", command_json);
    // Check if node_id is specified in parameters
    let node_id_opt = cmd_req
        .parameters
        .as_ref()
        .and_then(|p| p.get("node_id").or_else(|| p.get("node id")))
        .and_then(|v| v.as_i64());

    if let Some(node_id) = node_id_opt {
        // Insert command for specific node
        insert_command(&conn, node_id, &command_json)?;
    } else {
        // Insert command for all nodes
        let node_ids = get_all_node_ids(&conn)?;
        for node_id in node_ids {
            insert_command(&conn, node_id, &command_json)?;
        }
    }

    Ok(Response::builder().status(200).body("OK").build())
}

// ============================================================================
// Main HTTP Component
// ============================================================================

#[http_component]
fn handle_request(req: Request) -> Result<impl IntoResponse> {
    // Get log level from configuration and initialize logger
    let loglevel = variables::get("loglevel").unwrap_or_else(|_| "info".to_string());
    let log_level = match loglevel.to_lowercase().as_str() {
        "trace" => log::LevelFilter::Trace,
        "debug" => log::LevelFilter::Debug,
        "info" => log::LevelFilter::Info,
        "warn" => log::LevelFilter::Warn,
        "error" => log::LevelFilter::Error,
        _ => log::LevelFilter::Info,
    };
    let _ = SimpleLogger::new().with_level(log_level).init();

    // Parse request URI and method
    let uri = req.uri();

    // Extract path: remove domain/scheme if present, then remove query string
    let path = uri
        .split("://")
        .last()
        .unwrap_or(uri)
        .split('/')
        .skip(1)
        .collect::<Vec<_>>()
        .join("/");
    let path = format!("/{}", path.split('?').next().unwrap_or(&path));
    let method = req.method();

    log::debug!("Received request: method={}, path={}", method, path);

    match (method, path.as_str()) {
        (&spin_sdk::http::Method::Post, "/update") => handle_update(req),
        (&spin_sdk::http::Method::Get, path) if path.starts_with("/download") => {
            handle_download(req)
        }
        (&spin_sdk::http::Method::Post, "/command") => handle_command(req),
        _ => Ok(Response::builder().status(404).body("Not Found").build()),
    }
}
