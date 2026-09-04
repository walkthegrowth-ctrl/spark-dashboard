use spark_dashboard::config;
use spark_dashboard::lock::SingleInstanceLock;
use spark_dashboard::server::handler;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let mut port: Option<u16> = None;
    let mut config_path: Option<&str> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().ok();
                }
            }
            "--config" => {
                i += 1;
                if i < args.len() {
                    let next = args[i].as_str();
                    config_path = Some(next);
                }
            }
            _ => {}
        }
        i += 1;
    }

    let cfg = if let Some(cp) = config_path {
        config::load_config(cp)
    } else {
        // `SPARK_CONFIG_PATH` keeps both binaries consistent (see collect.rs);
        // the `--config` flag above still wins over the environment variable.
        config::load_config(
            std::env::var("SPARK_CONFIG_PATH")
                .as_deref()
                .unwrap_or("config/default.toml"),
        )
    };

    // Enforce "one server per database" via a cross-process `flock` on a file in
    // the database directory. A second `spark-serve` — even one launched with a
    // different `--port` — is refused, so there is exactly one HTTP endpoint and
    // one DB reader per database. Frontends are unaffected: they are HTTP
    // *clients* of that single server, so any number of (local or remote)
    // browser tabs can connect concurrently. The kernel frees the lock on exit,
    // so no stale-lock recovery is needed.
    let db_dir = Path::new(&cfg.database.path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    // `lock` must stay alive for the whole process; the `_` prefix just
    // silences the "unused variable" warning — it is still a real local that
    // Rust drops at end-of-scope (not immediately).
    let _lock = SingleInstanceLock::acquire(&db_dir, "server")
        .unwrap_or_else(|e| {
            eprintln!("spark-serve: refusing to start — {}", e.describe());
            std::process::exit(1);
        });

    let host = cfg.server.host;
    let server_port = port.unwrap_or(cfg.server.port);
    let db_path = cfg.database.path;
    let ipc_socket = cfg.collector.ipc_socket;

    println!("spark-serve: starting on {host}:{server_port}");
    // `_lock` stays pinned for the lifetime of `start_server` (i.e. the
    // process); the kernel releases the flock when the process exits.
    handler::start_server(&host, server_port, &db_path, &ipc_socket);
}
