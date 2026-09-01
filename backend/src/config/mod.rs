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
    pub compute: ComputeConfig,
    pub power: PowerConfig,
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

#[derive(Debug)]
pub struct ComputeConfig {
    pub collection_interval_secs: u64,
}

#[derive(Debug)]
pub struct PowerConfig {
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
    let mut compute_interval: u64 = 10;
    let mut power_interval: u64 = 10;

    let mut section = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, '=').collect();
        if parts.len() != 2 {
            continue;
        }
        let key = parts[0].trim();
        let value = parts[1].trim().trim_matches('"');

        let sec = section.as_str();
        match (sec, key) {
            ("server", "host") => server_host = value.to_string(),
            ("server", "port") => server_port = value.parse().unwrap_or(8090),
            ("collector", "ipc_socket") => ipc_socket = value.to_string(),
            ("database", "path") => db_path = value.to_string(),
            ("database", "wal_mode") => wal_mode = value.parse().unwrap_or(true),
            ("history", "retention_days") => retention_days = value.parse().unwrap_or(7),
            ("thermal", "collection_interval_secs") => {
                thermal_interval = value.parse().unwrap_or(30)
            }
            ("memory", "collection_interval_secs") => {
                memory_interval = value.parse().unwrap_or(60)
            }
            ("compute", "collection_interval_secs") => {
                compute_interval = value.parse().unwrap_or(10)
            }
            ("power", "collection_interval_secs") => {
                power_interval = value.parse().unwrap_or(10)
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
        compute: ComputeConfig {
            collection_interval_secs: compute_interval,
        },
        power: PowerConfig {
            collection_interval_secs: power_interval,
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
