use rusqlite::Connection;
use tiny_http::{Header, Response, Server, StatusCode};

use crate::collector::MemoryHistoryResponse;
use crate::db;

pub fn start_server(host: &str, port: u16, db_path: &str) {
    let server = Server::http(format!("{host}:{port}")).expect("Failed to start server");
    let conn = Connection::open(db_path).expect("Failed to open database");
    db::init_memory_table(&conn).expect("Failed to init memory table");

    for request in server.incoming_requests() {
        let url = request.url().to_string();
        let (status, body) = if url.starts_with("/api/memory/current") {
            handle_memory_current(&conn)
        } else if url.starts_with("/api/memory/history") {
            let params = parse_query_params(&url);
            handle_memory_history(&conn, &params)
        } else {
            (StatusCode(404), "Not Found".to_string())
        };
        let response = Response::from_string(body)
            .with_status_code(status)
            .with_header(Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap());
        let _ = request.respond(response);
    }
}

fn handle_memory_current(conn: &Connection) -> (StatusCode, String) {
    match db::get_latest_memory_stat(conn) {
        Ok(Some(stat)) => (StatusCode(200), serde_json::to_string(&stat).unwrap_or_else(|_| "{}".to_string())),
        Ok(None) => (StatusCode(503), "No data available".to_string()),
        Err(_) => (StatusCode(500), serde_json::json!({"error": "database error"}).to_string()),
    }
}

fn handle_memory_history(conn: &Connection, params: &std::collections::HashMap<String, String>) -> (StatusCode, String) {
    let limit: i32 = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let offset: i32 = params
        .get("offset")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if limit < 1 || limit > 1000 || offset < 0 {
        return (StatusCode(400), serde_json::json!({"error": "invalid parameters"}).to_string());
    }

    match db::get_memory_stats_paginated(conn, limit, offset) {
        Ok((data, total)) => {
            let response = MemoryHistoryResponse {
                count: data.len() as i64,
                total,
                data,
            };
            (StatusCode(200), serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string()))
        }
        Err(_) => (StatusCode(500), serde_json::json!({"error": "database error"}).to_string()),
    }
}

fn parse_query_params(url: &str) -> std::collections::HashMap<String, String> {
    let mut params = std::collections::HashMap::new();
    if let Some(query) = url.split('?').nth(1) {
        for pair in query.split('&') {
            let mut kv = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                params.insert(
                    urldecode(k),
                    urldecode(v),
                );
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
        conn
    }

    #[test]
    fn test_handle_memory_current() {
        let conn = test_db_with_data();
        let (status, _) = handle_memory_current(&conn);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_memory_current_empty_db() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_memory_table(&conn).unwrap();
        let (status, _) = handle_memory_current(&conn);
        assert_eq!(status, StatusCode(503));
    }

    #[test]
    fn test_handle_memory_history_defaults() {
        let conn = test_db_with_data();
        let params = HashMap::new();
        let (status, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_memory_history_custom_params() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "50".to_string());
        params.insert("offset".to_string(), "10".to_string());
        let (status, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(200));
    }

    #[test]
    fn test_handle_memory_history_limit_exceeds_max() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "5000".to_string());
        let (status, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_handle_memory_history_invalid_limit() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("limit".to_string(), "-1".to_string());
        let (status, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }

    #[test]
    fn test_handle_memory_history_invalid_offset() {
        let conn = test_db_with_data();
        let mut params = HashMap::new();
        params.insert("offset".to_string(), "-1".to_string());
        let (status, _) = handle_memory_history(&conn, &params);
        assert_eq!(status, StatusCode(400));
    }
}
