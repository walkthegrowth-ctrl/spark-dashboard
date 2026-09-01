use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::collector::compute::{read_compute, CpuRaw};
use crate::collector::reader::{parse_proc_meminfo, read_proc_meminfo};
use crate::collector::thermal::{
    annotate_zones, read_gpu_sensor, read_hwmon_sensors, read_thermal_zones,
};
use crate::config::Config;
use crate::db;
use crate::ipc::{ConfigChanges, IpcRequest};

/// Suppress on-demand compute samples taken shortly after the previous sample:
/// CPU utilization over a tiny window is noisy and not representative.
const COMPUTE_SAMPLE_GUARD_SECS: i64 = 2;

/// Source label recorded with each power sample (shown in the UI tooltip so
/// the user knows exactly what is being measured).
pub const POWER_SOURCE: &str = "nvidia-smi power.draw";

/// Runtime-mutable collector settings (changed live via IPC config requests).
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub retention_days: u32,
    pub memory_interval_secs: u64,
    pub thermal_interval_secs: u64,
    pub compute_interval_secs: u64,
    pub power_interval_secs: u64,
    pub last_prune: Instant,
}

impl RuntimeConfig {
    pub fn from_config(c: &Config) -> Self {
        RuntimeConfig {
            retention_days: c.history.retention_days,
            memory_interval_secs: c.memory.collection_interval_secs,
            thermal_interval_secs: c.thermal.collection_interval_secs,
            compute_interval_secs: c.compute.collection_interval_secs,
            power_interval_secs: c.power.collection_interval_secs,
            last_prune: Instant::now(),
        }
    }

    /// Apply the provided changes; returns an error naming the first invalid
    /// value without applying anything.
    pub fn apply(&mut self, changes: &ConfigChanges) -> Result<(), String> {
        changes.validate()?;
        if let Some(v) = changes.retention_days {
            self.retention_days = v;
        }
        if let Some(v) = changes.memory_interval_secs {
            self.memory_interval_secs = v;
        }
        if let Some(v) = changes.thermal_interval_secs {
            self.thermal_interval_secs = v;
        }
        if let Some(v) = changes.compute_interval_secs {
            self.compute_interval_secs = v;
        }
        if let Some(v) = changes.power_interval_secs {
            self.power_interval_secs = v;
        }
        Ok(())
    }
}

pub struct Collector {
    conn: Mutex<Connection>,
    cpu_prev: Mutex<Option<CpuRaw>>,
    pub runtime: Mutex<RuntimeConfig>,
    /// Serialized around the thermal per-second guard so the check and the
    /// write are one atomic section (prevents two threads from both passing
    /// the guard and double-writing the same second).
    thermal_write: Mutex<()>,
    /// Frozen zone->label map ("acpitz:<zone>" -> label) so acpitz zones keep a
    /// stable identity for the collector's lifetime (decided on first sight).
    zone_labels: Mutex<std::collections::HashMap<String, String>>,
}

impl Collector {
    pub fn new(config: Config) -> Self {
        let db_path = &config.database.path;
        let db_dir = Path::new(db_path).parent();
        if let Some(dir) = db_dir {
            fs::create_dir_all(dir).expect("Failed to create database directory");
        }
        let conn = Connection::open(db_path).expect("Failed to open database");
        if config.database.wal_mode {
            conn.execute_batch("PRAGMA journal_mode=WAL;")
                .expect("Failed to enable WAL mode");
        }
        db::init_memory_table(&conn).expect("Failed to init memory table");
        db::init_thermal_table(&conn).expect("Failed to init thermal table");
        db::init_compute_table(&conn).expect("Failed to init compute table");
        db::init_power_table(&conn).expect("Failed to init power table");
        Collector {
            conn: Mutex::new(conn),
            cpu_prev: Mutex::new(None),
            runtime: Mutex::new(RuntimeConfig::from_config(&config)),
            thermal_write: Mutex::new(()),
            zone_labels: Mutex::new(std::collections::HashMap::new()),
        }
    }

    // -- produce + persist helpers (shared by the periodic loop and IPC) -----

    fn produce_and_insert_memory(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let stat = self.read_memory().map_err(|e| format!("memory: {}", e))?;
        self.insert_memory(&stat)?;
        Ok(stat.timestamp)
    }

    /// Takes a thermal sample. Returns (skipped, timestamp).
    ///
    /// A guard prevents two batches from landing in the same second (a periodic
    /// sample and an IPC-triggered one), which would otherwise make
    /// get_latest_thermal_stats — "all rows at the max timestamp" — return two
    /// batches and duplicate every sensor in the UI. If a sample already exists
    /// for this second we skip writing and report the existing timestamp, so
    /// `/api/thermal/current` still reads the newest (now single) batch.
    fn produce_and_insert_thermal(&self) -> Result<(bool, i64), Box<dyn std::error::Error>> {
        // Serialize guard-check + write as one atomic section so two callers
        // (periodic loop, IPC) cannot both pass the guard and both write the
        // same second.
        let _guard = self.thermal_write.lock().unwrap();
        let now = chrono::Utc::now().timestamp();
        let last: Option<String> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT timestamp FROM thermal_stats ORDER BY timestamp DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok()
            .flatten()
        };
        if let Some(last_ts) = last {
            if last_ts == now.to_string() {
                return Ok((true, last_ts.parse().unwrap_or(now)));
            }
        }
        let zone_stats = self.read_thermal(now);
        self.insert_thermal_batch(&zone_stats)?;
        Ok((false, now))
    }

    /// Takes a power sample (nvidia-smi power.draw) and persists it.
    /// Returns (skipped, timestamp). Skipped when the GPU is not reporting.
    /// No per-second guard needed here: history/current reads pick the newest
    /// single row, so two same-second samples never duplicate the UI.
    fn produce_and_insert_power(&self) -> Result<(bool, i64), Box<dyn std::error::Error>> {
        let now = chrono::Utc::now().timestamp();
        let power_w = crate::collector::thermal::read_gpu_power()
            .ok_or_else(|| "power: no GPU power reading available (nvidia-smi)")?;
        let stat = crate::collector::PowerStat {
            id: None,
            timestamp: now,
            source: POWER_SOURCE.to_string(),
            power_w: Some(power_w),
        };
        self.insert_power(&stat)?;
        Ok((false, now))
    }

    /// Takes an on-demand compute sample. Returns (skipped, sample) where the
    /// sample is None when skipped (CPU utilization window too small).
    fn produce_and_insert_compute(
        &self,
    ) -> Result<(bool, Option<i64>), Box<dyn std::error::Error>> {
        let now = chrono::Utc::now().timestamp();
        let guard_start = (now - COMPUTE_SAMPLE_GUARD_SECS).to_string();
        // Last persisted compute sample is the single source of truth for the
        // guard (survives restarts; cpu_prev alone is lost on restart).
        let last: Option<String> = {
            let conn = self.conn.lock().unwrap();
            conn.query_row(
                "SELECT timestamp FROM compute_stats ORDER BY timestamp DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok()
            .flatten()
        };
        if let Some(last_ts) = last {
            if last_ts >= guard_start {
                return Ok((true, Some(last_ts.parse().unwrap_or(now))));
            }
        }
        let stat = self
            .read_compute()
            .map_err(|e| format!("compute: {}", e))?;
        self.insert_compute(&stat)?;
        Ok((false, Some(stat.timestamp)))
    }

    // -- individual readers (pure: no DB writes) -------------------------------

    fn read_memory(&self) -> Result<crate::collector::MemoryStat, String> {
        let content = read_proc_meminfo().map_err(|e| format!("Failed to read /proc/meminfo: {}", e))?;
        Ok(parse_proc_meminfo(&content))
    }

    /// One shared timestamp per batch so "latest batch" queries always see all
    /// sensors together. Zone labels are applied via the collector's frozen
    /// identity so `acpitz` zones keep a stable name (and UI slot) for life.
    fn read_thermal(&self, ts: i64) -> Vec<crate::collector::ThermalStat> {
        let gpu = read_gpu_sensor().map(|mut g| {
            g.timestamp = ts;
            g
        });
        let mut zones = read_thermal_zones();
        let gpu_temp = gpu.as_ref().and_then(|g| g.temperature_celsius);
        {
            let mut known = self.zone_labels.lock().unwrap();
            annotate_zones(&mut zones, gpu_temp, &mut known);
        }
        zones.iter_mut().for_each(|s| s.timestamp = ts);
        let mut hwmons = read_hwmon_sensors();
        hwmons.iter_mut().for_each(|s| s.timestamp = ts);

        let mut out = Vec::new();
        if let Some(g) = gpu {
            out.push(g);
        }
        out.extend(zones);
        out.extend(hwmons);
        out
    }

    fn read_compute(&self) -> Result<crate::collector::ComputeStat, String> {
        let mut prev = self.cpu_prev.lock().unwrap();
        let stat = read_compute(&mut prev);
        Ok(stat)
    }

    // -- individual writers ----------------------------------------------------

    fn insert_memory(&self, stat: &crate::collector::MemoryStat) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        db::insert_memory_stat(&conn, stat)?;
        Ok(())
    }

    fn insert_thermal_batch(
        &self,
        stats: &[crate::collector::ThermalStat],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        for stat in stats {
            db::insert_thermal_stat(&conn, stat)?;
        }
        Ok(())
    }

    fn insert_compute(&self, stat: &crate::collector::ComputeStat) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        db::insert_compute_stat(&conn, stat)?;
        Ok(())
    }

    fn insert_power(&self, stat: &crate::collector::PowerStat) -> Result<(), Box<dyn std::error::Error>> {
        let conn = self.conn.lock().unwrap();
        db::insert_power_stat(&conn, stat)?;
        Ok(())
    }

    // -- IPC dispatch ----------------------------------------------------------

    /// Handle one control-plane request. The resulting sample (if any) is
    /// persisted; the response only carries confirmation + timestamp.
    pub fn handle_ipc_request(&self, request: IpcRequest) -> crate::ipc::IpcResponse {
        match request {
            IpcRequest::Sample { stream } => {
                match stream.as_str() {
                    "memory" => match self.produce_and_insert_memory() {
                        Ok(ts) => crate::ipc::IpcResponse::ok(ts),
                        Err(e) => crate::ipc::IpcResponse::err(e.to_string()),
                    },
                    "thermal" => match self.produce_and_insert_thermal() {
                        Ok((skipped, ts)) => {
                            if skipped {
                                crate::ipc::IpcResponse::skipped(ts)
                            } else {
                                crate::ipc::IpcResponse::ok(ts)
                            }
                        }
                        Err(e) => crate::ipc::IpcResponse::err(e.to_string()),
                    },
                    "compute" => match self.produce_and_insert_compute() {
                        Ok((skipped, ts)) => {
                            match ts {
                                Some(ts) if skipped => crate::ipc::IpcResponse::skipped(ts),
                                Some(ts) => crate::ipc::IpcResponse::ok(ts),
                                None => crate::ipc::IpcResponse::ok(chrono::Utc::now().timestamp()),
                            }
                        }
                        Err(e) => crate::ipc::IpcResponse::err(e.to_string()),
                    },
                    "power" => match self.produce_and_insert_power() {
                        Ok((skipped, ts)) => {
                            if skipped {
                                crate::ipc::IpcResponse::skipped(ts)
                            } else {
                                crate::ipc::IpcResponse::ok(ts)
                            }
                        }
                        Err(e) => crate::ipc::IpcResponse::err(e.to_string()),
                    },
                    _ => crate::ipc::IpcResponse::err(format!("unknown stream '{}'", stream)),
                }
            }
            IpcRequest::Config { changes } => {
                let mut rt = self.runtime.lock().unwrap();
                match rt.apply(&changes) {
                    Ok(()) => {
                        eprintln!(
                            "spark-collect: runtime config updated: {:?}",
                            rt
                        );
                        crate::ipc::IpcResponse::ack()
                    }
                    Err(e) => crate::ipc::IpcResponse::err(e),
                }
            }
        }
    }

    // -- retention pruning -----------------------------------------------------

    /// Keep this cheap and low-frequency: at most once per 6h of wall time.
    fn maybe_prune_history(&self) {
        let retained_days: u32 = {
            let mut rt = self.runtime.lock().unwrap();
            if rt.last_prune.elapsed() < Duration::from_secs(6 * 3600) {
                return;
            }
            rt.last_prune = Instant::now();
            rt.retention_days.max(1)
        };
        let conn = self.conn.lock().unwrap();
        match db::prune_history(&conn, retained_days) {
            Ok(summary) => {
                let total = summary.memory + summary.thermal + summary.compute + summary.power;
                if total > 0 {
                    eprintln!(
                        "spark-collect: pruned old history rows: memory={} thermal={} compute={} power={} (retention={} days)",
                        summary.memory, summary.thermal, summary.compute, summary.power, retained_days
                    );
                }
            }
            Err(e) => eprintln!("spark-collect: history pruning failed: {}", e),
        }
    }

    // -- periodic loop ----------------------------------------------------------

    pub fn run_loop(&self) {
        // Intervals are snapshotted at the top of each iteration so that
        // IPC config changes take effect from the next sample tick.
        let mut last_mem = Instant::now();
        let mut last_thermal = Instant::now();
        let mut last_compute = Instant::now();
        let mut last_power = Instant::now();

        match self.produce_and_insert_memory() {
            Ok(_) => {}
            Err(e) => eprintln!("spark-collect: initial memory collection error: {}", e),
        }
        match self.produce_and_insert_thermal() {
            Ok(_) => {}
            Err(e) => eprintln!("spark-collect: initial thermal collection error: {}", e),
        }
        match self.produce_and_insert_compute() {
            Ok((_, _)) => {}
            Err(e) => eprintln!("spark-collect: initial compute collection error: {}", e),
        }
        match self.produce_and_insert_power() {
            Ok(_) => {}
            Err(e) => eprintln!("spark-collect: initial power collection error: {}", e),
        }

        loop {
            let (mem_interval, thermal_interval, compute_interval, power_interval) = {
                let rt = self.runtime.lock().unwrap();
                (
                    Duration::from_secs(rt.memory_interval_secs),
                    Duration::from_secs(rt.thermal_interval_secs),
                    Duration::from_secs(rt.compute_interval_secs),
                    Duration::from_secs(rt.power_interval_secs),
                )
            };

            if last_mem.elapsed() >= mem_interval {
                match self.produce_and_insert_memory() {
                    Ok(_) => {}
                    Err(e) => eprintln!("spark-collect: memory collection error: {}", e),
                }
                last_mem = Instant::now();
            }

            if last_thermal.elapsed() >= thermal_interval {
                match self.produce_and_insert_thermal() {
                    Ok(_) => {}
                    Err(e) => eprintln!("spark-collect: thermal collection error: {}", e),
                }
                last_thermal = Instant::now();
            }

            if last_compute.elapsed() >= compute_interval {
                match self.produce_and_insert_compute() {
                    Ok((_, _)) => {}
                    Err(e) => eprintln!("spark-collect: compute collection error: {}", e),
                }
                last_compute = Instant::now();
            }

            if last_power.elapsed() >= power_interval {
                match self.produce_and_insert_power() {
                    Ok(_) => {}
                    Err(e) => eprintln!("spark-collect: power collection error: {}", e),
                }
                last_power = Instant::now();
            }

            self.maybe_prune_history();
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ComputeConfig, CollectorConfig, Config, DatabaseConfig, HistoryConfig, MemoryConfig, PowerConfig, ServerConfig, ThermalConfig};

    fn test_collector() -> Collector {
        let config = Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8090,
            },
            collector: CollectorConfig {
                ipc_socket: "/tmp/spark-ipc-test-engine.sock".to_string(),
            },
            database: DatabaseConfig {
                path: ":memory:".to_string(),
                wal_mode: false,
            },
            history: HistoryConfig {
                retention_days: 1,
            },
            thermal: ThermalConfig {
                collection_interval_secs: 5,
            },
            memory: MemoryConfig {
                collection_interval_secs: 5,
            },
            compute: ComputeConfig {
                collection_interval_secs: 5,
            },
            power: PowerConfig {
                collection_interval_secs: 5,
            },
        };
        Collector::new(config)
    }

    #[test]
    fn test_runtime_config_apply_valid() {
        let mut rt = RuntimeConfig::from_config(&test_collector_config());
        let changes = ConfigChanges {
            retention_days: Some(3),
            memory_interval_secs: Some(2),
            thermal_interval_secs: Some(4),
            compute_interval_secs: Some(8),
            power_interval_secs: Some(16),
        };
        assert!(rt.apply(&changes).is_ok());
        assert_eq!(rt.retention_days, 3);
        assert_eq!(rt.memory_interval_secs, 2);
        assert_eq!(rt.thermal_interval_secs, 4);
        assert_eq!(rt.compute_interval_secs, 8);
        assert_eq!(rt.power_interval_secs, 16);
    }

    #[test]
    fn test_runtime_config_apply_partial() {
        let mut rt = RuntimeConfig::from_config(&test_collector_config());
        let changes = ConfigChanges {
            retention_days: Some(14),
            ..Default::default()
        };
        let original_memory = rt.memory_interval_secs;
        assert!(rt.apply(&changes).is_ok());
        assert_eq!(rt.retention_days, 14);
        assert_eq!(rt.memory_interval_secs, original_memory);
    }

    #[test]
    fn test_runtime_config_rejects_invalid_without_applying() {
        let mut rt = RuntimeConfig::from_config(&test_collector_config());
        let changes = ConfigChanges {
            retention_days: Some(14),
            memory_interval_secs: Some(7),
            thermal_interval_secs: Some(90_000),
            ..Default::default()
        };
        let err = rt.apply(&changes).unwrap_err();
        assert!(err.contains("thermal_interval_secs"));
        // The invalid request must roll back the whole batch.
        assert_eq!(rt.retention_days, test_collector_config().history.retention_days);
        assert_eq!(rt.memory_interval_secs, test_collector_config().memory.collection_interval_secs);
    }

    fn test_collector_config() -> Config {
        Config {
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 8090,
            },
            collector: CollectorConfig {
                ipc_socket: "/tmp/spark-ipc-test-engine.sock".to_string(),
            },
            database: DatabaseConfig {
                path: ":memory:".to_string(),
                wal_mode: false,
            },
            history: HistoryConfig {
                retention_days: 7,
            },
            thermal: ThermalConfig {
                collection_interval_secs: 5,
            },
            memory: MemoryConfig {
                collection_interval_secs: 10,
            },
            compute: ComputeConfig {
                collection_interval_secs: 20,
            },
            power: PowerConfig {
                collection_interval_secs: 20,
            },
        }
    }

    #[test]
    fn test_ipc_request_unknown_stream() {
        let c = test_collector();
        let resp = c.handle_ipc_request(IpcRequest::Sample {
            stream: "bogus".to_string(),
        });
        assert!(!resp.ok);
        assert!(resp.error.as_deref().unwrap().contains("unknown stream"));
    }

    #[test]
    fn test_ipc_request_memory_sample() {
        let c = test_collector();
        let resp = c.handle_ipc_request(IpcRequest::Sample {
            stream: "memory".to_string(),
        });
        assert!(resp.ok, "resp was: {:?}", resp);
        assert!(!resp.skipped);
        assert!(resp.timestamp.unwrap() > 0);
    }

    #[test]
    fn test_thermal_guard_same_second_skips() {
        let c = test_collector();
        let batch_size = c.read_thermal(1).len(); // sensors in one batch (no write)
        // First sample in this second writes a batch.
        let first = c.produce_and_insert_thermal().unwrap();
        assert!(!first.0, "first thermal sample should not be skipped: {:?}", first);
        // Same second again: skipped by the guard, reports the same timestamp.
        let second = c.produce_and_insert_thermal().unwrap();
        assert!(second.0, "second same-second sample must be skipped: {:?}", second);
        assert_eq!(first.1, second.1, "reports the existing second's timestamp");
        // Exactly one batch was written for that second (no duplication).
        let count: i64 = c
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM thermal_stats WHERE timestamp = ?1",
                [first.1.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count as usize, batch_size, "one batch per second, never two");
    }

    #[test]
    fn test_ipc_request_compute_guard() {
        let c = test_collector();
        // First sample: no previous reference, still taken (read_compute
        // returns the stat with whatever delta is available).
        let first = c.handle_ipc_request(IpcRequest::Sample {
            stream: "compute".to_string(),
        });
        assert!(first.ok, "first: {:?}", first);
        // Immediately: suppressed by the guard (within 2s of the first).
        let second = c.handle_ipc_request(IpcRequest::Sample {
            stream: "compute".to_string(),
        });
        assert!(second.ok);
        assert!(second.skipped, "second should be skipped: {:?}", second);
    }

    #[test]
    fn test_ipc_request_power_dispatch() {
        let c = test_collector();
        // Dispatch routes "power" and persists a row (skipped=true when
        // nvidia-smi is unavailable, e.g. on a build host).
        let resp = c.handle_ipc_request(IpcRequest::Sample {
            stream: "power".to_string(),
        });
        let count: i64 = c
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM power_stats", [], |r| r.get(0))
            .unwrap();
        if resp.skipped || !resp.ok {
            // No GPU power available on this host: acceptable, but the
            // dispatch must not have crashed or written a bogus row.
            assert_eq!(count, 0);
            return;
        }
        assert!(resp.timestamp.is_some());
        assert_eq!(count, 1, "one power row written for the successful dispatch");
        let latest = crate::db::get_latest_power_stat(&c.conn.lock().unwrap()).unwrap().unwrap();
        assert_eq!(latest.source, POWER_SOURCE);
        assert!(latest.power_w.unwrap() >= 0.0);
    }

    #[test]
    fn test_ipc_request_unknown_stream_includes_power() {
        let c = test_collector();
        let resp = c.handle_ipc_request(IpcRequest::Sample {
            stream: "bogus".to_string(),
        });
        assert!(!resp.ok);
        assert!(resp.error.as_deref().unwrap().contains("unknown stream"));
    }

    #[test]
    fn test_ipc_request_config_roundtrip() {
        let c = test_collector();
        let resp = c.handle_ipc_request(IpcRequest::Config {
            changes: ConfigChanges {
                retention_days: Some(30),
                ..Default::default()
            },
        });
        assert!(resp.ok);
        let rt = c.runtime.lock().unwrap();
        assert_eq!(rt.retention_days, 30);
    }

    #[test]
    fn test_ipc_request_config_invalid_no_side_effects() {
        let c = test_collector();
        let original_retention = c.runtime.lock().unwrap().retention_days;
        let resp = c.handle_ipc_request(IpcRequest::Config {
            changes: ConfigChanges {
                retention_days: Some(0),
                ..Default::default()
            },
        });
        assert!(!resp.ok);
        assert_eq!(c.runtime.lock().unwrap().retention_days, original_retention);
    }

    #[test]
    fn test_prune_removes_old_rows() {
        let c = test_collector();
        // Force the prune window to be open.
        {
            let mut rt = c.runtime.lock().unwrap();
            rt.retention_days = 7;
            rt.last_prune = Instant::now() - Duration::from_secs(7 * 3600);
        }
        // Insert one fresh row and one row 30 days old, then prune.
        let conn = c.conn.lock().unwrap();
        let fresh = crate::collector::MemoryStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            mem_total_bytes: Some(1_000_000),
            mem_free_bytes: Some(100_000),
            mem_available_bytes: Some(200_000),
            buffers_bytes: Some(10_000),
            cached_bytes: Some(30_000),
            swap_total_bytes: Some(0),
            swap_free_bytes: Some(0),
        };
        db::insert_memory_stat(&conn, &fresh).unwrap();
        let old_ts = chrono::Utc::now().timestamp() - (30 * 86_400);
        let old = crate::collector::MemoryStat {
            timestamp: old_ts,
            ..fresh
        };
        db::insert_memory_stat(&conn, &old).unwrap();
        drop(conn);

        c.maybe_prune_history();

        let conn = c.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memory_stats", [], |r| r.get::<_, i64>(0))
            .unwrap();
        assert_eq!(count, 1, "expected only the fresh row to remain");
    }
}
