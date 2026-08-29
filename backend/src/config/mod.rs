use std::env;
use std::fs;
use std::path::Path;

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = env::var_os("HOME") {
            let home_path = Path::new(&home);
            return home_path.join(&path[2..]).to_string_lossy().into_owned();
        }
    }
    path.to_string()
}

#[derive(Debug)]
pub struct Config {
    pub server: ServerConfig,
    pub collector: CollectorConfig,
    pub database: DatabaseConfig,
    pub history: HistoryConfig,
    pub thermal: ThermalConfig,
    pub memory: MemoryConfig,
}

#[derive(Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug)]
pub struct CollectorConfig {
    pub ipc_socket: String,
}

#[derive(Debug)]
pub struct DatabaseConfig {
    pub path: String,
    pub wal_mode: bool,
}

#[derive(Debug)]
pub struct HistoryConfig {
    pub retention_days: u32,
}

#[derive(Debug)]
pub struct ThermalConfig {
    pub collection_interval_secs: u64,
}

#[derive(Debug)]
pub struct MemoryConfig {
    pub collection_interval_secs: u64,
}

pub fn load_config(path: &str) -> Config {
    let content = fs::read_to_string(path).unwrap_or_else(|_| {
        include_str!("../../config/default.toml").to_string()
    });
    parse_config(&content)
}

fn parse_config(content: &str) -> Config {
    let mut server_host = "127.0.0.1".to_string();
    let mut server_port: u16 = 8090;
    let mut ipc_socket = "/tmp/spark-collect.sock".to_string();
    let mut db_path = "~/.local/share/spark-dashboard/spark.db".to_string();
    let mut wal_mode = true;
    let mut retention_days: u32 = 7;
    let mut thermal_interval: u64 = 30;
    let mut memory_interval: u64 = 60;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            continue;
        }
        let key = parts[0].trim();
        let value = parts[1].trim().trim_matches('"');

        match key {
            "host" => server_host = value.to_string(),
            "port" => server_port = value.parse().unwrap_or(8090),
            "ipc_socket" => ipc_socket = value.to_string(),
            "path" => db_path = value.to_string(),
            "wal_mode" => wal_mode = value.parse().unwrap_or(true),
            "retention_days" => retention_days = value.parse().unwrap_or(7),
            "collection_interval_secs" => {
                if thermal_interval == 30 {
                    thermal_interval = value.parse().unwrap_or(30);
                } else {
                    memory_interval = value.parse().unwrap_or(60);
                }
            }
            _ => {}
        }
    }

    Config {
        server: ServerConfig {
            host: server_host,
            port: server_port,
        },
        collector: CollectorConfig {
            ipc_socket,
        },
        database: DatabaseConfig {
            path: expand_tilde(&db_path),
            wal_mode,
        },
        history: HistoryConfig {
            retention_days,
        },
        thermal: ThermalConfig {
            collection_interval_secs: thermal_interval,
        },
        memory: MemoryConfig {
            collection_interval_secs: memory_interval,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_default_config() {
        let config = parse_config("");
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8090);
        assert_eq!(config.memory.collection_interval_secs, 60);
    }

    #[test]
    fn test_config_missing_field() {
        let config = parse_config("");
        assert_eq!(config.thermal.collection_interval_secs, 30);
    }

    #[test]
    fn test_parse_config_with_values() {
        let content = r#"
[server]
host = "0.0.0.0"
port = 9090

[database]
path = "/tmp/test.db"
wal_mode = false
"#;
        let config = parse_config(content);
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9090);
        assert_eq!(config.database.path, "/tmp/test.db");
        assert!(!config.database.wal_mode);
    }

    #[test]
    fn test_expand_tilde() {
        let path = expand_tilde("~/test.db");
        assert!(path.starts_with("/home/"));
        assert!(path.ends_with("/test.db"));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let path = expand_tilde("/tmp/test.db");
        assert_eq!(path, "/tmp/test.db");
    }
}
