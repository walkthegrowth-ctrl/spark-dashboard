use spark_dashboard::server::handler;

fn main() {
    let host = std::env::var("SPARK_API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("SPARK_API_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);
    let db_path = std::env::var("SPARK_DB_PATH")
        .unwrap_or_else(|_| "~/.local/share/spark-dashboard/spark.db".to_string());
    println!("spark-serve: starting on {host}:{port}");
    handler::start_server(&host, port, &db_path);
}
