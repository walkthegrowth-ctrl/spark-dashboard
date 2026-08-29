use spark_dashboard::server::handler;

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
                    config_path = Some(&args[i]);
                }
            }
            _ => {}
        }
        i += 1;
    }

    let cfg = if let Some(cp) = config_path {
        spark_dashboard::config::load_config(cp)
    } else {
        spark_dashboard::config::load_config("config/default.toml")
    };

    let host = cfg.server.host;
    let server_port = port.unwrap_or(cfg.server.port);
    let db_path = cfg.database.path;

    println!("spark-serve: starting on {host}:{server_port}");
    handler::start_server(&host, server_port, &db_path);
}
