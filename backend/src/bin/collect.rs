use spark_dashboard::collector::engine::Collector;
use spark_dashboard::config;
use spark_dashboard::ipc;
use spark_dashboard::lock::SingleInstanceLock;
use std::path::Path;
use std::sync::Arc;

fn main() {
    let config_path = std::env::var("SPARK_CONFIG_PATH")
        .unwrap_or_else(|_| "config/default.toml".to_string());
    let cfg = config::load_config(&config_path);

    // Enforce "one collector per database" via a cross-process `flock` on a file
    // in the database directory. This is the authoritative single-writer guard:
    // a second collector on the same DB is refused at startup (see src/lock.rs),
    // while a second one on a *different* database is allowed. The kernel frees
    // the lock automatically if this process dies, so no stale-lock recovery is
    // needed. `lock` is kept alive until the process exits.
    let db_dir = Path::new(&cfg.database.path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| Path::new(".").to_path_buf());
    // The lock handle must stay alive for the whole process (dropping it closes
    // the fd and releases the flock). We bind it to `_lock` — a real binding,
    // so it lives to the end of `main` — with the leading underscore only to
    // suppress the unused-variable warning.
    let _lock = SingleInstanceLock::acquire(&db_dir, "collector")
        .unwrap_or_else(|e| {
            eprintln!("spark-collect: refusing to start — {}", e.describe());
            std::process::exit(1);
        });

    println!("spark-collect: starting collector daemon");
    println!(
        "spark-collect: intervals memory={}s thermal={}s compute={}s retention={}d",
        cfg.memory.collection_interval_secs,
        cfg.thermal.collection_interval_secs,
        cfg.compute.collection_interval_secs,
        cfg.history.retention_days
    );
    let ipc_socket = cfg.collector.ipc_socket.clone();
    let collector = Arc::new(Collector::new(cfg));

    // Only the lock holder binds the IPC socket: if a second collector somehow
    // reached this point it would already have been refused above, so the socket
    // is the single collector's control endpoint. The listener runs until the
    // process exits (run.sh kills it via pkill on shutdown).
    let handler = {
        let c = Arc::clone(&collector);
        move |req| c.handle_ipc_request(req)
    };
    match ipc::spawn_listener(&ipc_socket, Box::new(handler)) {
        Ok(()) => println!("spark-collect: ipc listening on {}", ipc_socket),
        Err(e) => eprintln!("spark-collect: WARNING: ipc unavailable: {}", e),
    }

    // `lock` stays pinned for the lifetime of this function (i.e. the process);
    // it is released automatically on exit/crash by closing the fd.
    collector.run_loop();
}
