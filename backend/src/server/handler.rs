use std::collections::HashMap;
use std::env;
use std::fs;
use std::net::TcpStream;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use rusqlite::Connection;
use signal_hook::consts::{SIGINT, SIGTERM};
use signal_hook::iterator::Signals;
use tiny_http::{Header, Response, Server, StatusCode};

use crate::collector::{MemoryHistoryResponse, ThermalHistoryResponse};
use crate::db;

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

pub fn start_server(host: &str, port: u16, db_path: &str) {
    let server = Server::http(format!("{host}:{port}")).expect("Failed to start server");
    let db_dir = Path::new(db_path).parent();
    if let Some(dir) = db_dir {
        fs::create_dir_all(dir).ok();
    }
    let conn = Connection::open(db_path).expect("Failed to open database");
    db::init_memory_table(&conn).expect("Failed to init memory table");
    db::init_thermal_table(&conn).expect("Failed to init thermal table");

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

    for request in server.incoming_requests() {
        if shutdown.load(Ordering::SeqCst) {
            println!("spark-serve: shutting down...");
            break;
        }

        let url = request.url().to_string();
        let (status, body, ct) = if url.starts_with("/api/") {
            handle_api(&conn, &url)
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
}

fn handle_api(conn: &Connection, url: &str) -> (StatusCode, String, String) {
    if url.starts_with("/api/memory/current") {
        handle_memory_current(conn)
    } else if url.starts_with("/api/memory/history") {
        let params = parse_query_params(url);
        handle_memory_history(conn, &params)
    } else if url.starts_with("/api/thermal/current") {
        handle_thermal_current(conn)
    } else if url.starts_with("/api/thermal/history") {
        let params = parse_query_params(url);
        handle_thermal_history(conn, &params)
    } else {
        (StatusCode(404), "Endpoint not found".to_string(), "application/json".to_string())
    }
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
        conn
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
}
