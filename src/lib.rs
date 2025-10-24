use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use spin_sdk::{
    http::{IntoResponse, Request, Response},
    http_component,
    key_value::Store,
    sqlite::{Connection, Value},
    variables,
};

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
}

#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
    parameters: Option<serde_json::Value>,
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

fn insert_log_messages(conn: &Connection, node_id: i64, logs: &[LogEntry]) -> Result<()> {
    for log in logs {
        conn.execute(
            "INSERT INTO log_messages (timestamp, node_id, message) VALUES (?, ?, ?)",
            &[
                Value::Text(log.timestamp.clone()),
                Value::Integer(node_id),
                Value::Text(log.message.clone()),
            ],
        )?;
    }
    Ok(())
}

fn get_and_delete_commands(conn: &Connection, node_id: i64) -> Result<Vec<Command>> {
    let result = conn.execute(
        "SELECT id, command FROM commands WHERE node_id = ? ORDER BY id",
        &[Value::Integer(node_id)],
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
        &[Value::Integer(node_id)],
    )?;

    Ok(commands)
}

fn cleanup_old_data(conn: &Connection, delete_timeout_minutes: i64) -> Result<()> {
    let cutoff_time = Utc::now() - chrono::Duration::minutes(delete_timeout_minutes);
    let cutoff_str = cutoff_time.to_rfc3339();

    conn.execute(
        "DELETE FROM log_messages WHERE timestamp < ?",
        &[Value::Text(cutoff_str.clone())],
    )?;

    conn.execute(
        "DELETE FROM commands WHERE timestamp < ?",
        &[Value::Text(cutoff_str)],
    )?;

    Ok(())
}

fn get_logs_for_download(
    conn: &Connection,
    last_id: i64,
    max_upload_interval: i64,
) -> Result<Vec<DownloadLogEntry>> {
    let cutoff_time = Utc::now() - chrono::Duration::seconds((max_upload_interval as f64 * 1.1) as i64);
    let cutoff_str = cutoff_time.to_rfc3339();

    let result = conn.execute(
        "SELECT id, timestamp, node_id, message FROM log_messages 
         WHERE id > ? AND timestamp < ?
         ORDER BY id ASC LIMIT ?",
        &[
            Value::Integer(last_id),
            Value::Text(cutoff_str),
            Value::Integer(MAX_LOG_ITEMS_PER_DOWNLOAD),
        ],
    )?;

    let mut logs = Vec::new();
    for row in result.rows() {
        let id = row.get::<i64>("id");
        let timestamp = row.get::<&str>("timestamp");
        let node_id = row.get::<i64>("node_id");
        let message = row.get::<&str>("message");
        
        if let (Some(id), Some(timestamp), Some(node_id), Some(message)) = (id, timestamp, node_id, message) {
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

fn should_cleanup(store: &Store, delete_timeout_minutes: i64) -> Result<bool> {
    match store.get("last_delete_time") {
        Ok(Some(bytes)) => {
            let last_delete_str = String::from_utf8(bytes)?;
            let last_delete: DateTime<Utc> = last_delete_str.parse()?;
            let now = Utc::now();
            Ok((now - last_delete).num_minutes() > delete_timeout_minutes)
        }
        _ => Ok(true),
    }
}

fn update_last_delete_time(store: &Store) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    store.set("last_delete_time", now.as_bytes())?;
    Ok(())
}

fn get_max_upload_interval(store: &Store, default: i64) -> i64 {
    store
        .get("max_upload_interval")
        .ok()
        .and_then(|opt| opt)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

fn update_max_upload_interval(store: &Store, new_interval: i64, is_global: bool) -> Result<()> {
    if is_global {
        store.set("max_upload_interval", new_interval.to_string().as_bytes())?;
    } else {
        let current = get_max_upload_interval(store, DEFAULT_UPLOAD_INTERVAL_SECONDS);
        if new_interval > current {
            store.set("max_upload_interval", new_interval.to_string().as_bytes())?;
        }
    }
    Ok(())
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
        return Ok(Response::builder()
            .status(401)
            .body("Unauthorized")
            .build());
    }

    // Get node ID
    let node_id_str = req
        .header("x-node-id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing X-Node-ID header"))?;
    let node_id: i64 = node_id_str
        .parse()
        .map_err(|_| anyhow!("Invalid node ID"))?;

    // Parse request body
    let body = req.body();
    let upload_req: ProbeUploadRequest = serde_json::from_slice(body)?;

    // Open database and initialize
    let conn = Connection::open_default()?;
    init_database(&conn)?;

    // Insert log messages
    insert_log_messages(&conn, node_id, &upload_req.logs)?;

    // Check if cleanup is needed
    let store = Store::open_default()?;
    let delete_timeout = variables::get("delete_timeout")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_DELETE_TIMEOUT_MINUTES);

    if should_cleanup(&store, delete_timeout)? {
        cleanup_old_data(&conn, delete_timeout)?;
        update_last_delete_time(&store)?;
    }

    // Get and delete commands for this node
    let commands = get_and_delete_commands(&conn, node_id)?;

    // Return commands as JSON
    let response_body = serde_json::to_string(&commands)?;
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
        return Ok(Response::builder()
            .status(401)
            .body("Unauthorized")
            .build());
    }

    // Parse query parameter
    let uri = req.uri().to_string();
    let last_id = uri
        .split("last_log_message_id=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .ok_or_else(|| anyhow!("Missing last_log_message_id parameter"))?
        .parse::<i64>()
        .map_err(|_| anyhow!("Invalid last_log_message_id"))?;

    if last_id < 0 {
        return Ok(Response::builder()
            .status(400)
            .body("Invalid last_log_message_id: must be non-negative")
            .build());
    }

    // Open database
    let conn = Connection::open_default()?;
    init_database(&conn)?;

    // Get max upload interval
    let store = Store::open_default()?;
    let default_interval = variables::get("default_upload_interval")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(DEFAULT_UPLOAD_INTERVAL_SECONDS);
    let max_upload_interval = get_max_upload_interval(&store, default_interval);

    // Get logs
    let logs = get_logs_for_download(&conn, last_id, max_upload_interval)?;

    // Return logs as JSON
    let response = DownloadResponse { logs };
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
        return Ok(Response::builder()
            .status(401)
            .body("Unauthorized")
            .build());
    }

    // Parse request body
    let body = req.body();
    let cmd_req: CommandRequest = serde_json::from_slice(body)?;

    // Open database
    let conn = Connection::open_default()?;
    init_database(&conn)?;

    // Prepare command JSON
    let command = Command {
        command: cmd_req.command.clone(),
        parameters: cmd_req.parameters.clone(),
    };
    let command_json = serde_json::to_string(&command)?;

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

    // Update max_upload_interval if needed
    if cmd_req.command == "set_update_interval" {
        if let Some(params) = &cmd_req.parameters {
            // Extract active_period and inactive_period to determine max interval
            let active_period = params.get("active_period").and_then(|v| v.as_i64());
            let inactive_period = params.get("inactive_period").and_then(|v| v.as_i64());
            
            if let (Some(active), Some(inactive)) = (active_period, inactive_period) {
                let max_interval = active.max(inactive);
                let store = Store::open_default()?;
                update_max_upload_interval(&store, max_interval, node_id_opt.is_none())?;
            }
        }
    }

    Ok(Response::builder()
        .status(200)
        .body("OK")
        .build())
}

// ============================================================================
// Main HTTP Component
// ============================================================================

#[http_component]
fn handle_request(req: Request) -> Result<impl IntoResponse> {
    let path = req.uri();
    let method = req.method();

    match (method, path) {
        (&spin_sdk::http::Method::Post, "/update") => handle_update(req),
        (&spin_sdk::http::Method::Get, path) if path.starts_with("/download") => handle_download(req),
        (&spin_sdk::http::Method::Post, "/command") => handle_command(req),
        _ => Ok(Response::builder()
            .status(404)
            .body("Not Found")
            .build()),
    }
}
