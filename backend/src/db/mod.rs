use rusqlite::{params, Connection, Result};

use crate::collector::{MemoryStat, ThermalStat};

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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_memory_table(&conn).unwrap();
        init_thermal_table(&conn).unwrap();
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
}
