use std::sync::Arc;
use std::time::Duration;
use rusqlite::Connection;
use spark_dashboard::collector::reader;
use spark_dashboard::db;

#[test]
fn test_end_to_end_memory_collection() {
    let conn = Connection::open_in_memory().unwrap();
    db::init_memory_table(&conn).unwrap();

    let content = reader::read_proc_meminfo().unwrap();
    let stat = reader::parse_proc_meminfo(&content);
    db::insert_memory_stat(&conn, &stat).unwrap();

    let latest = db::get_latest_memory_stat(&conn).unwrap();
    assert!(latest.is_some());
    let latest = latest.unwrap();
    assert!(latest.mem_total_bytes.is_some());
    assert!(latest.mem_free_bytes.is_some());

    let (data, total) = db::get_memory_stats_paginated(&conn, 10, 0).unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(total, 1);
}

#[test]
fn test_end_to_end_memory_history() {
    let conn = Connection::open_in_memory().unwrap();
    db::init_memory_table(&conn).unwrap();

    for _ in 0..3 {
        let content = reader::read_proc_meminfo().unwrap();
        let stat = reader::parse_proc_meminfo(&content);
        db::insert_memory_stat(&conn, &stat).unwrap();
        std::thread::sleep(Duration::from_millis(10));
    }

    let (data, total) = db::get_memory_stats_paginated(&conn, 10, 0).unwrap();
    assert_eq!(data.len(), 3);
    assert_eq!(total, 3);

    for i in 1..data.len() {
        assert!(data[i].timestamp >= data[i-1].timestamp);
    }
}

#[test]
fn test_concurrent_api_reads() {
    let conn = Arc::new(std::sync::Mutex::new(
        Connection::open_in_memory().unwrap()
    ));
    db::init_memory_table(&conn.lock().unwrap()).unwrap();

    let content = reader::read_proc_meminfo().unwrap();
    let stat = reader::parse_proc_meminfo(&content);
    db::insert_memory_stat(&conn.lock().unwrap(), &stat).unwrap();

    let mut handles = vec![];
    for _ in 0..10 {
        let conn_clone = Arc::clone(&conn);
        let handle = std::thread::spawn(move || {
            let result = db::get_latest_memory_stat(&conn_clone.lock().unwrap());
            assert!(result.is_ok());
            result
        });
        handles.push(handle);
    }

    for handle in handles {
        let result = handle.join().unwrap();
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }
}
