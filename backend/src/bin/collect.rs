use spark_dashboard::collector::engine::Collector;
use spark_dashboard::config;

fn main() {
    let config_path = std::env::var("SPARK_CONFIG_PATH")
        .unwrap_or_else(|_| "config/default.toml".to_string());
    let cfg = config::load_config(&config_path);
    println!("spark-collect: starting collector daemon");
    println!("spark-collect: collection interval: {}s", cfg.memory.collection_interval_secs);
    let collector = Collector::new(cfg);
    collector.run_loop();
}
