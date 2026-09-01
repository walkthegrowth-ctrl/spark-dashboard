use rusqlite::{params, Connection, Result};

use crate::collector::{ComputeStat, CoreStat, MemoryStat, PowerStat, ThermalStat};

/// Delete rows older than `retention_days` from all three tables.
/// Timestamps are stored as TEXT unix-seconds (fixed 10 digits since 2001),
/// so lexicographic comparison is numerically correct.
pub struct PruneSummary {
    pub memory: u64,
    pub thermal: u64,
    pub compute: u64,
    pub power: u64,
}

pub fn prune_history(conn: &Connection, retention_days: u32) -> Result<PruneSummary> {
    let cutoff: i64 = chrono::Utc::now().timestamp() - (retention_days as i64 * 86_400);
    let cutoff_str = cutoff.to_string();
    let memory = conn
        .execute("DELETE FROM memory_stats WHERE timestamp < ?1", [cutoff_str.as_str()])?;
    let thermal = conn
        .execute("DELETE FROM thermal_stats WHERE timestamp < ?1", [cutoff_str.as_str()])?;
    let compute = conn
        .execute("DELETE FROM compute_stats WHERE timestamp < ?1", [cutoff_str.as_str()])?;
    let power = conn
        .execute("DELETE FROM power_stats WHERE timestamp < ?1", [cutoff_str.as_str()])?;
    Ok(PruneSummary {
        memory: memory as u64,
        thermal: thermal as u64,
        compute: compute as u64,
        power: power as u64,
    })
}

pub fn init_memory_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS memory_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            mem_total_bytes INTEGER,
            mem_free_bytes INTEGER,
            mem_available_bytes INTEGER,
            buffers_bytes INTEGER,
            cached_bytes INTEGER,
            swap_total_bytes INTEGER,
            swap_free_bytes INTEGER
        );
        CREATE INDEX IF NOT EXISTS idx_memory_stats_timestamp ON memory_stats(timestamp);",
    )
}

pub fn init_thermal_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS thermal_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            zone TEXT NOT NULL,
            sensor_type TEXT NOT NULL,
            temperature_celsius REAL,
            trip_point_type TEXT,
            trip_point_temp_celsius REAL,
            sensor_label TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_thermal_stats_timestamp ON thermal_stats(timestamp);
        CREATE INDEX IF NOT EXISTS idx_thermal_stats_zone ON thermal_stats(zone);",
    )
}

pub fn insert_thermal_stat(conn: &Connection, stat: &ThermalStat) -> Result<i64> {
    let ts = stat.timestamp.to_string();
    conn.execute(
        "INSERT INTO thermal_stats (timestamp, zone, sensor_type, temperature_celsius, trip_point_type, trip_point_temp_celsius, sensor_label)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![ts, stat.zone, stat.sensor_type, stat.temperature_celsius, stat.trip_point_type, stat.trip_point_temp_celsius, stat.sensor_label],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Latest thermal batch: all rows sharing the newest timestamp. Returns every
/// sensor of the most recent coherent sample. One batch is guaranteed per
/// timestamp second (the collector's per-second guard prevents two batches in
/// the same second — see engine::produce_and_insert_thermal), so this set is
/// exactly one sensor per UI slot, never duplicated.
pub fn get_latest_thermal_stats(conn: &Connection) -> Result<Vec<ThermalStat>> {
    let latest_ts: Option<String> = conn.query_row(
        "SELECT timestamp FROM thermal_stats ORDER BY timestamp DESC LIMIT 1",
        [],
        |row| row.get(0),
    ).ok();

    match latest_ts {
        Some(ts) => {
            let mut stmt = conn.prepare(
                "SELECT id, timestamp, zone, sensor_type, temperature_celsius, trip_point_type, trip_point_temp_celsius, sensor_label
                 FROM thermal_stats WHERE timestamp = ?1 ORDER BY zone",
            )?;
            let rows = stmt.query_map(params![ts], |row| {
                Ok(ThermalStat {
                    id: Some(row.get(0)?),
                    timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
                    zone: row.get(2)?,
                    sensor_type: row.get(3)?,
                    temperature_celsius: row.get(4)?,
                    trip_point_type: row.get(5)?,
                    trip_point_temp_celsius: row.get(6)?,
                    sensor_label: row.get(7)?,
                })
            })?;
            Ok(rows.collect::<Result<Vec<_>>>()?)
        }
        None => Ok(Vec::new()),
    }
}

pub fn get_thermal_stats_paginated(conn: &Connection, limit: i32, offset: i32) -> Result<(Vec<ThermalStat>, i64)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM thermal_stats",
        [],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, timestamp, zone, sensor_type, temperature_celsius, trip_point_type, trip_point_temp_celsius, sensor_label
         FROM thermal_stats ORDER BY timestamp ASC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| {
        Ok(ThermalStat {
            id: Some(row.get(0)?),
            timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
            zone: row.get(2)?,
            sensor_type: row.get(3)?,
            temperature_celsius: row.get(4)?,
            trip_point_type: row.get(5)?,
            trip_point_temp_celsius: row.get(6)?,
            sensor_label: row.get(7)?,
        })
    })?;
    let data: Vec<ThermalStat> = rows.collect::<Result<_>>()?;
    Ok((data, total))
}

pub fn init_compute_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS compute_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            cpu_utilization_pct REAL,
            gpu_utilization_pct REAL,
            gpu_memory_utilization_pct REAL,
            load_1 REAL,
            load_5 REAL,
            load_15 REAL,
            core_count INTEGER,
            cores TEXT,
            gpu_name TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_compute_stats_timestamp ON compute_stats(timestamp);",
    )
}

pub fn insert_compute_stat(conn: &Connection, stat: &ComputeStat) -> Result<i64> {
    let ts = stat.timestamp.to_string();
    let cores_json =
        serde_json::to_string(&stat.cores).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO compute_stats (timestamp, cpu_utilization_pct, gpu_utilization_pct, gpu_memory_utilization_pct, load_1, load_5, load_15, core_count, cores, gpu_name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![ts, stat.cpu_utilization_pct, stat.gpu_utilization_pct, stat.gpu_memory_utilization_pct, stat.load_1, stat.load_5, stat.load_15, stat.core_count, cores_json, stat.gpu_name],
    )?;
    Ok(conn.last_insert_rowid())
}

fn parse_cores_json(json: &str) -> Vec<CoreStat> {
    serde_json::from_str(json).unwrap_or_default()
}

fn row_to_compute(row: &rusqlite::Row) -> Result<ComputeStat> {
    Ok(ComputeStat {
        id: Some(row.get(0)?),
        timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
        cpu_utilization_pct: row.get(2)?,
        gpu_utilization_pct: row.get(3)?,
        gpu_memory_utilization_pct: row.get(4)?,
        load_1: row.get(5)?,
        load_5: row.get(6)?,
        load_15: row.get(7)?,
        core_count: row.get(8)?,
        cores: row.get::<_, Option<String>>(9)?.map_or_else(Vec::new, |s| parse_cores_json(&s)),
        gpu_name: row.get(10)?,
    })
}

pub fn get_latest_compute_stat(conn: &Connection) -> Result<Option<ComputeStat>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, cpu_utilization_pct, gpu_utilization_pct, gpu_memory_utilization_pct, load_1, load_5, load_15, core_count, cores, gpu_name
         FROM compute_stats ORDER BY timestamp DESC LIMIT 1",
    )?;
    let result: Result<Option<ComputeStat>> = stmt
        .query_map([], |row| row_to_compute(row))?
        .into_iter()
        .next()
        .transpose();
    result
}

pub fn get_compute_stats_paginated(conn: &Connection, limit: i32, offset: i32) -> Result<(Vec<ComputeStat>, i64)> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM compute_stats", [], |row| row.get(0))?;
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, cpu_utilization_pct, gpu_utilization_pct, gpu_memory_utilization_pct, load_1, load_5, load_15, core_count, cores, gpu_name
         FROM compute_stats ORDER BY timestamp ASC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| row_to_compute(row))?;
    let data: Vec<ComputeStat> = rows.collect::<Result<_>>()?;
    Ok((data, total))
}

/// Mean of the last N minutes of per-sample GPU utilization (already a % of
/// full). Used for the "GPU load 1m/5m/15m/60m" figures; None when no samples
/// exist in the window (or all are missing a value).
pub fn gpu_utilization_avg_last_minutes(conn: &Connection, minutes: i64) -> Result<Option<f64>> {
    let since = chrono::Utc::now().timestamp() - (minutes * 60);
    let since_str = since.to_string();
    let avg: Option<f64> = conn.query_row(
        "SELECT AVG(gpu_utilization_pct) FROM compute_stats
          WHERE timestamp >= ?1 AND gpu_utilization_pct IS NOT NULL",
        [since_str.as_str()],
        |row| row.get(0),
    )?;
    Ok(avg)
}

/// Mean of the last N minutes of per-sample 1-minute CPU load averages.
/// The kernel has no 60-min load average, so the 60-min figure is derived
/// from the history of 1-min loads; normalized to % of cores by the caller.
pub fn load_1_avg_last_minutes(conn: &Connection, minutes: i64) -> Result<Option<f64>> {
    let since = chrono::Utc::now().timestamp() - (minutes * 60);
    let since_str = since.to_string();
    let avg: Option<f64> = conn.query_row(
        "SELECT AVG(load_1) FROM compute_stats
          WHERE timestamp >= ?1 AND load_1 IS NOT NULL",
        [since_str.as_str()],
        |row| row.get(0),
    )?;
    Ok(avg)
}

pub fn insert_memory_stat(conn: &Connection, stat: &MemoryStat) -> Result<i64> {
    let ts = stat.timestamp.to_string();
    conn.execute(
        "INSERT INTO memory_stats (timestamp, mem_total_bytes, mem_free_bytes, mem_available_bytes, buffers_bytes, cached_bytes, swap_total_bytes, swap_free_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            ts,
            stat.mem_total_bytes,
            stat.mem_free_bytes,
            stat.mem_available_bytes,
            stat.buffers_bytes,
            stat.cached_bytes,
            stat.swap_total_bytes,
            stat.swap_free_bytes,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_latest_memory_stat(conn: &Connection) -> Result<Option<MemoryStat>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, mem_total_bytes, mem_free_bytes, mem_available_bytes, buffers_bytes, cached_bytes, swap_total_bytes, swap_free_bytes
         FROM memory_stats ORDER BY timestamp DESC LIMIT 1",
    )?;
    let rows = stmt.query_map(params![], |row| {
        Ok(MemoryStat {
            id: Some(row.get(0)?),
            timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
            mem_total_bytes: row.get(2)?,
            mem_free_bytes: row.get(3)?,
            mem_available_bytes: row.get(4)?,
            buffers_bytes: row.get(5)?,
            cached_bytes: row.get(6)?,
            swap_total_bytes: row.get(7)?,
            swap_free_bytes: row.get(8)?,
        })
    })?;
    let result: Result<Option<MemoryStat>> = rows.into_iter().next().transpose();
    result
}

pub fn get_memory_stats_paginated(conn: &Connection, limit: i32, offset: i32) -> Result<(Vec<MemoryStat>, i64)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM memory_stats",
        [],
        |row| row.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT id, timestamp, mem_total_bytes, mem_free_bytes, mem_available_bytes, buffers_bytes, cached_bytes, swap_total_bytes, swap_free_bytes
         FROM memory_stats ORDER BY timestamp ASC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| {
        Ok(MemoryStat {
            id: Some(row.get(0)?),
            timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
            mem_total_bytes: row.get(2)?,
            mem_free_bytes: row.get(3)?,
            mem_available_bytes: row.get(4)?,
            buffers_bytes: row.get(5)?,
            cached_bytes: row.get(6)?,
            swap_total_bytes: row.get(7)?,
            swap_free_bytes: row.get(8)?,
        })
    })?;
    let data: Vec<MemoryStat> = rows.collect::<Result<_>>()?;
    Ok((data, total))
}

pub fn init_power_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS power_stats (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            source TEXT NOT NULL,
            power_w REAL
        );
        CREATE INDEX IF NOT EXISTS idx_power_stats_timestamp ON power_stats(timestamp);",
    )
}

pub fn insert_power_stat(conn: &Connection, stat: &PowerStat) -> Result<i64> {
    let ts = stat.timestamp.to_string();
    conn.execute(
        "INSERT INTO power_stats (timestamp, source, power_w) VALUES (?1, ?2, ?3)",
        params![ts, stat.source, stat.power_w],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_latest_power_stat(conn: &Connection) -> Result<Option<PowerStat>> {
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, source, power_w FROM power_stats ORDER BY timestamp DESC LIMIT 1",
    )?;
    let row: Result<Option<PowerStat>> = stmt
        .query_map([], |row| {
            Ok(PowerStat {
                id: Some(row.get(0)?),
                timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
                source: row.get(2)?,
                power_w: row.get(3)?,
            })
        })?
        .into_iter()
        .next()
        .transpose();
    row
}

pub fn get_power_stats_paginated(conn: &Connection, limit: i32, offset: i32) -> Result<(Vec<PowerStat>, i64)> {
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM power_stats", [], |row| row.get(0))?;
    let mut stmt = conn.prepare(
        "SELECT id, timestamp, source, power_w FROM power_stats ORDER BY timestamp ASC LIMIT ?1 OFFSET ?2",
    )?;
    let rows = stmt.query_map(params![limit, offset], |row| {
        Ok(PowerStat {
            id: Some(row.get(0)?),
            timestamp: row.get::<_, String>(1)?.parse().unwrap_or(0),
            source: row.get(2)?,
            power_w: row.get(3)?,
        })
    })?;
    let data: Vec<PowerStat> = rows.collect::<Result<_>>()?;
    Ok((data, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_memory_table(&conn).unwrap();
        init_thermal_table(&conn).unwrap();
        init_compute_table(&conn).unwrap();
        conn
    }

    fn sample_stat() -> MemoryStat {
        MemoryStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            mem_total_bytes: Some(16384000),
            mem_free_bytes: Some(2048000),
            mem_available_bytes: Some(8192000),
            buffers_bytes: Some(512000),
            cached_bytes: Some(4096000),
            swap_total_bytes: Some(0),
            swap_free_bytes: Some(0),
        }
    }

    #[test]
    fn test_insert_memory_stat() {
        let conn = test_db();
        let stat = sample_stat();
        let id = insert_memory_stat(&conn, &stat).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_get_latest_memory_stat() {
        let conn = test_db();
        let stat = sample_stat();
        insert_memory_stat(&conn, &stat).unwrap();
        let latest = get_latest_memory_stat(&conn).unwrap();
        assert!(latest.is_some());
        let latest = latest.unwrap();
        assert_eq!(latest.mem_total_bytes, Some(16384000));
    }

    #[test]
    fn test_get_memory_stats_paginated() {
        let conn = test_db();
        for _ in 0..15 {
            insert_memory_stat(&conn, &sample_stat()).unwrap();
        }
        let (data, total) = get_memory_stats_paginated(&conn, 10, 0).unwrap();
        assert_eq!(data.len(), 10);
        assert_eq!(total, 15);
    }

    #[test]
    fn test_get_memory_stats_paginated_offset() {
        let conn = test_db();
        for _ in 0..15 {
            insert_memory_stat(&conn, &sample_stat()).unwrap();
        }
        let (data, total) = get_memory_stats_paginated(&conn, 10, 10).unwrap();
        assert_eq!(data.len(), 5);
        assert_eq!(total, 15);
    }

    #[test]
    fn test_get_memory_stats_paginated_exceeds_bounds() {
        let conn = test_db();
        for _ in 0..5 {
            insert_memory_stat(&conn, &sample_stat()).unwrap();
        }
        let (data, total) = get_memory_stats_paginated(&conn, 10, 0).unwrap();
        assert_eq!(data.len(), 5);
        assert_eq!(total, 5);
    }

    #[test]
    fn test_get_memory_stats_empty() {
        let conn = test_db();
        let (data, total) = get_memory_stats_paginated(&conn, 10, 0).unwrap();
        assert_eq!(data.len(), 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn test_create_table_if_not_exists() {
        let conn = Connection::open_in_memory().unwrap();
        init_memory_table(&conn).unwrap();
        init_memory_table(&conn).unwrap();
    }

    fn sample_thermal_stat() -> ThermalStat {
        ThermalStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            zone: "0".to_string(),
            sensor_type: "acpitz".to_string(),
            temperature_celsius: Some(65.4),
            trip_point_type: Some("critical".to_string()),
            trip_point_temp_celsius: Some(105.0),
            sensor_label: Some("Zone 0".to_string()),
        }
    }

    #[test]
    fn test_insert_thermal_stat() {
        let conn = test_db();
        let stat = sample_thermal_stat();
        let id = insert_thermal_stat(&conn, &stat).unwrap();
        assert!(id > 0);
    }

    #[test]
    fn test_get_latest_thermal_stats() {
        let conn = test_db();
        let stat = sample_thermal_stat();
        insert_thermal_stat(&conn, &stat).unwrap();
        let latest = get_latest_thermal_stats(&conn).unwrap();
        assert!(!latest.is_empty());
        assert_eq!(latest[0].temperature_celsius, Some(65.4));
        assert_eq!(latest[0].trip_point_type, Some("critical".to_string()));
        assert_eq!(latest[0].trip_point_temp_celsius, Some(105.0));
        assert_eq!(latest[0].sensor_label, Some("Zone 0".to_string()));
    }

    #[test]
    fn test_get_latest_thermal_stats_returns_only_newest_batch() {
        // "current" must show the newest coherent batch. Older batches are
        // ignored even when they hold more rows. (The collector's per-second
        // guard guarantees at most one batch per timestamp second, so the
        // reader never has to disambiguate between two same-second batches.)
        let conn = test_db();
        let old = "1700000100".to_string(); // older batch, 2 sensors
        for (stype, zone) in [("acpitz", "0"), ("nvidia", "gpu")] {
            let stat = ThermalStat {
                id: None,
                timestamp: old.parse().unwrap(),
                zone: zone.to_string(),
                sensor_type: stype.to_string(),
                temperature_celsius: Some(50.0),
                trip_point_type: None,
                trip_point_temp_celsius: None,
                sensor_label: Some(zone.to_string()),
            };
            insert_thermal_stat(&conn, &stat).unwrap();
        }
        let new = "1700000200".to_string(); // newer batch, 1 sensor
        let stat = ThermalStat {
            id: None,
            timestamp: new.parse().unwrap(),
            zone: "7".to_string(),
            sensor_type: "acpitz".to_string(),
            temperature_celsius: Some(90.0),
            trip_point_type: None,
            trip_point_temp_celsius: None,
            sensor_label: Some("SoC zone 7".to_string()),
        };
        insert_thermal_stat(&conn, &stat).unwrap();

        let latest = get_latest_thermal_stats(&conn).unwrap();
        assert_eq!(latest.len(), 1, "only the newest batch is returned");
        assert_eq!(latest[0].temperature_celsius, Some(90.0));
    }

    #[test]
    fn test_get_thermal_stats_paginated() {
        let conn = test_db();
        for _ in 0..15 {
            insert_thermal_stat(&conn, &sample_thermal_stat()).unwrap();
        }
        let (data, total) = get_thermal_stats_paginated(&conn, 10, 0).unwrap();
        assert_eq!(data.len(), 10);
        assert_eq!(total, 15);
    }

    #[test]
    fn test_get_thermal_stats_empty() {
        let conn = test_db();
        let stats = get_latest_thermal_stats(&conn).unwrap();
        assert!(stats.is_empty());
    }

    fn sample_compute_stat() -> ComputeStat {
        ComputeStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            cpu_utilization_pct: Some(62.5),
            gpu_utilization_pct: Some(41.0),
            gpu_memory_utilization_pct: Some(12.0),
            load_1: Some(0.61),
            load_5: Some(0.7),
            load_15: Some(0.63),
            core_count: Some(20),
            cores: vec![
                CoreStat { index: 0, utilization_pct: 55.0 },
                CoreStat { index: 1, utilization_pct: 70.0 },
            ],
            gpu_name: Some("NVIDIA GB10".to_string()),
        }
    }

    #[test]
    fn test_compute_roundtrip() {
        let conn = test_db();
        let stat = sample_compute_stat();
        insert_compute_stat(&conn, &stat).unwrap();

        let latest = get_latest_compute_stat(&conn).unwrap().unwrap();
        assert_eq!(latest.cpu_utilization_pct, Some(62.5));
        assert_eq!(latest.gpu_utilization_pct, Some(41.0));
        assert_eq!(latest.gpu_name.as_deref(), Some("NVIDIA GB10"));
        assert_eq!(latest.cores.len(), 2);
        assert_eq!(latest.cores[0].index, 0);
        assert_eq!(latest.cores[1].utilization_pct, 70.0);

        let (data, total) = get_compute_stats_paginated(&conn, 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(data.len(), 1);
    }

    #[test]
    fn test_get_compute_stats_empty() {
        let conn = test_db();
        assert!(get_latest_compute_stat(&conn).unwrap().is_none());
        let (data, total) = get_compute_stats_paginated(&conn, 10, 0).unwrap();
        assert!(data.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_compute_window_averages() {
        let conn = test_db();
        let fresh = sample_compute_stat(); // gpu 41.0, load_1 0.61, ts = now
        insert_compute_stat(&conn, &fresh).unwrap();

        // All windows contain the single fresh sample.
        for minutes in [1i64, 5, 15, 60] {
            let gpu = compute_avg_gpu(&conn, minutes);
            assert!((gpu - 41.0).abs() < 1e-9, "gpu {minutes}m");
            let load = compute_avg_load_1(&conn, minutes);
            assert!((load - 0.61).abs() < 1e-9, "load_1 {minutes}m");
        }

        // A sample 2 minutes old: outside the 1-min window, inside 5/15/60.
        let old = sample_compute_stat_old(120); // gpu 55.0, load_1 2.0, ts = now-120s
        insert_compute_stat(&conn, &old).unwrap();

        assert!(compute_avg_gpu(&conn, 1).abs_sub(41.0) < 1e-9);
        assert!(compute_avg_load_1(&conn, 1).abs_sub(0.61) < 1e-9);
        // 5/15/60 now average over both samples.
        assert!(compute_avg_gpu(&conn, 5).abs_sub((41.0 + 55.0) / 2.0) < 1e-9);
        assert!(compute_avg_load_1(&conn, 5).abs_sub((0.61 + 2.0) / 2.0) < 1e-9);
    }

    fn compute_avg_gpu(conn: &Connection, minutes: i64) -> f64 {
        gpu_utilization_avg_last_minutes(conn, minutes).unwrap().unwrap()
    }

    fn compute_avg_load_1(conn: &Connection, minutes: i64) -> f64 {
        load_1_avg_last_minutes(conn, minutes).unwrap().unwrap()
    }

    fn sample_compute_stat_old(secs_ago: i64) -> ComputeStat {
        ComputeStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp() - secs_ago,
            cpu_utilization_pct: Some(50.0),
            gpu_utilization_pct: Some(55.0),
            gpu_memory_utilization_pct: Some(10.0),
            load_1: Some(2.0),
            load_5: Some(2.0),
            load_15: Some(2.0),
            core_count: Some(20),
            cores: vec![],
            gpu_name: Some("NVIDIA GB10".to_string()),
        }
    }

    // -- power -----------------------------------------------------------------

    fn sample_power_stat() -> PowerStat {
        PowerStat {
            id: None,
            timestamp: chrono::Utc::now().timestamp(),
            source: "nvidia-smi power.draw".to_string(),
            power_w: Some(39.04),
        }
    }

    #[test]
    fn test_power_roundtrip() {
        let conn = test_db();
        init_power_table(&conn).unwrap();
        let stat = sample_power_stat();
        let id = insert_power_stat(&conn, &stat).unwrap();
        assert!(id > 0);

        let latest = get_latest_power_stat(&conn).unwrap().unwrap();
        assert_eq!(latest.power_w, Some(39.04));
        assert_eq!(latest.source, "nvidia-smi power.draw");

        let (data, total) = get_power_stats_paginated(&conn, 10, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(data.len(), 1);
    }

    #[test]
    fn test_power_latest_prefers_newest() {
        let conn = test_db();
        init_power_table(&conn).unwrap();
        let mut old = sample_power_stat();
        old.timestamp = chrono::Utc::now().timestamp() - 1000;
        old.power_w = Some(10.0);
        insert_power_stat(&conn, &old).unwrap();

        let mut fresh = sample_power_stat();
        fresh.power_w = Some(99.0);
        insert_power_stat(&conn, &fresh).unwrap();

        let latest = get_latest_power_stat(&conn).unwrap().unwrap();
        assert_eq!(latest.power_w, Some(99.0));
    }

    #[test]
    fn test_power_get_empty() {
        let conn = test_db();
        init_power_table(&conn).unwrap();
        assert!(get_latest_power_stat(&conn).unwrap().is_none());
        let (data, total) = get_power_stats_paginated(&conn, 10, 0).unwrap();
        assert!(data.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn test_prune_includes_power() {
        let conn = test_db();
        init_power_table(&conn).unwrap();
        let mut old = sample_power_stat();
        old.timestamp = chrono::Utc::now().timestamp() - (30 * 86_400);
        insert_power_stat(&conn, &old).unwrap();
        let summary = prune_history(&conn, 7).unwrap();
        assert_eq!(summary.power, 1);
        assert!(get_latest_power_stat(&conn).unwrap().is_none());
    }
}
