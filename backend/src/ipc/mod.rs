use std::fs;
use std::io::{BufRead, Write};
use std::os::unix::fs::FileTypeExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Client-side timeout for the connect + write + read round trip.
pub const CLIENT_TIMEOUT: Duration = Duration::from_secs(3);

/// Maximum size of a single request line accepted by the listener.
const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Read timeout for a listener-side connection (handles slow/malformed peers).
const LISTENER_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum size of a response line we accept as client (defensive).
const MAX_RESPONSE_BYTES: usize = 4096;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum IpcRequest {
    /// Ask the collector to take a fresh sample of one stream and persist it.
    Sample {
        /// "memory" | "thermal" | "compute"
        stream: String,
    },
    /// Apply runtime configuration changes (intervals, retention).
    Config { changes: ConfigChanges },
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct ConfigChanges {
    pub retention_days: Option<u32>,
    pub memory_interval_secs: Option<u64>,
    pub thermal_interval_secs: Option<u64>,
    pub compute_interval_secs: Option<u64>,
    pub power_interval_secs: Option<u64>,
}

impl ConfigChanges {
    pub const INTERVAL_MIN_SECS: u64 = 1;
    pub const INTERVAL_MAX_SECS: u64 = 86_400;
    pub const RETENTION_MIN_DAYS: u32 = 1;
    pub const RETENTION_MAX_DAYS: u32 = 3_650;

    /// Validate all provided fields; Ok(()) if none is out of bounds.
    /// The whole batch must be valid — a partial reject applies nothing.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(v) = self.memory_interval_secs {
            Self::check_interval("memory_interval_secs", v)?;
        }
        if let Some(v) = self.thermal_interval_secs {
            Self::check_interval("thermal_interval_secs", v)?;
        }
        if let Some(v) = self.compute_interval_secs {
            Self::check_interval("compute_interval_secs", v)?;
        }
        if let Some(v) = self.power_interval_secs {
            Self::check_interval("power_interval_secs", v)?;
        }
        if let Some(v) = self.retention_days {
            if !(Self::RETENTION_MIN_DAYS..=Self::RETENTION_MAX_DAYS).contains(&v) {
                return Err(format!(
                    "retention_days must be between {} and {}",
                    Self::RETENTION_MIN_DAYS,
                    Self::RETENTION_MAX_DAYS
                ));
            }
        }
        Ok(())
    }

    fn check_interval(name: &str, v: u64) -> Result<(), String> {
        if !(Self::INTERVAL_MIN_SECS..=Self::INTERVAL_MAX_SECS).contains(&v) {
            return Err(format!(
                "{} must be between {} and {} seconds",
                name,
                Self::INTERVAL_MIN_SECS,
                Self::INTERVAL_MAX_SECS
            ));
        }
        Ok(())
    }
}

/// Collector → server response. `timestamp` is the unix-seconds timestamp of
/// the sample that was persisted (server verifies its DB read is fresh);
/// `skipped` reports a suppressed sample (e.g. compute guard).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

impl IpcResponse {
    pub fn ok(timestamp: i64) -> Self {
        Self {
            ok: true,
            error: None,
            skipped: false,
            timestamp: Some(timestamp),
        }
    }

    pub fn skipped(timestamp: i64) -> Self {
        Self {
            ok: true,
            error: None,
            skipped: true,
            timestamp: Some(timestamp),
        }
    }

    pub fn ack() -> Self {
        Self {
            ok: true,
            error: None,
            skipped: false,
            timestamp: None,
        }
    }

    pub fn err(e: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(e.into()),
            skipped: false,
            timestamp: None,
        }
    }
}

pub type IpcHandler = Box<dyn Fn(IpcRequest) -> IpcResponse + Send + Sync>;

fn serve_connection(stream: UnixStream, handler: &IpcHandler) {
    let _ = stream.set_read_timeout(Some(LISTENER_READ_TIMEOUT));
    let mut reader = std::io::BufReader::new(stream);
    let mut request_line = String::new();
    let n = match reader.read_line(&mut request_line) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    if n > MAX_REQUEST_BYTES {
        respond(&mut reader, &IpcResponse::err("request too large"));
        return;
    }
    let request: IpcRequest = match serde_json::from_str(request_line.trim()) {
        Ok(req) => req,
        Err(e) => {
            respond(&mut reader, &IpcResponse::err(format!("invalid json request: {}", e)));
            return;
        }
    };
    let response = handler(request);
    respond(&mut reader, &response);
}

fn respond(reader: &mut std::io::BufReader<UnixStream>, response: &IpcResponse) {
    let mut line = serde_json::to_string(response)
        .unwrap_or_else(|_| r#"{"ok":false,"error":"serialization error"}"#.to_string());
    line.push('\n');
    if let Err(e) = reader.get_mut().write_all(line.as_bytes()) {
        eprintln!("spark-ipc: failed to send response: {}", e);
    }
}

/// Bind the ipc socket and serve requests sequentially on a background
/// thread owned by that thread (which outlives every test/main run).
/// A failure to bind or spawn is returned as an error string.
pub fn spawn_listener(socket_path: &str, handler: IpcHandler) -> Result<(), String> {
    // A stale socket from a previous crashed run fails bind(); remove it if
    // it really is a socket. A busy socket (collector already running) is
    // removed and rebound: last-writer-wins, matching run.sh's cleanup model.
    if let Ok(meta) = fs::metadata(socket_path) {
        if meta.file_type().is_socket() {
            let _ = fs::remove_file(socket_path);
        }
    }
    let listener = UnixListener::bind(socket_path)
        .map_err(|e| format!("failed to bind ipc socket {}: {}", socket_path, e))?;
    let handler = Arc::new(handler);
    std::thread::Builder::new()
        .name("spark-ipc".to_string())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => serve_connection(stream, &handler),
                    Err(e) => eprintln!("spark-ipc: accept error: {}", e),
                }
            }
        })
        .map_err(|e| format!("failed to spawn ipc thread: {}", e))?;
    Ok(())
}

/// True if a live collector is accepting connections on `socket_path`. Used at
/// daemon start as a single-writer guard: on unix, connect() succeeds if and
/// only if a process is actively listening (a stale socket file — left behind by
/// a crash — has no listener and connect() fails with ECONNREFUSED/ENXIO). This
/// cleanly distinguishes "another collector is running" from "the socket file is
/// just leftover and we may take it over".
pub fn is_listener_live(socket_path: &str) -> bool {
    UnixStream::connect(socket_path).is_ok()
}

/// Connect to the collector, send one request line, read the response line.
/// A non-existent socket fails fast (connection refused); the read is bounded
/// by CLIENT_TIMEOUT.
pub fn send_request(socket_path: &str, request: &IpcRequest) -> Result<IpcResponse, String> {
    let mut stream = UnixStream::connect(socket_path)
        .map_err(|e| format!("collector unreachable at {}: {}", socket_path, e))?;
    stream
        .set_read_timeout(Some(CLIENT_TIMEOUT))
        .map_err(|e| format!("failed to set read timeout: {}", e))?;

    let mut payload = serde_json::to_string(request)
        .map_err(|e| format!("failed to serialize request: {}", e))?;
    payload.push('\n');
    stream
        .write_all(payload.as_bytes())
        .map_err(|e| format!("failed to write request: {}", e))?;

    let mut reader = std::io::BufReader::new(stream);
    let mut response_line: String = String::new();
    let n = reader
        .read_line(&mut response_line)
        .map_err(|e| format!("read error from collector: {}", e))?;
    if n == 0 {
        return Err("collector closed the connection without responding".to_string());
    }
    if n > MAX_RESPONSE_BYTES {
        return Err("collector response too large".to_string());
    }
    serde_json::from_str(response_line.trim()).map_err(|e| format!("invalid response: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn test_socket_path(name: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "spark-ipc-test-{}-{}.sock",
            std::process::id(),
            name
        ));
        p.to_string_lossy().into_owned()
    }

    fn cleanup(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_response_serialization_ok_with_timestamp() {
        let r = IpcResponse::ok(1234);
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""ok":true"#));
        assert!(s.contains(r#""timestamp":1234"#));
        assert!(!s.contains(r#""error":"#));
    }

    #[test]
    fn test_response_omits_error_and_timestamp_when_absent() {
        let r = IpcResponse::ack();
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""ok":true"#));
        assert!(!s.contains("error"));
        assert!(!s.contains("timestamp"));
    }

    #[test]
    fn test_response_serialization_error() {
        let r = IpcResponse::err("bad thing");
        let s = serde_json::to_string(&r).unwrap();
        assert!(s.contains(r#""ok":false"#));
        assert!(s.contains("bad thing"));
        assert!(!s.contains("timestamp"));
    }

    #[test]
    fn test_request_sample_roundtrip() {
        let req = IpcRequest::Sample {
            stream: "memory".to_string(),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert_eq!(s, r#"{"type":"sample","stream":"memory"}"#);
        let back: IpcRequest = serde_json::from_str(&s).unwrap();
        match back {
            IpcRequest::Sample { stream } => assert_eq!(stream, "memory"),
            _ => panic!("expected sample"),
        }
    }

    #[test]
    fn test_request_config_roundtrip() {
        let req = IpcRequest::Config {
            changes: ConfigChanges {
                retention_days: Some(3),
                memory_interval_secs: Some(2),
                thermal_interval_secs: None,
                compute_interval_secs: Some(30),
                power_interval_secs: Some(300),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.starts_with(r#"{"type":"config""#));
        let back: IpcRequest = serde_json::from_str(&s).unwrap();
        match back {
            IpcRequest::Config { changes } => {
                assert_eq!(changes.retention_days, Some(3));
                assert_eq!(changes.memory_interval_secs, Some(2));
                assert_eq!(changes.thermal_interval_secs, None);
                assert_eq!(changes.compute_interval_secs, Some(30));
                assert_eq!(changes.power_interval_secs, Some(300));
            }
            _ => panic!("expected config"),
        }
    }

    #[test]
    fn test_unknown_request_type_rejected() {
        let result: Result<IpcRequest, _> = serde_json::from_str(r#"{"type":"shutdown"}"#);
        assert!(result.is_err());
    }

    #[test]
    fn test_config_changes_validate_valid() {
        let c = ConfigChanges {
            retention_days: Some(30),
            memory_interval_secs: Some(5),
            thermal_interval_secs: Some(60),
            compute_interval_secs: Some(300),
            power_interval_secs: Some(10),
        };
        assert!(c.validate().is_ok());
    }

    #[test]
    fn test_config_changes_validate_bounds() {
        let zero_interval = ConfigChanges {
            memory_interval_secs: Some(0),
            ..Default::default()
        };
        assert!(zero_interval
            .validate()
            .err()
            .unwrap()
            .contains("memory_interval_secs"));

        let huge_interval = ConfigChanges {
            thermal_interval_secs: Some(999_999),
            ..Default::default()
        };
        assert!(huge_interval
            .validate()
            .err()
            .unwrap()
            .contains("thermal_interval_secs"));

        let zero_retention = ConfigChanges {
            retention_days: Some(0),
            ..Default::default()
        };
        assert!(zero_retention
            .validate()
            .err()
            .unwrap()
            .contains("retention_days"));

        let huge_retention = ConfigChanges {
            retention_days: Some(99_999),
            ..Default::default()
        };
        assert!(huge_retention
            .validate()
            .err()
            .unwrap()
            .contains("retention_days"));

        // Empty changes are valid (nothing to change).
        assert!(ConfigChanges::default().validate().is_ok());
    }

    #[test]
    fn test_end_to_end_sample_over_socket() {
        let path = test_socket_path("sample");
        let got_stream: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));
        let got_stream2 = Arc::clone(&got_stream);
        let requested: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
        let requested2 = Arc::clone(&requested);
        let handler: IpcHandler = Box::new(move |req| match req {
            IpcRequest::Sample { stream } => {
                *got_stream2.lock().unwrap() = Some(stream);
                requested2.store(true, Ordering::SeqCst);
                IpcResponse::ok(42)
            }
            IpcRequest::Config { .. } => IpcResponse::ack(),
        });
        let _listener = spawn_listener(&path, handler).expect("listener");

        let resp = send_request(
            &path,
            &IpcRequest::Sample {
                stream: "memory".into(),
            },
        )
        .expect("request failed");
        assert!(resp.ok);
        assert!(!resp.skipped);
        assert_eq!(resp.timestamp, Some(42));
        assert_eq!(
            got_stream.lock().unwrap().as_deref(),
            Some("memory")
        );
        assert!(requested.load(Ordering::SeqCst));

        cleanup(&path);
    }

    #[test]
    fn test_end_to_end_concurrent_clients() {
        // The sequential accept loop must serialize concurrent clients.
        let path = test_socket_path("conc");
        let handler: IpcHandler = Box::new(|req| match req {
            IpcRequest::Sample { .. } => IpcResponse::ok(1),
            IpcRequest::Config { .. } => IpcResponse::ack(),
        });
        let _listener = spawn_listener(&path, handler).expect("listener");

        let mut handles = Vec::new();
        for _ in 0..4 {
            let path = path.clone();
            handles.push(std::thread::spawn(move || {
                send_request(
                    &path,
                    &IpcRequest::Sample {
                        stream: "memory".into(),
                    },
                )
            }));
        }
        for h in handles {
            let resp = h.join().unwrap().expect("request failed");
            assert!(resp.ok);
        }

        cleanup(&path);
    }

    #[test]
    fn test_end_to_end_config_ack_over_socket() {
        let path = test_socket_path("cfg");
        let handler: IpcHandler = Box::new(|req| match req {
            IpcRequest::Config { .. } => IpcResponse::ack(),
            IpcRequest::Sample { .. } => IpcResponse::err("nope"),
        });
        let _listener = spawn_listener(&path, handler).expect("listener");

        let resp = send_request(
            &path,
            &IpcRequest::Config {
                changes: ConfigChanges {
                    retention_days: Some(30),
                    ..Default::default()
                },
            },
        )
        .expect("request failed");
        assert!(resp.ok);
        assert!(!resp.skipped);
        assert_eq!(resp.timestamp, None);

        cleanup(&path);
    }

    #[test]
    fn test_end_to_end_malformed_request() {
        let path = test_socket_path("bad");
        let handler: IpcHandler = Box::new(|_req| IpcResponse::ok(1));
        let _listener = spawn_listener(&path, handler).expect("listener");

        let mut stream = UnixStream::connect(&path).expect("connect");
        stream.write_all(b"this is not json\n").expect("write");
        stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
        let mut reader = std::io::BufReader::new(stream.try_clone().unwrap());
        let mut buf = String::new();
        let _ = reader.read_line(&mut buf);
        let resp: IpcResponse = serde_json::from_str(buf.trim()).expect("parse");
        assert!(!resp.ok);
        assert!(resp
            .error
            .as_deref()
            .unwrap()
            .contains("invalid json request"));

        cleanup(&path);
    }

    #[test]
    fn test_stale_socket_cleanup() {
        let path = test_socket_path("stale");
        // Create a stale socket file: bind then close without removing.
        let stale = UnixListener::bind(&path).expect("bind stale");
        drop(stale);
        assert!(std::path::Path::new(&path).exists());

        let handler: IpcHandler = Box::new(|_req| IpcResponse::ack());
        let _listener = spawn_listener(&path, handler).expect("listener after stale");
        let resp = send_request(
            &path,
            &IpcRequest::Sample {
                stream: "compute".into(),
            },
        )
        .expect("request");
        assert!(resp.ok);
        cleanup(&path);
    }

    #[test]
    fn test_client_refused_when_no_listener() {
        let path = test_socket_path("nolistener");
        let start = std::time::Instant::now();
        let err = send_request(
            &path,
            &IpcRequest::Sample {
                stream: "memory".into(),
            },
        );
        let elapsed = start.elapsed();
        assert!(err.is_err());
        // No listener → connection refused, fast failure.
        assert!(elapsed < Duration::from_secs(1));
    }
}
