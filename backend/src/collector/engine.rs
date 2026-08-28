use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::Connection;

use crate::collector::reader::{parse_proc_meminfo, read_proc_meminfo};
use crate::config::Config;
use crate::db;

pub struct Collector {
    config: Config,
    conn: Mutex<Connection>,
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
        Collector {
            config,
            conn: Mutex::new(conn),
        }
    }

    pub fn collect_memory(&self) -> Result<(), Box<dyn std::error::Error>> {
        let content = read_proc_meminfo().map_err(|e| format!("Failed to read /proc/meminfo: {}", e))?;
        let stat = parse_proc_meminfo(&content);

        let conn = self.conn.lock().unwrap();
        db::insert_memory_stat(&conn, &stat)?;

        Ok(())
    }

    pub fn run_loop(&self) {
        let interval = Duration::from_secs(self.config.memory.collection_interval_secs);
        loop {
            match self.collect_memory() {
                Ok(_) => {}
                Err(e) => eprintln!("spark-collect: collection error: {}", e),
            }
            std::thread::sleep(interval);
        }
    }
}
