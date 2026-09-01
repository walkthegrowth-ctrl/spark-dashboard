use std::collections::HashMap;
use std::env;
use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use rusqlite::{Connection, OpenFlags};
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use tiny_http::{Header, Request, Response, Server, StatusCode};

use crate::collector::{
    ComputeHistoryResponse, MemoryHistoryResponse, PowerHistoryResponse, ThermalHistoryResponse,
};
use crate::db;
use crate::ipc::{self, ConfigChanges, IpcRequest};

fn static_dir() -> String {
    let exe_path = env::current_exe().ok();
    let cwd = env::current_dir().ok();

    for base in [cwd, exe_path.map(|p| p.parent().unwrap().to_path_buf())] {
        if let Some(b) = base {
            let candidate = b.join("static");
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
            let candidate2 = b.join("backend").join("static");
            if candidate2.exists() {
                return candidate2.to_string_lossy().into_owned();
            }
        }
    }
    "static".to_string()
}

fn content_type(path: &str) -> &str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    match ext {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Trigger a fresh sample on the collector over IPC (best effort).
/// A successful or even skipped sample means recent data is available; a
/// connection failure means "collector down/unreachable" and the caller
/// falls back to the latest DB row (silent here — /current is never fatal).
fn trigger_sample(socket_path: &str, stream: &str) {
    let result = ipc::send_request(
        socket_path,
        &IpcRequest::Sample {
            stream: stream.to_string(),
        },
    );
    if let Ok(resp) = result {
        if !resp.ok {
            eprintln!(
                "spark-serve: collector sample trigger failed for '{}': {}",
                stream,
                resp.error.unwrap_or_else(|| "unknown error".to_string())
            );
        }
    } else if let Err(e) = result {
        eprintln!("spark-serve: collector unreachable for '{}': {} (serving latest DB data)", stream, e);
    }
}

pub fn start_server(host: &str, port: u16, db_path: &str, ipc_socket: &str) {
    let server = Server::http(format!("{host}:{port}")).expect("Failed to start server");
    let db_dir = Path::new(db_path).parent();
    if let Some(dir) = db_dir {
        fs::create_dir_all(dir).ok();
    }
    let conn = Connection::open(db_path).expect("Failed to open database");
    db::init_memory_table(&conn).expect("Failed to init memory table");
    db::init_thermal_table(&conn).expect("Failed to init thermal table");
    db::init_compute_table(&conn).expect("Failed to init compute table");
    db::init_power_table(&conn).expect("Failed to init power table");

    let shutdown = Arc::new(AtomicBool::new(false));

    let shutdown_clone = Arc::clone(&shutdown);
    let addr = format!("{host}:{port}");
    thread::spawn(move || {
        let mut signals = Signals::new([SIGINT, SIGTERM]).expect("Failed to register signal handlers");
        for _sig in signals.forever() {
            shutdown_clone.store(true, Ordering::SeqCst);
            loop {
                if shutdown_clone.load(Ordering::SeqCst) {
                    let _ = TcpStream::connect(&addr);
                    std::thread::sleep(Duration::from_millis(100));
                } else {
                    break;
                }
            }
        }
    });

    println!("spark-serve: listening on {host}:{port}");

    // Serve an unlimited number of concurrent frontend clients (local or remote
    // browsers). Each request is handled on its own worker thread with its OWN
    // read-only database connection (SQLite WAL allows many concurrent readers),
    // so requests never serialize on a shared connection — one slow or hung
    // client can no longer hold up the others.
    let db_path_owned = db_path.to_string();
    let ipc_socket_owned = ipc_socket.to_string();

    for request in server.incoming_requests() {
        if shutdown.load(Ordering::SeqCst) {
            println!("spark-serve: shutting down...");
            break;
        }
        let db_path_owned = db_path_owned.clone();
        let ipc_socket_owned = ipc_socket_owned.clone();
        thread::spawn(move || serve_request(request, &db_path_owned, &ipc_socket_owned));
    }
}

/// Serve a single HTTP request on its own thread with its own read-only
/// connection, fully independent of every in-flight request.
fn serve_request(mut request: Request, db_path: &str, ipc_socket: &str) {
    let url = request.url().to_string();
    let (status, body, ct) = if url.starts_with("/api/") {
        match Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY) {
            Ok(conn) => handle_api(&conn, &url, ipc_socket, &mut request),
            Err(e) => (
                StatusCode(500),
                json_err(format!("database read error: {}", e)),
                "application/json".to_string(),
            ),
        }
    } else if url.starts_with("/static/") || url.starts_with("/assets/") {
        let rel_path = if url.starts_with("/static/") {
            &url[8..]
        } else {
            &url[1..]
        };
        let file_path = format!("{}/{}", static_dir(), rel_path);
        if let Some(content) = fs::read_to_string(&file_path).ok() {
            (StatusCode(200), content, content_type(&file_path).to_string())
        } else {
            (StatusCode(404), "Static file not found".to_string(), "text/plain".to_string())
        }
    } else if url == "/" {
        let index_path = format!("{}/index.html", static_dir());
        if let Some(content) = fs::read_to_string(&index_path).ok() {
            (StatusCode(200), content, "text/html; charset=utf-8".to_string())
        } else {
            (
                StatusCode(503),
                "Frontend not built. Run: cd frontend && npm run build".to_string(),
                "text/plain".to_string(),
            )
        }
    } else {
        (StatusCode(404), "Not Found".to_string(), "text/plain".to_string())
    };
    let response = Response::from_string(body)
        .with_status_code(status)
        .with_header(
            Header::from_bytes(&b"Content-Type"[..], ct.as_bytes())
                .unwrap_or_else(|_| Header::from_bytes(&b"Content-Type"[..], b"text/plain").unwrap()),
        );
    let _ = request.respond(response);
}

fn handle_api(
    conn: &Connection,
    url: &str,
    ipc_socket: &str,
    request: &mut Request,
) -> (StatusCode, String, String) {
    let url_only = url.split('?').next().unwrap_or(url);
    if url_only == "/api/config" && request.method().as_str() == "POST" {
        return handle_config_post(request, ipc_socket);
    }
    if url_only.starts_with("/api/memory/current") {
        trigger_sample(ipc_socket, "memory"); // best effort; stale data on failure
        handle_memory_current(conn)
    } else if url_only.starts_with("/api/memory/history") {
        let params = parse_query_params(url);
        handle_memory_history(conn, &params)
    } else if url_only.starts_with("/api/thermal/current") {
        trigger_sample(ipc_socket, "thermal");
        handle_thermal_current(conn)
    } else if url_only.starts_with("/api/thermal/history") {
        let params = parse_query_params(url);
        handle_thermal_history(conn, &params)
    } else if url_only.starts_with("/api/compute/current") {
        trigger_sample(ipc_socket, "compute");
        handle_compute_current(conn)
    } else if url_only.starts_with("/api/compute/history") {
        let params = parse_query_params(url);
        handle_compute_history(conn, &params)
    } else if url_only.starts_with("/api/power/current") {
        trigger_sample(ipc_socket, "power");
        handle_power_current(conn)
    } else if url_only.starts_with("/api/power/history") {
        let params = parse_query_params(url);
        handle_power_history(conn, &params)
    } else {
        (StatusCode(404), "Endpoint not found".to_string(), "application/json".to_string())
    }
}

/// POST /api/config — apply runtime configuration changes to the collector.
fn handle_config_post(request: &mut Request, ipc_socket: &str) -> (StatusCode, String, String) {
    let mut body = String::new();
    if let Err(e) = read_body(request, &mut body) {
        return (
            StatusCode(400),
            json_err(format!("failed to read request body: {}", e)),
            "application/json".to_string(),
        );
    }
    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode(400),
                json_err(format!("invalid JSON body: {}", e)),
                "application/json".to_string(),
            )
        }
    };

    let retention_days = json
        .get("retention_days")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let memory_interval_secs = json
        .get("memory_interval_secs")
        .and_then(|v| v.as_u64());
    let thermal_interval_secs = json
        .get("thermal_interval_secs")
        .and_then(|v| v.as_u64());
    let compute_interval_secs = json
        .get("compute_interval_secs")
        .and_then(|v| v.as_u64());
    let power_interval_secs = json
        .get("power_interval_secs")
        .and_then(|v| v.as_u64());
    // Reject unknown/typo'd keys so silent config drift is avoided.
    let known_keys = [
        "retention_days",
        "memory_interval_secs",
        "thermal_interval_secs",
        "compute_interval_secs",
        "power_interval_secs",
    ];
    if let Some(obj) = json.as_object() {
        for key in obj.keys() {
            if !known_keys.contains(&key.as_str()) {
                return (
                    StatusCode(400),
                    json_err(format!("unknown config key '{}'", key)),
                    "application/json".to_string(),
                );
            }
        }
        if obj.is_empty() {
            return (
                StatusCode(400),
                json_err("no config changes provided"),
                "application/json".to_string(),
            );
        }
    } else if retention_days.is_none()
        && memory_interval_secs.is_none()
        && thermal_interval_secs.is_none()
        && compute_interval_secs.is_none()
        && power_interval_secs.is_none()
    {
        return (
            StatusCode(400),
            json_err("expected a JSON object of config changes"),
            "application/json".to_string(),
        );
    }

    let changes = ConfigChanges {
        retention_days,
        memory_interval_secs,
        thermal_interval_secs,
        compute_interval_secs,
        power_interval_secs,
    };
    if let Err(e) = changes.validate() {
        return (StatusCode(400), json_err(e), "application/json".to_string());
    }

    match ipc::send_request(ipc_socket, &IpcRequest::Config { changes }) {
        Ok(resp) if resp.ok => (
            StatusCode(200),
            serde_json::json!({"ok": true}).to_string(),
            "application/json".to_string(),
        ),
        Ok(resp) => (
            StatusCode(502),
            json_err(resp.error.unwrap_or_else(|| "collector rejected config change".to_string())),
            "application/json".to_string(),
        ),
        Err(e) => (
            StatusCode(503),
            json_err(format!("collector unreachable: {}", e)),
            "application/json".to_string(),
        ),
    }
}

fn read_body(request: &mut Request, buf: &mut String) -> std::io::Result<()> {
    let reader = request.as_reader();
    let mut tmp = Vec::new();
    reader.read_to_end(&mut tmp)?;
    *buf = String::from_utf8_lossy(&tmp).into_owned();
    Ok(())
}

fn json_err(msg: impl Into<String>) -> String {
    serde_json::json!({"error": msg.into()}).to_string()
}

fn handle_memory_current(conn: &Connection) -> (StatusCode, String, String) {
    match db::get_latest_memory_stat(conn) {
        Ok(Some(stat)) => (
            StatusCode(200),
            serde_json::to_string(&stat).unwrap_or_else(|_| "{}".to_string()),
            "application/json".to_string(),
        ),
        Ok(None) => (
            StatusCode(503),
            "No data available".to_string(),
            "text/plain".to_string(),
        ),
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_memory_history(conn: &Connection, params: &HashMap<String, String>) -> (StatusCode, String, String) {
    let limit: i32 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let offset: i32 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if limit < 1 || limit > 1000 || offset < 0 {
        return (
            StatusCode(400),
            serde_json::json!({"error": "invalid parameters"}).to_string(),
            "application/json".to_string(),
        );
    }

    match db::get_memory_stats_paginated(conn, limit, offset) {
        Ok((data, total)) => {
            let response = MemoryHistoryResponse {
                count: data.len() as i64,
                total,
                data,
            };
            (
                StatusCode(200),
                serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()),
                "application/json".to_string(),
            )
        }
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_thermal_current(conn: &Connection) -> (StatusCode, String, String) {
    match db::get_latest_thermal_stats(conn) {
        Ok(stats) if !stats.is_empty() => (
            StatusCode(200),
            serde_json::to_string(&stats).unwrap_or_else(|_| "[]".to_string()),
            "application/json".to_string(),
        ),
        Ok(_) => (
            StatusCode(503),
            "No data available".to_string(),
            "text/plain".to_string(),
        ),
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_thermal_history(conn: &Connection, params: &HashMap<String, String>) -> (StatusCode, String, String) {
    let limit: i32 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let offset: i32 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if limit < 1 || limit > 1000 || offset < 0 {
        return (
            StatusCode(400),
            serde_json::json!({"error": "invalid parameters"}).to_string(),
            "application/json".to_string(),
        );
    }

    match db::get_thermal_stats_paginated(conn, limit, offset) {
        Ok((data, total)) => {
            let response = ThermalHistoryResponse {
                count: data.len() as i64,
                total,
                data,
            };
            (
                StatusCode(200),
                serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()),
                "application/json".to_string(),
            )
        }
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_compute_current(conn: &Connection) -> (StatusCode, String, String) {
    match db::get_latest_compute_stat(conn) {
        Ok(Some(stat)) => {
            // Keep the flat shape the dashboard already consumes, and add:
            let mut obj = match serde_json::to_value(&stat) {
                Ok(v) => v,
                Err(_) => return (
                    StatusCode(500),
                    json_err("serialization error"),
                    "application/json".to_string(),
                ),
            };

            // CPU load averages normalized to a share of the maximum possible
            // total load (one unit of running work per core, i.e. load / cores),
            // so 100% = every core fully busy.
            let cores = stat.core_count.unwrap_or(0).max(1) as f64;
            let norm = |v: Option<f64>| v.map(|v| (v / cores * 100.0).clamp(0.0, 100.0));
            if let Some(v) = norm(stat.load_1) {
                obj["load_1_pct"] = serde_json::json!(v);
            }
            if let Some(v) = norm(stat.load_5) {
                obj["load_5_pct"] = serde_json::json!(v);
            }
            if let Some(v) = norm(stat.load_15) {
                obj["load_15_pct"] = serde_json::json!(v);
            }

            // The kernel has no 60-min load average, so the 60-min CPU figure
            // is derived from the history of 1-min loads over the last hour.
            if let Some(raw) = db::load_1_avg_last_minutes(conn, 60).ok().flatten() {
                if let Some(v) = norm(Some(raw)) {
                    obj["load_60_pct"] = serde_json::json!(v);
                }
            }

            // GPU "load" over 1/5/15/60 min: rolling averages of the
            // utilization samples stored over that period (utilization is
            // already % of full). None is reported when no samples fall in
            // the window.
            for (minutes, key) in [
                (1i64, "gpu_load_1"),
                (5, "gpu_load_5"),
                (15, "gpu_load_15"),
                (60, "gpu_load_60"),
            ] {
                if let Some(v) = db::gpu_utilization_avg_last_minutes(conn, minutes).ok().flatten() {
                    obj[key] = serde_json::json!(v);
                }
            }

            (
                StatusCode(200),
                serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string()),
                "application/json".to_string(),
            )
        }
        Ok(None) => (
            StatusCode(503),
            "No data available".to_string(),
            "text/plain".to_string(),
        ),
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_compute_history(conn: &Connection, params: &HashMap<String, String>) -> (StatusCode, String, String) {
    let limit: i32 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let offset: i32 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if limit < 1 || limit > 1000 || offset < 0 {
        return (
            StatusCode(400),
            serde_json::json!({"error": "invalid parameters"}).to_string(),
            "application/json".to_string(),
        );
    }

    match db::get_compute_stats_paginated(conn, limit, offset) {
        Ok((data, total)) => {
            let response = ComputeHistoryResponse {
                count: data.len() as i64,
                total,
                data,
            };
            (
                StatusCode(200),
                serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()),
                "application/json".to_string(),
            )
        }
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_power_current(conn: &Connection) -> (StatusCode, String, String) {
    match db::get_latest_power_stat(conn) {
        Ok(Some(stat)) => (
            StatusCode(200),
            serde_json::to_string(&stat).unwrap_or_else(|_| "{}".to_string()),
            "application/json".to_string(),
        ),
        Ok(None) => (
            StatusCode(503),
            "No data available".to_string(),
            "text/plain".to_string(),
        ),
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn handle_power_history(conn: &Connection, params: &HashMap<String, String>) -> (StatusCode, String, String) {
    let limit: i32 = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);
    let offset: i32 = params.get("offset").and_then(|v| v.parse().ok()).unwrap_or(0);

    if limit < 1 || limit > 1000 || offset < 0 {
        return (
            StatusCode(400),
            serde_json::json!({"error": "invalid parameters"}).to_string(),
            "application/json".to_string(),
        );
    }

    match db::get_power_stats_paginated(conn, limit, offset) {
        Ok((data, total)) => {
            let response = PowerHistoryResponse {
                count: data.len() as i64,
                total,
                data,
            };
            (
                StatusCode(200),
                serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()),
                "application/json".to_string(),
            )
        }
        Err(_) => (
            StatusCode(500),
            serde_json::json!({"error": "database error"}).to_string(),
            "application/json".to_string(),
        ),
    }
}

fn parse_query_params(url: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    if let Some(query) = url.split('?').nth(1) {
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(urldecode(k), urldecode(v));
            }
        }
    }
    params
}

fn urldecode(s: &str) -> String {
    s.replace("%20", " ").replace("+", " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_db_with_data() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        db::init_memory_table(&conn).unwrap();
        db::init_thermal_table(&conn).unwrap();
        db::init_compute_table(&conn).unwrap();
        db::init_power_table(&conn).unwrap();
        let stat = crate::collector::MemoryStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            mem_total_bytes: Some(16384000),
            mem_free_bytes: Some(2048000),
            mem_available_bytes: Some(8192000),
            buffers_bytes: Some(512000),
            cached_bytes: Some(4096000),
            swap_total_bytes: Some(0),
            swap_free_bytes: Some(0),
        };
        db::insert_memory_stat(&conn, &stat).unwrap();
        let thermal = crate::collector::ThermalStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            zone: "0".to_string(),
            sensor_type: "acpitz".to_string(),
            temperature_celsius: Some(65.4),
            trip_point_type: Some("critical".to_string()),
            trip_point_temp_celsius: Some(105.0),
            sensor_label: Some("Zone 0".to_string()),
        };
        db::insert_thermal_stat(&conn, &thermal).unwrap();
        let compute = crate::collector::ComputeStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            cpu_utilization_pct: Some(60.0),
            gpu_utilization_pct: Some(40.0),
            gpu_memory_utilization_pct: Some(10.0),
            load_1: Some(0.5),
            load_5: Some(0.6),
            load_15: Some(0.7),
            core_count: Some(20),
            cores: vec![],
            gpu_name: Some("NVIDIA GB10".to_string()),
        };
        db::insert_compute_stat(&conn, &compute).unwrap();
        let power = crate::collector::PowerStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            source: "nvidia-smi power.draw".to_string(),
            power_w: Some(39.0),
        };
        db::insert_power_stat(&conn, &power).unwrap();
        conn
    }

    #[test]
    fn test_handle_compute_current() {
        let conn = test_db_with_data();
        let (status, body, _) = handle_compute_current(&conn);
        assert_eq!(status, StatusCode(200));
        assert!(body.contains("cpu_utilization_pct"));
    }

    #[test]
    fn test_handle_memory_current() {
        let conn = test_db_with_data();
        let (status, _, _) = handle_memory_current(&conn);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_memory_current_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_memory_table(&conn).unwrap();
        let (status, _, _) = handle_memory_current(&conn);
        assert_eq!(status, StatusCode(503));
    }

    #[test]
    fn test_handle_memory_history_defaults() {
        let conn = test_db_with_data();
        let params = HashMap::new();
        let (status, _, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_memory_history_custom_params() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "50".to_string());
        params.insert("offset".to_string(), "10".to_string());
        let (status, _, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_memory_history_limit_exceeds_max() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "5000".to_string());
        let (status, _, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_handle_memory_history_invalid_limit() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "-1".to_string());
        let (status, _, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_handle_memory_history_invalid_offset() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("offset".to_string(), "-1".to_string());
        let (status, _, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_content_type_detection() {
        assert_eq!(content_type("page.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("style.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("app.js"), "application/javascript; charset=utf-8");
        assert_eq!(content_type("data.json"), "application/json");
        assert_eq!(content_type("image.png"), "image/png");
        assert_eq!(content_type("unknown.xyz"), "application/octet-stream");
    }

    #[test]
    fn test_handle_thermal_current() {
        let conn = test_db_with_data();
        let (status, _, _) = handle_thermal_current(&conn);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_thermal_current_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_thermal_table(&conn).unwrap();
        let (status, _, _) = handle_thermal_current(&conn);
        assert_eq!(status, StatusCode(503));
    }

    #[test]
    fn test_handle_compute_current_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_compute_table(&conn).unwrap();
        let (status, _, _) = handle_compute_current(&conn);
        assert_eq!(status, StatusCode(503));
    }

    #[test]
    fn test_handle_compute_history_defaults() {
        let conn = test_db_with_data();
        let params = HashMap::new();
        let (status, _, _) = handle_compute_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_compute_history_invalid_limit() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "-1".to_string());
        let (status, _, _) = handle_compute_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_handle_thermal_history_defaults() {
        let conn = test_db_with_data();
        let params = HashMap::new();
        let (status, _, _) = handle_thermal_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_thermal_history_custom_params() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "50".to_string());
        params.insert("offset".to_string(), "10".to_string());
        let (status, _, _) = handle_thermal_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_thermal_history_invalid_limit() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "-1".to_string());
        let (status, _, _) = handle_thermal_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_handle_power_current() {
        let conn = test_db_with_data();
        let (status, body, ct) = handle_power_current(&conn);
        assert_eq!(status, StatusCode(200));
        assert!(body.contains(r#""power_w":39"#));
        assert!(body.contains(r#""source":"nvidia-smi power.draw""#));
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn test_handle_power_current_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_power_table(&conn).unwrap();
        let (status, _, _) = handle_power_current(&conn);
        assert_eq!(status, StatusCode(503));
    }

    #[test]
    fn test_handle_compute_current_load_normalized_and_gpu_windows() {
        let conn = test_db_with_data();
        // Fixture: core_count=20, load_1=0.5, load_5=0.6, load_15=0.7,
        // gpu_utilization_pct=40.0, sample timestamp ~= now.
        let (status, body, _) = handle_compute_current(&conn);
        assert_eq!(status, StatusCode(200));
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();

        // CPU load averages as % of maximum total load (load / cores * 100).
        assert!((v["load_1_pct"].as_f64().unwrap() - 2.5).abs() < 1e-9);
        assert!((v["load_5_pct"].as_f64().unwrap() - 3.0).abs() < 1e-9);
        assert!((v["load_15_pct"].as_f64().unwrap() - 3.5).abs() < 1e-9);
        // 60-min window averages the single 1-min load (0.5 / 20 cores).
        assert!((v["load_60_pct"].as_f64().unwrap() - 2.5).abs() < 1e-9);

        // All four GPU windows contain the single sample, so all average 40.
        assert!((v["gpu_load_1"].as_f64().unwrap() - 40.0).abs() < 1e-9);
        assert!((v["gpu_load_5"].as_f64().unwrap() - 40.0).abs() < 1e-9);
        assert!((v["gpu_load_15"].as_f64().unwrap() - 40.0).abs() < 1e-9);
        assert!((v["gpu_load_60"].as_f64().unwrap() - 40.0).abs() < 1e-9);

        // The original flat fields are unchanged alongside the new ones.
        assert_eq!(v["gpu_utilization_pct"].as_f64(), Some(40.0));
        assert_eq!(v["load_1"].as_f64(), Some(0.5));
    }

    #[test]
    fn test_handle_compute_current_gpu_window_empty_when_no_recent_data() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_compute_table(&conn).unwrap();
        // One sample 2 minutes old: inside the 5/15-min windows, outside 1-min.
        let stat = crate::collector::ComputeStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp() - 120,
            cpu_utilization_pct: Some(10.0),
            gpu_utilization_pct: Some(25.0),
            gpu_memory_utilization_pct: None,
            load_1: Some(1.0),
            load_5: Some(1.0),
            load_15: Some(1.0),
            core_count: Some(20),
            cores: vec![],
            gpu_name: None,
        };
        db::insert_compute_stat(&conn, &stat).unwrap();

        let (status, body, _) = handle_compute_current(&conn);
        assert_eq!(status, StatusCode(200));
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(v["gpu_load_1"].is_null(), "2-min-old sample is outside the 1-min window");
        assert!((v["gpu_load_5"].as_f64().unwrap() - 25.0).abs() < 1e-9);
        assert!((v["gpu_load_15"].as_f64().unwrap() - 25.0).abs() < 1e-9);
        assert!((v["gpu_load_60"].as_f64().unwrap() - 25.0).abs() < 1e-9);
        // load_1 is 1.0 across the hour, so normalized it is 1/20 cores = 5%.
        assert!((v["load_60_pct"].as_f64().unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_handle_power_history_defaults() {
        let conn = test_db_with_data();
        let params = HashMap::new();
        let (status, body, _) = handle_power_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
        assert!(body.contains(r#""count":1"#));
        assert!(body.contains(r#""power_w":39"#));
    }

    #[test]
    fn test_handle_power_history_invalid_limit() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "-1".to_string());
        let (status, _, _) = handle_power_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }
}
