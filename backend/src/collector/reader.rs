use std::fs;

use crate::collector::MemoryStat;

pub fn parse_proc_meminfo(content: &str) -> MemoryStat {
    let mut mem_total: Option<i64> = None;
    let mut mem_free: Option<i64> = None;
    let mut mem_available: Option<i64> = None;
    let mut buffers: Option<i64> = None;
    let mut cached: Option<i64> = None;
    let mut swap_total: Option<i64> = None;
    let mut swap_free: Option<i64> = None;

    for line in content.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() != 2 {
            continue;
        }
        let key = parts[0].trim();
        let value_str = parts[1].trim().split_whitespace().next();
        let value = value_str.and_then(|v| v.parse::<u64>().ok()).map(|v| (v as i64) * 1024);

        match key {
            "MemTotal" => mem_total = value,
            "MemFree" => mem_free = value,
            "MemAvailable" => mem_available = value,
            "Buffers" => buffers = value,
            "Cached" => cached = value,
            "SwapTotal" => swap_total = value,
            "SwapFree" => swap_free = value,
            _ => {}
        }
    }

    MemoryStat {
        id: None,
        timestamp: chrono::Utc::now().timestamp(),
        mem_total_bytes: mem_total,
        mem_free_bytes: mem_free,
        mem_available_bytes: mem_available,
        buffers_bytes: buffers,
        cached_bytes: cached,
        swap_total_bytes: swap_total,
        swap_free_bytes: swap_free,
    }
}

pub fn read_proc_meminfo() -> Result<String, std::io::Error> {
    fs::read_to_string("/proc/meminfo")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_meminfo() -> String {
        "MemTotal:       16384000 kB\n\
         MemFree:         2048000 kB\n\
         MemAvailable:    8192000 kB\n\
         Buffers:          512000 kB\n\
         Cached:           4096000 kB\n\
         SwapTotal:             0 kB\n\
         SwapFree:              0 kB"
            .to_string()
    }

    #[test]
    fn test_parse_proc_meminfo_happy_path() {
        let stat = parse_proc_meminfo(&sample_meminfo());
        assert_eq!(stat.mem_total_bytes, Some(16384000 * 1024));
        assert_eq!(stat.mem_free_bytes, Some(2048000 * 1024));
        assert_eq!(stat.mem_available_bytes, Some(8192000 * 1024));
        assert_eq!(stat.buffers_bytes, Some(512000 * 1024));
        assert_eq!(stat.cached_bytes, Some(4096000 * 1024));
        assert_eq!(stat.swap_total_bytes, Some(0));
        assert_eq!(stat.swap_free_bytes, Some(0));
    }

    #[test]
    fn test_parse_proc_meminfo_missing_field() {
        let content = "MemTotal:       16384000 kB\nMemFree:         2048000 kB".to_string();
        let stat = parse_proc_meminfo(&content);
        assert_eq!(stat.mem_total_bytes, Some(16384000 * 1024));
        assert_eq!(stat.mem_free_bytes, Some(2048000 * 1024));
        assert_eq!(stat.mem_available_bytes, None);
        assert_eq!(stat.buffers_bytes, None);
        assert_eq!(stat.cached_bytes, None);
    }

    #[test]
    fn test_parse_proc_meminfo_malformed_value() {
        let content = "MemTotal:       abc kB\nMemFree:         2048000 kB".to_string();
        let stat = parse_proc_meminfo(&content);
        assert_eq!(stat.mem_total_bytes, None);
        assert_eq!(stat.mem_free_bytes, Some(2048000 * 1024));
    }

    #[test]
    fn test_parse_proc_meminfo_empty_input() {
        let stat = parse_proc_meminfo("");
        assert_eq!(stat.mem_total_bytes, None);
        assert_eq!(stat.mem_free_bytes, None);
        assert_eq!(stat.mem_available_bytes, None);
        assert_eq!(stat.buffers_bytes, None);
        assert_eq!(stat.cached_bytes, None);
        assert_eq!(stat.swap_total_bytes, None);
        assert_eq!(stat.swap_free_bytes, None);
    }

    #[test]
    fn test_parse_proc_meminfo_extra_fields() {
        let content = "MemTotal:       16384000 kB\n\
         SomeUnknownField:  999999 kB\n\
         MemFree:         2048000 kB".to_string();
        let stat = parse_proc_meminfo(&content);
        assert_eq!(stat.mem_total_bytes, Some(16384000 * 1024));
        assert_eq!(stat.mem_free_bytes, Some(2048000 * 1024));
    }
}
